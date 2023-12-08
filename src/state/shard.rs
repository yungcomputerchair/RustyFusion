use std::collections::HashMap;

use crate::{
    chunk::EntityMap,
    error::{FFError, FFResult, Severity},
    net::{LoginData, CONN_ID_DISCONNECTED},
    player::Player,
    tabledata::tdata_get,
    Entity, Item,
};

pub struct ShardServerState {
    login_server_conn_id: i64,
    next_pc_id: i32,
    login_data: HashMap<i64, LoginData>,
    entity_map: EntityMap,
    buyback_lists: HashMap<i32, Vec<Item>>,
}

impl Default for ShardServerState {
    fn default() -> Self {
        let mut state = Self {
            login_server_conn_id: CONN_ID_DISCONNECTED,
            next_pc_id: 1,
            login_data: HashMap::new(),
            entity_map: EntityMap::default(),
            buyback_lists: HashMap::new(),
        };
        for npc in tdata_get().get_npcs() {
            let chunk_pos = npc.get_position().chunk_coords();
            let entity_map = state.get_entity_map();
            let id = entity_map.track(Box::new(npc));
            entity_map.update(id, Some(chunk_pos), None);
        }
        state
    }
}
impl ShardServerState {
    pub fn get_login_server_conn_id(&self) -> i64 {
        self.login_server_conn_id
    }

    pub fn gen_next_pc_id(&mut self) -> i32 {
        let id = self.next_pc_id;
        self.next_pc_id += 1;
        id
    }

    pub fn get_login_data(&mut self) -> &mut HashMap<i64, LoginData> {
        &mut self.login_data
    }

    pub fn get_entity_map(&mut self) -> &mut EntityMap {
        &mut self.entity_map
    }

    pub fn get_buyback_lists(&mut self) -> &mut HashMap<i32, Vec<Item>> {
        &mut self.buyback_lists
    }

    pub fn set_login_server_conn_id(&mut self, conn_id: i64) {
        self.login_server_conn_id = conn_id;
    }

    pub fn get_player_mut(&mut self, pc_id: i32) -> FFResult<&mut Player> {
        self.entity_map.get_player(pc_id).ok_or(FFError::build(
            Severity::Warning,
            format!("Player with ID {} doesn't exist", pc_id),
        ))
    }
}
