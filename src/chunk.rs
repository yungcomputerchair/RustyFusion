#![allow(clippy::needless_range_loop)]

use std::{
    alloc::{self, Layout},
    collections::{HashMap, HashSet},
    fmt::{Display, Formatter},
};

use crate::{
    config::config_get,
    defines::ID_OVERWORLD,
    entity::EntityID,
    error::{log, panic_log, FFError, FFResult, Severity},
    Position,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstanceID {
    pub channel_num: u8,
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

pub const MAP_SQUARE_COUNT: i32 = 16; // how many map squares there are in each direction
pub const NCHUNKS: usize = MAP_SQUARE_COUNT as usize * 8; // 16 map squares with side lengths of 8 chunks
pub const MAP_SQUARE_SIZE: i32 = 51200;
pub const MAP_SIZE: i32 = MAP_SQUARE_SIZE * MAP_SQUARE_COUNT; // top corner of (16, 16)
pub const CHUNK_SIZE: usize = MAP_SIZE as usize / NCHUNKS;

pub fn world_pos_to_chunk_pos(pos: Position) -> (i32, i32) {
    (
        (pos.x * NCHUNKS as i32) / MAP_SIZE,
        (pos.y * NCHUNKS as i32) / MAP_SIZE,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkCoords {
    pub i: InstanceID,
    pub x: i32,
    pub y: i32,
}
impl ChunkCoords {
    pub fn from_pos_inst(pos: Position, instance_id: InstanceID) -> Self {
        let (x, y) = world_pos_to_chunk_pos(pos);
        Self {
            x,
            y,
            i: instance_id,
        }
    }
}
impl Display for ChunkCoords {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {})", self.x, self.y, self.i)
    }
}

fn get_visibility_range() -> usize {
    config_get().shard.visibility_range.get()
}

#[derive(Debug, Clone, Copy)]
pub enum TickMode {
    Always,
    WhenLoaded,
    Never,
}

struct RegistryEntry {
    chunk: Option<ChunkCoords>,
    tick_mode: TickMode,
}

struct ChunkMap {
    chunks: Box<[[Chunk; NCHUNKS]; NCHUNKS]>,
    loaded_chunk_count: usize,
    loaded_entity_count: usize,
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

    fn get_player_count(&self) -> usize {
        self.chunks
            .iter()
            .flatten()
            .map(|chunk| chunk.get_player_count())
            .sum()
    }
}

/// Tracks spatial metadata (chunk positions, tick modes) and visibility for
/// entities. Does NOT store entity data — that lives in typed stores on
/// `ShardServerState`.
pub struct EntityMap {
    registry: HashMap<EntityID, RegistryEntry>,
    chunk_maps: HashMap<InstanceID, ChunkMap>,
    instances_to_cleanup: HashSet<InstanceID>,
    entities_to_cleanup: HashSet<EntityID>,
    next_pc_id: u32,
    next_npc_id: u32,
    next_slider_id: u32,
    next_egg_id: u32,
}

impl EntityMap {
    pub fn get_all_ids(&self) -> impl Iterator<Item = EntityID> + '_ {
        self.registry.keys().cloned()
    }

    pub fn get_tickable_ids(&self) -> impl Iterator<Item = EntityID> + '_ {
        self.registry
            .iter()
            .filter_map(|(id, entry)| match entry.tick_mode {
                TickMode::Always => Some(*id),
                TickMode::Never => None,
                TickMode::WhenLoaded => {
                    match entry.chunk {
                        Some(coords) => {
                            if let Some(chunk) = self.get_chunk(coords) {
                                if chunk.is_loaded() {
                                    return Some(*id);
                                }
                            }
                            None
                        }
                        // need to tick transient entities!
                        // e.g. when a mob is dead it is off-screen, but
                        // needs to tick or else it will never respawn
                        None => Some(*id),
                    }
                }
            })
    }

    pub fn get_entity_chunk(&self, id: EntityID) -> Option<ChunkCoords> {
        self.registry.get(&id).and_then(|entry| entry.chunk)
    }

    pub fn get_around_entity(&self, id: EntityID) -> HashSet<EntityID> {
        let mut entities = HashSet::new();
        if let Some(coords) = self.registry.get(&id).and_then(|entry| entry.chunk) {
            for coords in Self::get_coords_around(coords, get_visibility_range()) {
                if let Some(chunk) = self.get_chunk(coords) {
                    entities.extend(chunk.get_all());
                }
            }
        }
        entities
    }

    pub fn get_around_chunk(&self, coords: ChunkCoords) -> HashSet<EntityID> {
        let mut entities = HashSet::new();
        for coords in Self::get_coords_around(coords, get_visibility_range()) {
            if let Some(chunk) = self.get_chunk(coords) {
                entities.extend(chunk.get_all());
            }
        }
        entities
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

    pub fn get_npc_ids(&self) -> impl Iterator<Item = i32> + '_ {
        self.registry.keys().filter_map(|id| {
            if let EntityID::NPC(npc_id) = id {
                Some(*npc_id)
            } else {
                None
            }
        })
    }

    pub fn get_instance_ids(&self, instance_id: InstanceID) -> Vec<EntityID> {
        self.chunk_maps
            .get(&instance_id)
            .map(|cm| cm.get_ids())
            .unwrap_or_default()
    }

    pub fn get_tick_mode(&self, id: EntityID) -> Option<TickMode> {
        self.registry.get(&id).map(|entry| entry.tick_mode)
    }

    pub fn instance_exists(&self, instance_id: InstanceID) -> bool {
        self.chunk_maps.contains_key(&instance_id)
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

    pub fn gen_next_egg_id(&mut self) -> i32 {
        let id = self.next_egg_id;
        if id == u32::MAX {
            panic_log("Ran out of egg IDs");
        }
        self.next_egg_id += 1;
        id as i32
    }

    pub fn track(&mut self, id: EntityID, tick_mode: TickMode) {
        if self.registry.contains_key(&id) {
            panic_log(&format!("Already tracking entity with id {:?}", id));
        }
        let entry = RegistryEntry {
            chunk: None,
            tick_mode,
        };
        self.registry.insert(id, entry);
    }

    pub fn untrack(&mut self, id: EntityID) {
        if self.registry.remove(&id).is_none() {
            panic_log(&format!("Entity with id {:?} already untracked", id));
        }
    }

    pub fn is_tracked(&self, id: EntityID) -> bool {
        self.registry.contains_key(&id)
    }

    pub fn mark_for_cleanup(&mut self, id: EntityID) {
        self.entities_to_cleanup.insert(id);
    }

    /// Moves an entity between chunks. Returns the sets of entity IDs that
    /// left and entered this entity's visibility as a result of the move.
    /// Returns `None` if the entity was already in the target chunk.
    pub fn update(
        &mut self,
        id: EntityID,
        to_chunk: Option<ChunkCoords>,
    ) -> Option<(HashSet<EntityID>, HashSet<EntityID>)> {
        let entry = self.registry.get_mut(&id).unwrap_or_else(|| {
            panic_log(&format!("Entity with id {:?} untracked", id));
        });
        let from_chunk = entry.chunk;
        if from_chunk == to_chunk {
            return None;
        }

        let around_from = self.remove_from_chunk(id);
        let around_to = self.insert_into_chunk(id, to_chunk);
        if let Some(coords) = from_chunk {
            if to_chunk.is_none() && self.should_cleanup_instance(coords.i) {
                self.instances_to_cleanup.insert(coords.i);
            }
        }

        let removed: HashSet<EntityID> = around_from.difference(&around_to).cloned().collect();
        let added: HashSet<EntityID> = around_to.difference(&around_from).cloned().collect();
        Some((removed, added))
    }

    pub fn set_tick(&mut self, id: EntityID, tick_mode: TickMode) -> FFResult<()> {
        let entry = self.registry.get_mut(&id).ok_or(FFError::build(
            Severity::Warning,
            format!("Entity with ID {:?} doesn't exist", id),
        ))?;
        entry.tick_mode = tick_mode;
        Ok(())
    }

    pub fn get_channel_population(&self, channel_num: u8) -> usize {
        self.chunk_maps
            .iter()
            .filter(|(instance_id, _)| instance_id.channel_num == channel_num)
            .map(|(_, chunk_map)| chunk_map.get_player_count())
            .sum()
    }

    pub fn get_min_pop_channel_num(&self) -> u8 {
        let num_channels = config_get().shard.num_channels.get();
        (1..=num_channels)
            .min_by_key(|channel_num| self.get_channel_population(*channel_num))
            .unwrap()
    }

    pub fn get_num_instances(&self) -> usize {
        self.chunk_maps.len()
    }

    pub fn get_num_base_instances(&self) -> usize {
        self.chunk_maps
            .keys()
            .filter(|iid| iid.instance_num.is_none())
            .count()
    }

    pub fn get_num_chunks(&self) -> usize {
        self.chunk_maps.len() * NCHUNKS * NCHUNKS
    }

    pub fn get_num_loaded_chunks(&self) -> usize {
        self.chunk_maps
            .values()
            .map(|cm| cm.loaded_chunk_count)
            .sum()
    }

    pub fn get_num_loaded_entities(&self) -> usize {
        self.chunk_maps
            .values()
            .map(|cm| cm.loaded_entity_count)
            .sum()
    }

    fn remove_from_chunk(&mut self, id: EntityID) -> HashSet<EntityID> {
        let mut affected = HashSet::new();
        let entry = self.registry.get_mut(&id).unwrap();
        if let Some(coords) = entry.chunk {
            entry.chunk = None;
            let instance_id = coords.i;
            let chunk = self.get_chunk_mut(coords).unwrap();
            if !chunk.remove(id) {
                panic_log(&format!(
                    "Chunk {:?} did not contain entity with ID {:?}",
                    coords, id
                ));
            }
            let chunk_was_loaded = chunk.is_loaded();
            let is_player = matches!(id, EntityID::Player(_));
            let mut chunks_unloaded = 0;
            let mut entities_in_unloaded = 0;
            let coords_around = Self::get_coords_around(coords, get_visibility_range());
            for coords in coords_around {
                if let Some(chunk) = self.get_chunk_mut(coords) {
                    affected.extend(chunk.get_all());
                    if is_player {
                        let was_loaded = chunk.is_loaded();
                        chunk.load_count -= 1;
                        if was_loaded && chunk.load_count == 0 {
                            chunks_unloaded += 1;
                            entities_in_unloaded += chunk.tracked.len();
                        }
                    }
                }
            }
            let cm = self.chunk_maps.get_mut(&instance_id).unwrap();
            if chunk_was_loaded {
                cm.loaded_entity_count -= 1;
            }
            cm.loaded_chunk_count -= chunks_unloaded;
            cm.loaded_entity_count -= entities_in_unloaded;
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
            let instance_id = coords.i;
            if let Some(chunk) = self.get_chunk_mut(coords) {
                if !chunk.insert(id) {
                    panic_log(&format!(
                        "Chunk {:?} already contained entity with ID {:?}",
                        coords, id
                    ));
                }
                let chunk_already_loaded = chunk.is_loaded();
                let entry = self.registry.get_mut(&id).unwrap();
                entry.chunk = to_chunk;
                let is_player = matches!(id, EntityID::Player(_));
                let mut chunks_loaded = 0;
                let mut entities_in_loaded = 0;
                let coords_around = Self::get_coords_around(coords, get_visibility_range());
                for coords in coords_around {
                    if let Some(chunk) = self.get_chunk_mut(coords) {
                        if is_player {
                            let was_loaded = chunk.is_loaded();
                            chunk.load_count += 1;
                            if !was_loaded {
                                chunks_loaded += 1;
                                entities_in_loaded += chunk.tracked.len();
                            }
                        }
                        affected.extend(chunk.get_all());
                    }
                }
                affected.remove(&id); // we don't want ourself in this
                let cm = self.chunk_maps.get_mut(&instance_id).unwrap();
                if chunk_already_loaded {
                    cm.loaded_entity_count += 1;
                }
                cm.loaded_chunk_count += chunks_loaded;
                cm.loaded_entity_count += entities_in_loaded;
            }
        }
        affected
    }

    fn get_chunk(&self, coords: ChunkCoords) -> Option<&Chunk> {
        if (0..NCHUNKS as i32).contains(&coords.x) && (0..NCHUNKS as i32).contains(&coords.y) {
            let chunk_map = self.chunk_maps.get(&coords.i)?;
            let chunk = &chunk_map.chunks[coords.x as usize][coords.y as usize];
            return Some(chunk);
        }
        None
    }

    fn get_chunk_mut(&mut self, coords: ChunkCoords) -> Option<&mut Chunk> {
        if (0..NCHUNKS as i32).contains(&coords.x) && (0..NCHUNKS as i32).contains(&coords.y) {
            let chunk_map = self.init_instance(coords.i);
            let chunk = &mut chunk_map.chunks[coords.x as usize][coords.y as usize];
            return Some(chunk);
        }
        None
    }

    fn get_coords_around(coords: ChunkCoords, range: usize) -> Vec<ChunkCoords> {
        let num_around = (range * 2 + 1) * (range * 2 + 1);
        let mut coords_around = Vec::with_capacity(num_around);
        let range = range as i32;
        for x in (coords.x - range)..=(coords.x + range) {
            for y in (coords.y - range)..=(coords.y + range) {
                coords_around.push(ChunkCoords { x, y, i: coords.i });
            }
        }
        coords_around
    }

    fn init_instance(&mut self, instance_id: InstanceID) -> &mut ChunkMap {
        self.chunk_maps.entry(instance_id).or_insert_with(|| {
            let chunks = unsafe {
                let ptr = alloc::alloc(Layout::new::<[[Chunk; NCHUNKS]; NCHUNKS]>())
                    as *mut [[Chunk; NCHUNKS]; NCHUNKS];
                if ptr.is_null() {
                    panic_log("Failed to allocate memory for chunk map");
                }
                for x in 0..NCHUNKS {
                    for y in 0..NCHUNKS {
                        let chunk_ptr = &mut (*ptr)[x][y] as *mut Chunk;
                        chunk_ptr.write(Chunk::default());
                    }
                }
                Box::from_raw(ptr)
            };
            log(
                Severity::Debug,
                &format!("Initialized instance {}", instance_id),
            );
            ChunkMap {
                chunks,
                loaded_chunk_count: 0,
                loaded_entity_count: 0,
            }
        });
        self.chunk_maps.get_mut(&instance_id).unwrap()
    }

    pub fn garbage_collect_instances(&mut self) -> Vec<EntityID> {
        let mut entity_ids = Vec::new();
        let instances_to_cleanup = self.instances_to_cleanup.clone();
        for instance_id in instances_to_cleanup {
            entity_ids.extend(self.cleanup_instance(instance_id));
        }
        self.instances_to_cleanup.clear();
        entity_ids
    }

    pub fn garbage_collect_entities(&mut self) -> Vec<EntityID> {
        let entity_ids: Vec<EntityID> = self.entities_to_cleanup.iter().cloned().collect();
        for id in &entity_ids {
            self.untrack(*id);
        }
        self.entities_to_cleanup.clear();
        entity_ids
    }

    fn cleanup_instance(&mut self, instance_id: InstanceID) -> Vec<EntityID> {
        let ids = self
            .chunk_maps
            .get(&instance_id)
            .map(|cm| cm.get_ids())
            .unwrap_or_default();
        for id in &ids {
            self.untrack(*id);
        }
        self.chunk_maps.remove(&instance_id);
        log(
            Severity::Debug,
            &format!("Cleaned up instance {}", instance_id),
        );
        ids
    }

    fn should_cleanup_instance(&mut self, instance_id: InstanceID) -> bool {
        if instance_id.instance_num.is_none() {
            return false; // don't clean up the main instance
        }

        let chunk_map = self.chunk_maps.get(&instance_id).unwrap();
        chunk_map.get_player_count() == 0
    }
}
impl Default for EntityMap {
    fn default() -> Self {
        Self {
            registry: HashMap::new(),
            chunk_maps: HashMap::new(),
            instances_to_cleanup: HashSet::new(),
            entities_to_cleanup: HashSet::new(),
            next_pc_id: 1,
            next_npc_id: 1,
            next_slider_id: 1,
            next_egg_id: 1,
        }
    }
}

#[derive(Default, Clone)]
pub struct Chunk {
    load_count: usize,
    tracked: HashSet<EntityID>,
}

impl Chunk {
    fn is_loaded(&self) -> bool {
        self.load_count > 0
    }

    fn get_all(&self) -> &HashSet<EntityID> {
        &self.tracked
    }

    fn get_player_count(&self) -> usize {
        self.tracked
            .iter()
            .filter(|id| matches!(id, EntityID::Player(_)))
            .count()
    }

    fn insert(&mut self, id: EntityID) -> bool {
        self.tracked.insert(id)
    }

    fn remove(&mut self, id: EntityID) -> bool {
        self.tracked.remove(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Position that maps to chunk (x, y) in the grid.
    /// Each chunk spans MAP_SIZE / NCHUNKS units.
    fn pos_for_chunk(x: i32, y: i32) -> Position {
        let chunk_size = MAP_SIZE / NCHUNKS as i32;
        Position {
            x: x * chunk_size + chunk_size / 2,
            y: y * chunk_size + chunk_size / 2,
            z: 0,
        }
    }

    fn default_instance() -> InstanceID {
        InstanceID::default()
    }

    /// Register an entity in the map and place it in the appropriate chunk.
    fn place_entity(
        map: &mut EntityMap,
        id: EntityID,
        pos: Position,
        inst: InstanceID,
    ) -> ChunkCoords {
        let chunk = ChunkCoords::from_pos_inst(pos, inst);
        map.track(id, TickMode::WhenLoaded);
        map.update(id, Some(chunk));
        chunk
    }

    #[test]
    fn test_loaded_counts_zero_initially() {
        let map = EntityMap::default();
        assert_eq!(map.get_num_loaded_chunks(), 0);
        assert_eq!(map.get_num_loaded_entities(), 0);
    }

    #[test]
    fn test_player_loads_surrounding_chunks() {
        let mut map = EntityMap::default();
        let inst = default_instance();

        let pos = pos_for_chunk(64, 64);
        place_entity(&mut map, EntityID::Player(1), pos, inst);

        assert_eq!(map.get_num_loaded_chunks(), 9);
        assert_eq!(map.get_num_loaded_entities(), 1);
    }

    #[test]
    fn test_npc_in_loaded_chunk_counted() {
        let mut map = EntityMap::default();
        let inst = default_instance();
        let pos = pos_for_chunk(64, 64);

        place_entity(&mut map, EntityID::NPC(1), pos, inst);
        assert_eq!(map.get_num_loaded_chunks(), 0);
        assert_eq!(map.get_num_loaded_entities(), 0);

        place_entity(&mut map, EntityID::Player(1), pos, inst);
        assert_eq!(map.get_num_loaded_chunks(), 9);
        assert_eq!(map.get_num_loaded_entities(), 2);
    }

    #[test]
    fn test_removing_player_unloads_chunks() {
        let mut map = EntityMap::default();
        let inst = default_instance();
        let pos = pos_for_chunk(64, 64);

        place_entity(&mut map, EntityID::Player(1), pos, inst);
        assert_eq!(map.get_num_loaded_chunks(), 9);
        assert_eq!(map.get_num_loaded_entities(), 1);

        map.update(EntityID::Player(1), None);
        assert_eq!(map.get_num_loaded_chunks(), 0);
        assert_eq!(map.get_num_loaded_entities(), 0);
    }

    #[test]
    fn test_two_players_overlapping_visibility() {
        let mut map = EntityMap::default();
        let inst = default_instance();
        let pos = pos_for_chunk(64, 64);

        place_entity(&mut map, EntityID::Player(1), pos, inst);
        place_entity(&mut map, EntityID::Player(2), pos, inst);

        assert_eq!(map.get_num_loaded_chunks(), 9);
        assert_eq!(map.get_num_loaded_entities(), 2);
    }

    #[test]
    fn test_removing_one_of_two_overlapping_players() {
        let mut map = EntityMap::default();
        let inst = default_instance();
        let pos = pos_for_chunk(64, 64);

        place_entity(&mut map, EntityID::Player(1), pos, inst);
        place_entity(&mut map, EntityID::Player(2), pos, inst);

        map.update(EntityID::Player(1), None);
        assert_eq!(map.get_num_loaded_chunks(), 9);
        assert_eq!(map.get_num_loaded_entities(), 1);
    }

    #[test]
    fn test_npc_added_to_already_loaded_chunk() {
        let mut map = EntityMap::default();
        let inst = default_instance();
        let pos = pos_for_chunk(64, 64);

        place_entity(&mut map, EntityID::Player(1), pos, inst);
        assert_eq!(map.get_num_loaded_entities(), 1);

        place_entity(&mut map, EntityID::NPC(1), pos, inst);
        assert_eq!(map.get_num_loaded_entities(), 2);

        place_entity(&mut map, EntityID::NPC(2), pos, inst);
        assert_eq!(map.get_num_loaded_entities(), 3);
    }

    #[test]
    fn test_npc_removed_from_loaded_chunk() {
        let mut map = EntityMap::default();
        let inst = default_instance();
        let pos = pos_for_chunk(64, 64);

        place_entity(&mut map, EntityID::Player(1), pos, inst);
        place_entity(&mut map, EntityID::NPC(1), pos, inst);
        assert_eq!(map.get_num_loaded_entities(), 2);

        map.update(EntityID::NPC(1), None);
        assert_eq!(map.get_num_loaded_entities(), 1);
        assert_eq!(map.get_num_loaded_chunks(), 9);
    }

    #[test]
    fn test_player_moving_between_chunks() {
        let mut map = EntityMap::default();
        let inst = default_instance();

        let pos1 = pos_for_chunk(64, 64);
        place_entity(&mut map, EntityID::Player(1), pos1, inst);
        assert_eq!(map.get_num_loaded_chunks(), 9);

        let pos2 = pos_for_chunk(80, 80);
        let new_coords = ChunkCoords::from_pos_inst(pos2, inst);
        map.update(EntityID::Player(1), Some(new_coords));

        assert_eq!(map.get_num_loaded_chunks(), 9);
        assert_eq!(map.get_num_loaded_entities(), 1);
    }

    #[test]
    fn test_npcs_in_chunk_counted_on_load_transition() {
        let mut map = EntityMap::default();
        let inst = default_instance();
        let pos = pos_for_chunk(64, 64);

        for i in 1..=5 {
            place_entity(&mut map, EntityID::NPC(i), pos, inst);
        }
        assert_eq!(map.get_num_loaded_entities(), 0);

        place_entity(&mut map, EntityID::Player(1), pos, inst);
        assert_eq!(map.get_num_loaded_entities(), 6);

        map.update(EntityID::Player(1), None);
        assert_eq!(map.get_num_loaded_entities(), 0);
        assert_eq!(map.get_num_loaded_chunks(), 0);
    }

    #[test]
    fn test_entity_at_chunk_boundary() {
        let mut map = EntityMap::default();
        let inst = default_instance();

        let player_pos = pos_for_chunk(1, 1);
        place_entity(&mut map, EntityID::Player(1), player_pos, inst);

        let npc_pos = pos_for_chunk(2, 2);
        place_entity(&mut map, EntityID::NPC(1), npc_pos, inst);
        assert_eq!(map.get_num_loaded_entities(), 2);

        let far_npc_pos = pos_for_chunk(3, 3);
        place_entity(&mut map, EntityID::NPC(2), far_npc_pos, inst);
        assert_eq!(map.get_num_loaded_entities(), 2);
    }

    #[test]
    fn test_update_returns_visibility_deltas() {
        let mut map = EntityMap::default();
        let inst = default_instance();
        let pos = pos_for_chunk(64, 64);

        // Place NPC first
        place_entity(&mut map, EntityID::NPC(1), pos, inst);

        // Place player at same position — NPC should appear in `added`
        map.track(EntityID::Player(1), TickMode::WhenLoaded);
        let chunk = ChunkCoords::from_pos_inst(pos, inst);
        let (removed, added) = map.update(EntityID::Player(1), Some(chunk)).unwrap();
        assert!(removed.is_empty());
        assert!(added.contains(&EntityID::NPC(1)));

        // Move player far away — NPC should appear in `removed`
        let far_pos = pos_for_chunk(80, 80);
        let far_chunk = ChunkCoords::from_pos_inst(far_pos, inst);
        let (removed, added) = map.update(EntityID::Player(1), Some(far_chunk)).unwrap();
        assert!(removed.contains(&EntityID::NPC(1)));
        assert!(!added.contains(&EntityID::NPC(1)));
    }

    #[test]
    fn test_update_same_chunk_returns_none() {
        let mut map = EntityMap::default();
        let inst = default_instance();
        let pos = pos_for_chunk(64, 64);
        let chunk = ChunkCoords::from_pos_inst(pos, inst);

        place_entity(&mut map, EntityID::Player(1), pos, inst);
        assert!(map.update(EntityID::Player(1), Some(chunk)).is_none());
    }
}
