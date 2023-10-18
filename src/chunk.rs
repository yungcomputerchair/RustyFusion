use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crate::{Entity, EntityID};

pub const NCHUNKS: usize = 16 * 8; // 16 map squares with side lengths of 8 chunks
pub const MAP_BOUNDS: i32 = 8192 * 100; // top corner of (16, 16)

pub struct EntityMap {
    chunks: [[Option<Chunk>; NCHUNKS]; NCHUNKS],
    unchunked: HashMap<EntityID, Rc<RefCell<dyn Entity>>>,
    registry: HashMap<EntityID, Option<(i32, i32)>>,
}

impl EntityMap {
    pub fn get_all(&mut self) -> Box<dyn Iterator<Item = &mut Rc<RefCell<dyn Entity>>> + '_> {
        let mut entities: Box<dyn Iterator<Item = &mut Rc<RefCell<dyn Entity>>>> =
            Box::new(self.unchunked.values_mut());
        for chunk in self.chunks.iter_mut().flatten().flatten() {
            entities = Box::new(entities.chain(chunk.get_all()));
        }
        entities
    }

    pub fn track(&mut self, entity: Rc<RefCell<dyn Entity>>) {
        let id = entity.borrow().get_id();
        if self.registry.contains_key(&id) {
            panic!("Already tracking entity with id {:?}", id);
        }
        self.unchunked.insert(id, entity);
        self.registry.insert(id, None);
    }

    pub fn update(&mut self, id: EntityID, to_chunk: Option<(i32, i32)>) {
        if self.registry.get(&id).is_some_and(|current_chunk| *current_chunk == to_chunk) {
            return;
        }

        if let Some((x, y)) = to_chunk {
            println!("Moving to ({x}, {y})");
        }

        // remove from last chunk
        let from_chunk = self
            .registry
            .remove(&id)
            .unwrap_or_else(|| panic!("Entity with id {:?} untracked", id));

        let entity;
        if let Some((x, y)) = from_chunk {
            // chunk is guaranteed to exist; see below
            let chunk = self.chunks[x as usize][y as usize].as_mut().unwrap();
            entity = chunk.remove(&id);
        } else {
            entity = self.unchunked.remove(&id);
        }
        let entity = entity.unwrap();

        // reinsert
        if let Some((x, y)) = to_chunk {
            if (0..NCHUNKS as i32).contains(&x) && (0..NCHUNKS as i32).contains(&y) {
                let chunk = &mut self.chunks[x as usize][y as usize];
                let chunk = chunk.get_or_insert(Chunk::default()); // init chunk
                chunk.insert(entity);
                self.registry.insert(id, to_chunk);
                return;
            }
        }

        self.unchunked.insert(id, entity);
        self.registry.insert(id, None);
    }
}
impl Default for EntityMap {
    fn default() -> Self {
        Self {
            chunks: std::array::from_fn(|_| std::array::from_fn(|_| None)),
            unchunked: HashMap::new(),
            registry: HashMap::new(),
        }
    }
}

#[derive(Default)]
pub struct Chunk {
    tracked: HashMap<EntityID, Rc<RefCell<dyn Entity>>>,
}

impl Chunk {
    pub fn get_all(&mut self) -> impl Iterator<Item = &mut Rc<RefCell<dyn Entity>>> {
        self.tracked.values_mut()
    }

    pub fn insert(&mut self, entity: Rc<RefCell<dyn Entity>>) {
        let id = entity.borrow().get_id();
        self.tracked.insert(id, entity);
    }

    pub fn remove(&mut self, id: &EntityID) -> Option<Rc<RefCell<dyn Entity>>> {
        self.tracked.remove(id)
    }
}
