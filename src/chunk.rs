use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crate::{Entity, EntityID};

const NCHUNKS: usize = 16 * 8; // 16 map squares with side lengths of 8 chunks

pub struct EntityMap {
    chunks: [[Option<Chunk>; NCHUNKS]; NCHUNKS],
    unchunked: HashMap<EntityID, Rc<RefCell<dyn Entity>>>,
    registry: HashMap<EntityID, (usize, usize)>,
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

    pub fn update(&mut self, entity: Rc<RefCell<dyn Entity>>, chunk: Option<(usize, usize)>) {
        let id = entity.borrow().get_id();

        // remove from last chunk
        let last_chunk = self.registry.get(&id);
        if let Some((x, y)) = last_chunk {
            let chunk = self.chunks[*x][*y].as_mut().unwrap();
            chunk.remove(id);
        }

        // insert
        if let Some((x, y)) = chunk {
            if self.chunks[x][y].is_none() {
                self.chunks[x][y] = Some(Chunk::default());
            }
            let chunk = self.chunks[x][y].as_mut().unwrap();
            chunk.insert(entity);
            self.registry.insert(id, (x, y));
        } else {
            self.unchunked.insert(id, entity);
        }
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

    pub fn remove(&mut self, id: EntityID) {
        if self.tracked.remove(&id).is_none() {
            panic!(
                "Tried to remove entity {:?} from chunk, but it wasn't there",
                id
            );
        }
    }
}
