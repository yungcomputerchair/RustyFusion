use std::collections::{HashMap, HashSet};

use crate::{net::ClientMap, player::Player, Entity, EntityID};

pub const NCHUNKS: usize = 16 * 8; // 16 map squares with side lengths of 8 chunks
pub const MAP_BOUNDS: i32 = 8192 * 100; // top corner of (16, 16)
pub const VISIBILITY_RANGE: i32 = 1;

struct RegistryEntry {
    entity: Box<dyn Entity>,
    chunk: Option<(i32, i32)>,
}

pub struct EntityMap {
    registry: HashMap<EntityID, RegistryEntry>,
    chunks: [[Chunk; NCHUNKS]; NCHUNKS],
}

impl EntityMap {
    pub fn get_all(&mut self) -> impl Iterator<Item = &mut Box<dyn Entity>> {
        self.registry.values_mut().map(|entry| &mut entry.entity)
    }

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

    pub fn get_player(&mut self, pc_uid: i64) -> Option<&mut Player> {
        let id = EntityID::Player(pc_uid);
        self.registry.get_mut(&id).and_then(|entry| {
            let entity_ref = entry.entity.as_mut().as_any();
            let player_ref = entity_ref.downcast_mut();
            player_ref
        })
    }

    pub fn track(&mut self, entity: Box<dyn Entity>) {
        let id = entity.get_id();
        if self.registry.contains_key(&id) {
            panic!("Already tracking entity with id {:?}", id);
        }
        let entry = RegistryEntry {
            entity,
            chunk: None,
        };
        self.registry.insert(id, entry);
    }

    pub fn update(
        &mut self,
        id: EntityID,
        to_chunk: Option<(i32, i32)>,
        _client_map: &mut ClientMap,
    ) {
        let entry = self.registry.get(&id).unwrap_or_else(|| {
            panic!("Entity with id {:?} untracked", id);
        });
        if entry.chunk == to_chunk {
            return;
        }

        let around_from = self.remove_from_chunk(id);
        let around_to = self.insert_into_chunk(id, to_chunk);

        let removed = around_from.difference(&around_to);
        for e in removed {
            println!("Goodbye {:?} {:?}", id, e);
        }

        let added = around_to.difference(&around_from);
        for e in added {
            println!("Hello {:?} {:?}", id, e);
        }

        println!("Moved to {:?}", self.registry[&id].chunk);
    }

    fn remove_from_chunk(&mut self, id: EntityID) -> HashSet<EntityID> {
        let mut affected = HashSet::new();
        let entry = self.registry.get_mut(&id).unwrap();
        if let Some((x, y)) = entry.chunk {
            let chunk = &mut self.chunks[x as usize][y as usize];
            if !chunk.remove(id) {
                panic!("Chunk ({x}, {y}) did not contain entity with ID {:?}", id);
            }
            entry.chunk = None;
            affected.extend(self.get_around(x, y, VISIBILITY_RANGE));
        }
        affected
    }

    fn insert_into_chunk(
        &mut self,
        id: EntityID,
        to_chunk: Option<(i32, i32)>,
    ) -> HashSet<EntityID> {
        let mut affected = HashSet::new();
        if let Some((x, y)) = to_chunk {
            if let Some(chunk) = self.get_chunk(x, y) {
                if !chunk.insert(id) {
                    panic!("Chunk ({x}, {y}) already contained entity with ID {:?}", id);
                }
                let entry = self.registry.get_mut(&id).unwrap();
                entry.chunk = to_chunk;
                affected.extend(self.get_around(x, y, VISIBILITY_RANGE));
                affected.remove(&id); // we don't want ourself in this
            }
        }
        affected
    }

    fn get_chunk(&mut self, x: i32, y: i32) -> Option<&mut Chunk> {
        if (0..NCHUNKS as i32).contains(&x) && (0..NCHUNKS as i32).contains(&y) {
            let chunk = &mut self.chunks[x as usize][y as usize];
            return Some(chunk);
        }
        None
    }

    fn get_around(&mut self, x: i32, y: i32, range: i32) -> HashSet<EntityID> {
        let mut entities = HashSet::new();
        for x in (x - range)..=(x + range) {
            for y in (y - range)..=(y + range) {
                if let Some(chunk) = self.get_chunk(x, y) {
                    entities.extend(chunk.get_all());
                }
            }
        }
        entities
    }
}
impl Default for EntityMap {
    fn default() -> Self {
        Self {
            chunks: std::array::from_fn(|_| std::array::from_fn(|_| Chunk::default())),
            registry: HashMap::new(),
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
