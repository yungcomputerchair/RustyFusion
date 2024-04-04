#![allow(clippy::needless_range_loop)]

use std::{
    collections::{HashMap, HashSet},
    fmt::{Display, Formatter},
};

use crate::{
    config::config_get,
    defines::{ID_OVERWORLD, MAX_NUM_CHANNELS},
    entity::{Entity, EntityID, Player, Slider, NPC},
    enums::ShardChannelStatus,
    error::{log, log_if_failed, panic_log, FFError, FFResult, Severity},
    net::{ClientMap, FFClient},
    Position,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstanceID {
    pub channel_num: usize,
    pub map_num: u32,
    pub instance_num: Option<u32>,
}
impl Default for InstanceID {
    fn default() -> Self {
        Self {
            channel_num: 1,
            map_num: ID_OVERWORLD,
            instance_num: None,
        }
    }
}
impl Display for InstanceID {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}/{}:{}",
            self.channel_num,
            self.map_num,
            self.instance_num
                .map(|id| id.to_string())
                .unwrap_or("None".to_string())
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkCoords {
    pub i: InstanceID,
    pub x: i32,
    pub y: i32,
}
impl ChunkCoords {
    pub fn from_pos_inst(pos: Position, instance_id: InstanceID) -> Self {
        Self {
            x: (pos.x * NCHUNKS as i32) / MAP_BOUNDS,
            y: (pos.y * NCHUNKS as i32) / MAP_BOUNDS,
            i: instance_id,
        }
    }
}
impl Display for ChunkCoords {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {})", self.x, self.y, self.i)
    }
}

pub const MAP_SQUARE_COUNT: i32 = 16; // how many map squares there are in each direction
pub const NCHUNKS: usize = MAP_SQUARE_COUNT as usize * 8; // 16 map squares with side lengths of 8 chunks
pub const MAP_SQUARE_SIZE: i32 = 51200;
pub const MAP_BOUNDS: i32 = MAP_SQUARE_SIZE * MAP_SQUARE_COUNT; // top corner of (16, 16)

fn get_visibility_range() -> usize {
    config_get().shard.visibility_range.get()
}

struct RegistryEntry {
    entity: Box<dyn Entity>,
    chunk: Option<ChunkCoords>,
    tick: bool,
}

struct ChunkMap {
    player_count: usize,
    chunks: [[Chunk; NCHUNKS]; NCHUNKS],
}
impl ChunkMap {
    fn get_ids(&self) -> Vec<EntityID> {
        self.chunks
            .iter()
            .flatten()
            .flat_map(|chunk| chunk.get_all())
            .cloned()
            .collect()
    }
}

pub struct EntityMap {
    registry: HashMap<EntityID, RegistryEntry>,
    chunk_maps: HashMap<InstanceID, ChunkMap>,
    next_pc_id: u32,
    next_npc_id: u32,
    next_slider_id: u32,
}

impl EntityMap {
    pub fn get_from_id(&self, id: EntityID) -> Option<&dyn Entity> {
        self.registry.get(&id).map(|entry| entry.entity.as_ref())
    }

    pub fn get_from_id_mut(&mut self, id: EntityID) -> Option<&mut dyn Entity> {
        // compiler doesn't like the use of a closure here
        match self.registry.get_mut(&id) {
            Some(entry) => Some(entry.entity.as_mut()),
            None => None,
        }
    }

    pub fn get_all_ids(&self) -> impl Iterator<Item = EntityID> + '_ {
        self.registry.keys().cloned()
    }

    pub fn get_tickable_ids(&self) -> impl Iterator<Item = EntityID> + '_ {
        self.registry
            .iter()
            .filter_map(|(id, entry)| if entry.tick { Some(*id) } else { None })
    }

    pub fn get_around_entity(&mut self, id: EntityID) -> HashSet<EntityID> {
        if let Some(coords) = self.registry.get(&id).and_then(|entry| entry.chunk) {
            self.get_around(coords, get_visibility_range())
        } else {
            HashSet::new()
        }
    }

    pub fn get_player(&self, pc_id: i32) -> Option<&Player> {
        let id = EntityID::Player(pc_id);
        self.registry.get(&id).and_then(|entry| {
            let entity_ref = entry.entity.as_ref().as_any();
            let player_ref = entity_ref.downcast_ref();
            player_ref
        })
    }

    pub fn get_player_mut(&mut self, pc_id: i32) -> Option<&mut Player> {
        let id = EntityID::Player(pc_id);
        self.registry.get_mut(&id).and_then(|entry| {
            let entity_ref = entry.entity.as_mut().as_any_mut();
            let player_ref = entity_ref.downcast_mut();
            player_ref
        })
    }

    pub fn get_player_ids(&self) -> impl Iterator<Item = i32> + '_ {
        self.registry.keys().filter_map(|id| {
            if let EntityID::Player(pc_id) = id {
                Some(*pc_id)
            } else {
                None
            }
        })
    }

    pub fn find_players(&self, f: impl Fn(&Player) -> bool) -> Vec<i32> {
        self.registry
            .values()
            .filter_map(|entry| {
                let entity_id = entry.entity.get_id();
                if let EntityID::Player(pc_id) = entity_id {
                    let pc = self.get_player(pc_id).unwrap();
                    if f(pc) {
                        return Some(pc_id);
                    }
                }
                None
            })
            .collect()
    }

    pub fn get_npc(&self, npc_id: i32) -> Option<&NPC> {
        let id = EntityID::NPC(npc_id);
        self.registry.get(&id).and_then(|entry| {
            let entity_ref = entry.entity.as_ref().as_any();
            let npc_ref = entity_ref.downcast_ref();
            npc_ref
        })
    }

    pub fn get_npc_mut(&mut self, npc_id: i32) -> Option<&mut NPC> {
        let id = EntityID::NPC(npc_id);
        self.registry.get_mut(&id).and_then(|entry| {
            let entity_ref = entry.entity.as_mut().as_any_mut();
            let npc_ref = entity_ref.downcast_mut();
            npc_ref
        })
    }

    pub fn get_npc_ids(&self) -> impl Iterator<Item = i32> + '_ {
        self.registry.keys().filter_map(|id| {
            if let EntityID::NPC(npc_id) = id {
                Some(*npc_id)
            } else {
                None
            }
        })
    }

    pub fn find_npcs(&self, f: impl Fn(&NPC) -> bool) -> Vec<i32> {
        self.registry
            .values()
            .filter_map(|entry| {
                let entity_id = entry.entity.get_id();
                if let EntityID::NPC(npc_id) = entity_id {
                    let npc = self.get_npc(npc_id).unwrap();
                    if f(npc) {
                        return Some(npc_id);
                    }
                }
                None
            })
            .collect()
    }

    pub fn get_slider(&self, slider_id: i32) -> Option<&Slider> {
        let id = EntityID::Slider(slider_id);
        self.registry.get(&id).and_then(|entry| {
            let entity_ref = entry.entity.as_ref().as_any();
            let slider_ref = entity_ref.downcast_ref();
            slider_ref
        })
    }

    pub fn get_slider_mut(&mut self, slider_id: i32) -> Option<&mut Slider> {
        let id = EntityID::Slider(slider_id);
        self.registry.get_mut(&id).and_then(|entry| {
            let entity_ref = entry.entity.as_mut().as_any_mut();
            let slider_ref = entity_ref.downcast_mut();
            slider_ref
        })
    }

    pub fn validate_proximity(&self, ids: &[EntityID], range: u32) -> FFResult<()> {
        let mut locations = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(entry) = self.registry.get(id) {
                locations.push((
                    entry.entity.get_position(),
                    entry.entity.get_chunk_coords().i,
                ));
            } else {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("Entity with ID {:?} doesn't exist", id),
                ));
            }
        }

        for i in 0..locations.len() {
            for j in (i + 1)..locations.len() {
                let inst1 = locations[i].1;
                let inst2 = locations[j].1;
                if inst1 != inst2 {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Entity with ID {:?} is in a different instance than entity with ID {:?} ({} != {})",
                            ids[i], ids[j], inst1, inst2
                        ),
                    ));
                }

                let pos1 = locations[i].0;
                let pos2 = locations[j].0;
                let distance = pos1.distance_to(&pos2);
                if distance > range {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Entity with ID {:?} is too far from entity with ID {:?} ({} > {})",
                            ids[i], ids[j], distance, range
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn gen_next_pc_id(&mut self) -> i32 {
        let id = self.next_pc_id;
        if id == u32::MAX {
            panic_log("Ran out of PC IDs");
        }
        self.next_pc_id += 1;
        id as i32
    }

    pub fn gen_next_npc_id(&mut self) -> i32 {
        let id = self.next_npc_id;
        if id == u32::MAX {
            panic_log("Ran out of NPC IDs");
        }
        self.next_npc_id += 1;
        id as i32
    }

    pub fn gen_next_slider_id(&mut self) -> i32 {
        let id = self.next_slider_id;
        if id == u32::MAX {
            panic_log("Ran out of slider IDs");
        }
        self.next_slider_id += 1;
        id as i32
    }

    pub fn track(&mut self, entity: Box<dyn Entity>, tick: bool) -> EntityID {
        let id = entity.get_id();
        if self.registry.contains_key(&id) {
            panic_log(&format!("Already tracking entity with id {:?}", id));
        }
        let entry = RegistryEntry {
            entity,
            chunk: None,
            tick,
        };
        self.registry.insert(id, entry);
        id
    }

    pub fn untrack(&mut self, id: EntityID) -> Box<dyn Entity> {
        self.registry
            .remove(&id)
            .unwrap_or_else(|| {
                panic_log(&format!("Entity with id {:?} already untracked", id));
            })
            .entity
    }

    pub fn update(
        &mut self,
        id: EntityID,
        to_chunk: Option<ChunkCoords>,
        client_map: Option<&mut ClientMap>,
    ) {
        let entry = self.registry.get_mut(&id).unwrap_or_else(|| {
            panic_log(&format!("Entity with id {:?} untracked", id));
        });
        let from_chunk = entry.chunk;
        if from_chunk == to_chunk {
            return;
        }

        let around_from = self.remove_from_chunk(id);
        let around_to = self.insert_into_chunk(id, to_chunk);
        if let Some(coords) = from_chunk {
            if to_chunk.is_none() {
                self.check_instance_for_cleanup(coords.i);
            }
        }

        // if there's no client map, nobody needs to be notified
        if client_map.is_none() {
            return;
        }
        let client_map = client_map.unwrap();

        let removed = around_from.difference(&around_to);
        for e in removed {
            // us to them
            let from = self.get_from_id(id).unwrap();
            if let Some(from_client) = from.get_client(client_map) {
                // possible for the ID to be unregistered if the instance was cleaned up
                if let Some(to) = self.get_from_id(*e) {
                    log_if_failed(to.send_exit(from_client));
                }
            }

            // them to us
            // possible for the ID to be unregistered if the instance was cleaned up
            if let Some(from) = self.get_from_id(*e) {
                if let Some(from_client) = from.get_client(client_map) {
                    let to = self.get_from_id(id).unwrap();
                    log_if_failed(to.send_exit(from_client));
                }
            }
        }

        let added = around_to.difference(&around_from);
        for e in added {
            // us to them
            let from = self.get_from_id(id).unwrap();
            if let Some(from_client) = from.get_client(client_map) {
                let to = self.get_from_id(*e).unwrap();
                log_if_failed(to.send_enter(from_client));
            }

            // them to us
            let from = self.get_from_id(*e).unwrap();
            if let Some(from_client) = from.get_client(client_map) {
                let to = self.get_from_id(id).unwrap();
                log_if_failed(to.send_enter(from_client));
            }
        }

        // if let EntityID::Player(pc_id) = id {
        //     log(
        //         Severity::Debug,
        //         &match self.registry[&id].chunk {
        //             Some(coords) => format!("Moved {} to {}", pc_id, coords),
        //             None => format!("Removed {} from map", pc_id),
        //         },
        //     );
        // }
    }

    pub fn set_tick(&mut self, id: EntityID, tick: bool) -> FFResult<()> {
        let entry = self.registry.get_mut(&id).ok_or(FFError::build(
            Severity::Warning,
            format!("Entity with ID {:?} doesn't exist", id),
        ))?;
        entry.tick = tick;
        Ok(())
    }

    pub fn for_each_around(
        &mut self,
        id: EntityID,
        clients: &mut ClientMap,
        mut f: impl FnMut(&mut FFClient) -> FFResult<()>,
    ) {
        for eid in self.get_around_entity(id).iter() {
            let e = self.registry.get_mut(eid).unwrap().entity.as_mut();
            if let Some(c) = e.get_client(clients) {
                log_if_failed(f(c));
            }
        }
    }

    pub fn get_channel_population(&self, channel_num: usize) -> usize {
        self.chunk_maps
            .iter()
            .filter(|(instance_id, _)| instance_id.channel_num == channel_num)
            .map(|(_, chunk_map)| chunk_map.player_count)
            .sum()
    }

    pub fn get_min_pop_channel_num(&self) -> usize {
        let num_channels = config_get().shard.num_channels.get();
        (1..=num_channels)
            .min_by_key(|channel_num| self.get_channel_population(*channel_num))
            .unwrap()
    }

    pub fn get_channel_statuses(&self) -> [ShardChannelStatus; MAX_NUM_CHANNELS] {
        let mut statuses = [ShardChannelStatus::Closed; MAX_NUM_CHANNELS];
        let num_channels = config_get().shard.num_channels.get();
        for channel_num in 1..=num_channels {
            statuses[channel_num - 1] = self.get_channel_status(channel_num);
        }
        statuses
    }

    fn get_channel_status(&self, channel_num: usize) -> ShardChannelStatus {
        let max_pop = config_get().shard.max_channel_pop.get();
        let pop = self.get_channel_population(channel_num);
        if pop >= max_pop {
            ShardChannelStatus::Closed
        } else {
            let pop_fraction = pop as f64 / max_pop as f64;
            if pop_fraction >= 0.75 {
                ShardChannelStatus::Busy
            } else if pop_fraction >= 0.25 {
                ShardChannelStatus::Normal
            } else {
                ShardChannelStatus::Empty
            }
        }
    }

    fn remove_from_chunk(&mut self, id: EntityID) -> HashSet<EntityID> {
        let mut affected = HashSet::new();
        let entry = self.registry.get_mut(&id).unwrap();
        if let Some(coords) = entry.chunk {
            entry.chunk = None;
            let chunk = self.get_chunk(coords).unwrap();
            if !chunk.remove(id) {
                panic_log(&format!(
                    "Chunk {:?} did not contain entity with ID {:?}",
                    coords, id
                ));
            }
            if let EntityID::Player(_) = id {
                self.instance_player_exit(coords.i);
            }
            affected.extend(self.get_around(coords, get_visibility_range()));
        }
        affected
    }

    fn insert_into_chunk(
        &mut self,
        id: EntityID,
        to_chunk: Option<ChunkCoords>,
    ) -> HashSet<EntityID> {
        let mut affected = HashSet::new();
        if let Some(coords) = to_chunk {
            if let Some(chunk) = self.get_chunk(coords) {
                if !chunk.insert(id) {
                    panic_log(&format!(
                        "Chunk {:?} already contained entity with ID {:?}",
                        coords, id
                    ));
                }
                if let EntityID::Player(_) = id {
                    self.instance_player_enter(coords.i);
                }
                let entry = self.registry.get_mut(&id).unwrap();
                entry.chunk = to_chunk;
                affected.extend(self.get_around(coords, get_visibility_range()));
                affected.remove(&id); // we don't want ourself in this
            }
        }
        affected
    }

    fn get_chunk(&mut self, coords: ChunkCoords) -> Option<&mut Chunk> {
        if (0..NCHUNKS as i32).contains(&coords.x) && (0..NCHUNKS as i32).contains(&coords.y) {
            let chunk_map = self.init_instance(coords.i);
            let chunk = &mut chunk_map.chunks[coords.x as usize][coords.y as usize];
            return Some(chunk);
        }
        None
    }

    fn get_around(&mut self, coords: ChunkCoords, range: usize) -> HashSet<EntityID> {
        let range = range as i32;
        let mut entities = HashSet::new();
        for x in (coords.x - range)..=(coords.x + range) {
            for y in (coords.y - range)..=(coords.y + range) {
                let coords = ChunkCoords { x, y, i: coords.i };
                if let Some(chunk) = self.get_chunk(coords) {
                    entities.extend(chunk.get_all());
                }
            }
        }
        entities
    }

    fn init_instance(&mut self, instance_id: InstanceID) -> &mut ChunkMap {
        let new = !self.chunk_maps.contains_key(&instance_id);
        self.chunk_maps.entry(instance_id).or_insert_with(|| {
            let chunk_map = ChunkMap {
                player_count: 0,
                chunks: std::array::from_fn(|_| std::array::from_fn(|_| Chunk::default())),
            };
            log(
                Severity::Debug,
                &format!("Initialized instance {}", instance_id),
            );
            chunk_map
        });
        if instance_id.instance_num.is_some() && new {
            let main_instance = InstanceID {
                channel_num: instance_id.channel_num,
                map_num: instance_id.map_num,
                instance_num: None,
            };
            let mut npc_count = 0;
            let mut id_mappings = HashMap::new();
            let mut leader_mappings = HashMap::new();
            let mut follower_mappings = HashMap::new();
            let template_chunks = self.chunk_maps.get(&main_instance).unwrap().chunks.clone();
            for x in 0..NCHUNKS {
                for y in 0..NCHUNKS {
                    for id in template_chunks[x][y].get_all() {
                        let tick = self.registry[&id].tick;
                        if let EntityID::NPC(npc_id) = *id {
                            let mut npc = self.get_npc(npc_id).unwrap().clone();
                            npc.instance_id = instance_id;
                            let new_id = self.gen_next_npc_id();
                            id_mappings.insert(npc.id, new_id);
                            npc.id = new_id;

                            // since there's no guarantee on what order the NPCs will be iterated upon,
                            // we update leader/follower ids after everyone is cloned
                            if !npc.follower_ids.is_empty() {
                                follower_mappings.insert(new_id, npc.follower_ids.clone());
                            }
                            npc.follower_ids.clear();
                            if npc.leader_id.is_some() {
                                leader_mappings.insert(new_id, npc.leader_id.unwrap());
                            }
                            npc.leader_id = None;

                            let chunk_pos = npc.get_chunk_coords();
                            let new_npc_id = self.track(Box::new(npc), tick);
                            self.update(new_npc_id, Some(chunk_pos), None);
                            npc_count += 1;
                        }
                    }
                }
            }

            // update leaders
            for (follower_id, leader_id) in leader_mappings {
                let npc = self.get_npc_mut(follower_id).unwrap();
                let new_leader_id = id_mappings[&leader_id];
                npc.leader_id = Some(new_leader_id);
            }

            // update followers
            for (leader_id, follower_ids) in follower_mappings {
                let npc = self.get_npc_mut(leader_id).unwrap();
                for follower_id in follower_ids {
                    let new_follower_id = id_mappings[&follower_id];
                    npc.follower_ids.insert(new_follower_id);
                }
            }

            log(
                Severity::Debug,
                &format!("Copied {} NPCs to instance {}", npc_count, instance_id),
            );
        }
        self.chunk_maps.get_mut(&instance_id).unwrap()
    }

    fn instance_player_enter(&mut self, instance_id: InstanceID) {
        let chunk_map = self.init_instance(instance_id);
        chunk_map.player_count += 1;
    }

    fn instance_player_exit(&mut self, instance_id: InstanceID) {
        let chunk_map = self.init_instance(instance_id);
        chunk_map.player_count -= 1;
    }

    fn check_instance_for_cleanup(&mut self, instance_id: InstanceID) {
        if instance_id.instance_num.is_none() {
            return; // don't clean up the main instance
        }

        let chunk_map = self.chunk_maps.get(&instance_id).unwrap();
        if chunk_map.player_count == 0 {
            for id in chunk_map.get_ids() {
                self.untrack(id);
            }
            self.chunk_maps.remove(&instance_id);
            log(
                Severity::Debug,
                &format!("Cleaned up instance {}", instance_id),
            );
        }
    }
}
impl Default for EntityMap {
    fn default() -> Self {
        Self {
            registry: HashMap::new(),
            chunk_maps: HashMap::new(),
            next_pc_id: 1,
            next_npc_id: 1,
            next_slider_id: 1,
        }
    }
}

#[derive(Default, Clone)]
pub struct Chunk {
    tracked: HashSet<EntityID>,
}

impl Chunk {
    fn get_all(&self) -> &HashSet<EntityID> {
        &self.tracked
    }

    fn insert(&mut self, id: EntityID) -> bool {
        self.tracked.insert(id)
    }

    fn remove(&mut self, id: EntityID) -> bool {
        self.tracked.remove(&id)
    }
}
