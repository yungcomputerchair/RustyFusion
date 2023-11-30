use std::{collections::HashMap, time::SystemTime};

use rusty_fusion::{
    chunk::EntityMap,
    error::{FFError, FFResult, Severity},
    net::LoginData,
    player::Player,
};

pub struct ShardServerState {
    login_server_conn_id: i64,
    login_server_conn_time: SystemTime,
    login_data: HashMap<i64, LoginData>,
    entity_map: EntityMap,
}

impl ShardServerState {
    pub fn new() -> Self {
        Self {
            login_server_conn_id: super::CONN_ID_DISCONNECTED,
            login_server_conn_time: SystemTime::UNIX_EPOCH,
            login_data: HashMap::new(),
            entity_map: EntityMap::default(),
        }
    }

    pub fn get_login_server_conn_id(&self) -> i64 {
        self.login_server_conn_id
    }

    pub fn get_login_server_conn_time(&self) -> SystemTime {
        self.login_server_conn_time
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

    pub fn set_login_server_conn_time(&mut self, conn_time: SystemTime) {
        self.login_server_conn_time = conn_time;
    }

    pub fn get_player_mut(&mut self, pc_uid: i64) -> FFResult<&mut Player> {
        self.entity_map.get_player(pc_uid).ok_or(FFError::build(
            Severity::Warning,
            format!("Player with ID {} doesn't exist", pc_uid),
        ))
    }

    pub fn update_player(
        &mut self,
        pc_uid: i64,
        f: impl FnOnce(&mut Player, &mut Self),
    ) -> FFResult<()> {
        // to avoid a double-borrow, we create a copy of the player and then replace it
        let mut player = *self.get_player_mut(pc_uid)?;
        f(&mut player, self);
        *self.get_player_mut(pc_uid)? = player;
        Ok(())
    }
}
