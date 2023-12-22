use std::{
    collections::{HashMap, HashSet},
    fmt::{Display, Formatter},
};

use uuid::Uuid;

use crate::{
    config::config_get,
    defines::ID_OVERWORLD,
    error::{log, FFError, FFResult, Severity},
    net::{ffclient::FFClient, ClientMap},
    npc::NPC,
    player::Player,
    Entity, EntityID, Position,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstanceID {
    pub map_num: u32,
    pub instance_num: Option<Uuid>,
}
impl Default for InstanceID {
    fn default() -> Self {
        Self {
            map_num: ID_OVERWORLD,
            instance_num: None,
        }
    }
}
impl Display for InstanceID {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}",
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

pub const NCHUNKS: usize = 16 * 8; // 16 map squares with side lengths of 8 chunks
pub const MAP_BOUNDS: i32 = 8192 * 100; // top corner of (16, 16)
fn get_visibility_range() -> usize {
    config_get().shard.visibility_range.get()
}

struct RegistryEntry {
    entity: Box<dyn Entity>,
    chunk: Option<ChunkCoords>,
}

struct ChunkMap {
    player_count: usize,
    chunks: [[Chunk; NCHUNKS]; NCHUNKS],
}

pub struct EntityMap {
    registry: HashMap<EntityID, RegistryEntry>,
    chunk_maps: HashMap<InstanceID, ChunkMap>,
}

impl EntityMap {
    pub fn get_from_id(&mut self, id: EntityID) -> Option<&mut Box<dyn Entity>> {
        self.registry.get_mut(&id).map(|entry| &mut entry.entity)
    }

    pub fn get_from_ids(
        &mut self,
        ids: &HashSet<EntityID>,
    ) -> impl Iterator<Item = &mut Box<dyn Entity>> {
        let ids = ids.clone();
        self.registry.iter_mut().filter_map(move |(id, entry)| {
            if ids.contains(id) {
                Some(&mut entry.entity)
            } else {
                None
            }
        })
    }

    pub fn get_around_entity(
        &mut self,
        id: EntityID,
    ) -> Option<impl Iterator<Item = &mut Box<dyn Entity>>> {
        if let Some(coords) = self.registry.get(&id).and_then(|entry| entry.chunk) {
            let ids = self.get_around(coords, get_visibility_range());
            Some(self.get_from_ids(&ids))
        } else {
            None
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

    pub fn find_player(&self, f: impl Fn(&Player) -> bool) -> Option<i32> {
        self.registry.values().find_map(|entry| {
            let entity_id = entry.entity.get_id();
            if let EntityID::Player(pc_id) = entity_id {
                let pc = self.get_player(pc_id).unwrap();
                if f(pc) {
                    return Some(pc_id);
                }
            }
            None
        })
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

    pub fn find_npc(&self, f: impl Fn(&NPC) -> bool) -> Option<i32> {
        self.registry.values().find_map(|entry| {
            let entity_id = entry.entity.get_id();
            if let EntityID::NPC(npc_id) = entity_id {
                let npc = self.get_npc(npc_id).unwrap();
                if f(npc) {
                    return Some(npc_id);
                }
            }
            None
        })
    }

    pub fn validate_proximity(&self, ids: &[EntityID], range: u32) -> FFResult<()> {
        let mut positions = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(entity) = self.registry.get(id) {
                positions.push(entity.entity.get_position());
            } else {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("Entity with ID {:?} doesn't exist", id),
                ));
            }
        }

        for i in 0..positions.len() {
            for j in (i + 1)..positions.len() {
                let pos1 = positions[i];
                let pos2 = positions[j];
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

    pub fn track(&mut self, entity: Box<dyn Entity>) -> EntityID {
        let id = entity.get_id();
        if self.registry.contains_key(&id) {
            panic!("Already tracking entity with id {:?}", id);
        }
        let entry = RegistryEntry {
            entity,
            chunk: None,
        };
        self.registry.insert(id, entry);
        id
    }

    pub fn untrack(&mut self, id: EntityID) -> Box<dyn Entity> {
        self.registry
            .remove(&id)
            .unwrap_or_else(|| {
                panic!("Entity with id {:?} already untracked", id);
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
            panic!("Entity with id {:?} untracked", id);
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
                let to = self.get_from_id(*e).unwrap();
                let _ = to.send_exit(from_client);
            }

            // them to us
            let from = self.get_from_id(*e).unwrap();
            if let Some(from_client) = from.get_client(client_map) {
                let to = self.get_from_id(id).unwrap();
                let _ = to.send_exit(from_client);
            }
        }

        let added = around_to.difference(&around_from);
        for e in added {
            // us to them
            let from = self.get_from_id(id).unwrap();
            if let Some(from_client) = from.get_client(client_map) {
                let to = self.get_from_id(*e).unwrap();
                let _ = to.send_enter(from_client);
            }

            // them to us
            let from = self.get_from_id(*e).unwrap();
            if let Some(from_client) = from.get_client(client_map) {
                let to = self.get_from_id(id).unwrap();
                let _ = to.send_enter(from_client);
            }
        }

        log(
            Severity::Debug,
            &match self.registry[&id].chunk {
                Some(coords) => format!("Moved to {}", coords),
                None => "Removed".to_string(),
            },
        );
    }

    pub fn for_each_around(
        &mut self,
        id: EntityID,
        clients: &mut ClientMap,
        mut f: impl FnMut(&mut FFClient),
    ) {
        if let Some(iter) = self.get_around_entity(id) {
            for e in iter {
                if let Some(c) = e.get_client(clients) {
                    f(c);
                }
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
                panic!("Chunk {:?} did not contain entity with ID {:?}", coords, id);
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
                    panic!(
                        "Chunk {:?} already contained entity with ID {:?}",
                        coords, id
                    );
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
        self.chunk_maps.entry(instance_id).or_insert_with(|| {
            let chunks: [[Chunk; NCHUNKS]; NCHUNKS] =
                std::array::from_fn(|_| std::array::from_fn(|_| Chunk::default()));
            log(
                Severity::Info,
                &format!("Initialized instance {}", instance_id),
            );
            ChunkMap {
                player_count: 0,
                chunks,
            }
        })
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
            self.chunk_maps.remove(&instance_id);
            log(
                Severity::Info,
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
        }
    }
}

#[derive(Default)]
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
