use std::collections::HashMap;

use crate::player::Player;

pub struct LoginServerState {
    next_pc_uid: i64,
    next_shard_id: i64,
    pub players: HashMap<i64, Player>,
}
impl Default for LoginServerState {
    fn default() -> Self {
        Self {
            next_pc_uid: 1,
            next_shard_id: 1,
            players: HashMap::new(),
        }
    }
}
impl LoginServerState {
    pub fn get_next_pc_uid(&mut self) -> i64 {
        let next = self.next_pc_uid;
        self.next_pc_uid += 1;
        next
    }

    pub fn get_next_shard_id(&mut self) -> i64 {
        let next = self.next_shard_id;
        self.next_shard_id += 1;
        next
    }
}
