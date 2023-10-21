use std::collections::{HashMap, HashSet};

use crate::{player::Player, Entity, EntityID};

pub const NCHUNKS: usize = 16 * 8; // 16 map squares with side lengths of 8 chunks
pub const MAP_BOUNDS: i32 = 8192 * 100; // top corner of (16, 16)

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
        self.registry.values_mut().map(|f| &mut f.entity)
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

    pub fn update(&mut self, id: EntityID, to_chunk: Option<(i32, i32)>) {
        let entry = self.registry.get(&id).unwrap_or_else(|| {
            panic!("Entity with id {:?} untracked", id);
        });
        if entry.chunk == to_chunk {
            return;
        }

        self.remove_from_chunk(id);
        self.insert_into_chunk(id, to_chunk);
        println!("Moved to {:?}", self.registry[&id].chunk);
    }

    fn remove_from_chunk(&mut self, id: EntityID) {
        let entry = self.registry.get_mut(&id).unwrap();
        if let Some((x, y)) = entry.chunk {
            let chunk = &mut self.chunks[x as usize][y as usize];
            if !chunk.remove(id) {
                panic!("Chunk ({x}, {y}) did not contain entity with ID {:?}", id);
            }
            entry.chunk = None;
        }
    }

    fn insert_into_chunk(&mut self, id: EntityID, to_chunk: Option<(i32, i32)>) {
        if let Some((x, y)) = to_chunk {
            if let Some(chunk) = self.get_chunk(x, y) {
                if !chunk.insert(id) {
                    panic!("Chunk ({x}, {y}) already contained entity with ID {:?}", id);
                }
                let entry = self.registry.get_mut(&id).unwrap();
                entry.chunk = to_chunk;
            }
        }
    }

    fn get_chunk(&mut self, x: i32, y: i32) -> Option<&mut Chunk> {
        if (0..NCHUNKS as i32).contains(&x) && (0..NCHUNKS as i32).contains(&y) {
            let chunk = &mut self.chunks[x as usize][y as usize];
            return Some(chunk);
        }
        None
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
    pub fn get_all(&mut self) -> impl Iterator<Item = &EntityID> {
        self.tracked.iter()
    }

    pub fn insert(&mut self, id: EntityID) -> bool {
        self.tracked.insert(id)
    }

    pub fn remove(&mut self, id: EntityID) -> bool {
        self.tracked.remove(&id)
    }

    pub fn is_empty(&self) -> bool {
        self.tracked.is_empty()
    }
}
