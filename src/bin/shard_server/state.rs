use std::collections::HashMap;

use rusty_fusion::{chunk::EntityMap, net::LoginData, npc::NPC, player::Player};

pub struct ShardServerState {
    login_server_conn_id: i64,
    login_data: HashMap<i64, LoginData>,
    entity_map: EntityMap,
}

impl ShardServerState {
    pub fn new() -> Self {
        Self {
            login_server_conn_id: super::CONN_ID_DISCONNECTED,
            login_data: HashMap::new(),
            entity_map: EntityMap::default(),
        }
    }

    pub fn get_login_server_conn_id(&self) -> i64 {
        self.login_server_conn_id
    }

    pub fn get_login_data(&mut self) -> &mut HashMap<i64, LoginData> {
        &mut self.login_data
    }

    pub fn get_entity_map(&mut self) -> &mut EntityMap {
        &mut self.entity_map
    }

    pub fn set_login_server_conn_id(&mut self, conn_id: i64) {
        self.login_server_conn_id = conn_id;
    }

    pub fn get_player_mut(&mut self, pc_uid: i64) -> &mut Player {
        self.entity_map.get_player(pc_uid).unwrap()
    }

    pub fn update_player(&mut self, pc_uid: i64, f: impl FnOnce(&mut Player, &mut Self)) {
        // to avoid a double-borrow, we create a copy of the player and then replace it
        let mut player = *self.entity_map.get_player(pc_uid).unwrap();
        f(&mut player, self);
        *self.entity_map.get_player(pc_uid).unwrap() = player;
    }

    pub fn _update_npc(&mut self, npc_id: i32, f: impl FnOnce(&mut NPC, &mut Self)) {
        // same as above
        let mut npc = *self.entity_map.get_npc(npc_id).unwrap();
        f(&mut npc, self);
        *self.entity_map.get_npc(npc_id).unwrap() = npc;
    }
}
