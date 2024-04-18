use std::{
    collections::{HashMap, HashSet},
    time::SystemTime,
};

use uuid::Uuid;

use crate::{
    defines::{MAX_NUM_CHANNELS, MAX_NUM_SHARDS},
    entity::Player,
    enums::ShardChannelStatus,
    error::{FFError, FFResult, Severity},
};

pub struct Account {
    pub id: i64,
    pub username: String,
    pub password_hashed: String,
    pub selected_slot: u8,
    pub account_level: i16,
    pub banned_until: SystemTime,
    pub ban_reason: String,
}

struct LoginSession {
    account: Account,
    players: HashMap<i64, Player>,
    selected_player_uid: Option<i64>,
}

struct ShardServerInfo {
    player_uids: HashSet<i64>,
    channel_statuses: [ShardChannelStatus; MAX_NUM_CHANNELS],
}
impl Default for ShardServerInfo {
    fn default() -> Self {
        Self {
            player_uids: HashSet::new(),
            channel_statuses: [ShardChannelStatus::Closed; MAX_NUM_CHANNELS],
        }
    }
}

pub struct PlayerSearchRequest {
    pub requesting_shard_id: i32,
    pub searching_shard_ids: HashSet<i32>,
}

pub struct LoginServerState {
    pub server_id: Uuid,
    sessions: HashMap<i64, LoginSession>,
    shards: HashMap<i32, ShardServerInfo>,
    pub player_search_reqeust: Option<PlayerSearchRequest>,
}
impl Default for LoginServerState {
    fn default() -> Self {
        Self {
            server_id: Uuid::new_v4(),
            sessions: HashMap::new(),
            shards: HashMap::new(),
            player_search_reqeust: None,
        }
    }
}
impl LoginServerState {
    fn get_session(&self, acc_id: i64) -> FFResult<&LoginSession> {
        self.sessions.get(&acc_id).ok_or(FFError::build(
            Severity::Warning,
            format!("Account {} not logged in", acc_id),
        ))
    }

    fn get_session_mut(&mut self, acc_id: i64) -> FFResult<&mut LoginSession> {
        self.sessions.get_mut(&acc_id).ok_or(FFError::build(
            Severity::Warning,
            format!("Account {} not logged in", acc_id),
        ))
    }

    pub fn is_session_active(&self, acc_id: i64) -> bool {
        self.sessions.contains_key(&acc_id)
    }

    pub fn start_session(&mut self, account: Account, player_it: impl Iterator<Item = Player>) {
        let mut players = HashMap::new();
        for player in player_it {
            players.insert(player.get_uid(), player);
        }
        self.sessions.insert(
            account.id,
            LoginSession {
                account,
                players,
                selected_player_uid: None,
            },
        );
    }

    pub fn end_session(&mut self, acc_id: i64) -> FFResult<()> {
        if self.sessions.remove(&acc_id).is_none() {
            return Err(FFError::build(
                Severity::Warning,
                format!("Account {} not logged in", acc_id),
            ));
        }
        Ok(())
    }

    pub fn set_selected_player_id(&mut self, acc_id: i64, player_uid: i64) -> FFResult<()> {
        let session = self.get_session_mut(acc_id)?;
        session.selected_player_uid = Some(player_uid);
        Ok(())
    }

    pub fn get_selected_player_id(&self, acc_id: i64) -> FFResult<Option<i64>> {
        let session = self.get_session(acc_id)?;
        Ok(session.selected_player_uid)
    }

    pub fn get_username(&self, acc_id: i64) -> FFResult<String> {
        let session = self.get_session(acc_id)?;
        Ok(session.account.username.clone())
    }

    pub fn get_players_mut(&mut self, acc_id: i64) -> FFResult<&mut HashMap<i64, Player>> {
        let acc = self.get_session_mut(acc_id)?;
        Ok(&mut acc.players)
    }

    pub fn get_lowest_pop_shard_id(&mut self) -> Option<i32> {
        self.shards
            .iter()
            .min_by_key(|(_, shard)| shard.player_uids.len())
            .map(|(shard_id, _)| *shard_id)
    }

    pub fn register_shard(&mut self, shard_id: i32) -> FFResult<()> {
        if self.shards.contains_key(&shard_id) {
            return Err(FFError::build(
                Severity::Warning,
                format!("Shard {} already registered", shard_id),
            ));
        }

        if !(1..=MAX_NUM_SHARDS as i32).contains(&shard_id) {
            return Err(FFError::build(
                Severity::Warning,
                format!("Shard ID {} out of range", shard_id),
            ));
        }

        self.shards.insert(shard_id, ShardServerInfo::default());
        Ok(())
    }

    pub fn unregister_shard(&mut self, shard_id: i32) {
        self.shards.remove(&shard_id);
    }

    pub fn get_shard_ids(&self) -> Vec<i32> {
        self.shards.keys().copied().collect()
    }

    pub fn unset_player_shard(&mut self, player_uid: i64) -> Option<i32> {
        for (shard_id, shard) in self.shards.iter_mut() {
            if shard.player_uids.remove(&player_uid) {
                return Some(*shard_id);
            }
        }
        None
    }

    pub fn set_player_shard(&mut self, player_uid: i64, shard_id: i32) -> Option<i32> {
        let old_shard_id = self.unset_player_shard(player_uid);
        let shard = self.shards.get_mut(&shard_id).unwrap();
        shard.player_uids.insert(player_uid);
        old_shard_id
    }

    pub fn get_player_shard(&self, player_uid: i64) -> Option<i32> {
        for (shard_id, shard) in self.shards.iter() {
            if shard.player_uids.contains(&player_uid) {
                return Some(*shard_id);
            }
        }
        None
    }

    pub fn get_shard_channel_statuses(
        &self,
        shard_id: i32,
    ) -> [ShardChannelStatus; MAX_NUM_CHANNELS] {
        let shard = self.shards.get(&shard_id).unwrap();
        shard.channel_statuses
    }

    pub fn update_shard_channel_statuses(
        &mut self,
        shard_id: i32,
        statuses: [ShardChannelStatus; MAX_NUM_CHANNELS],
    ) {
        let shard = self.shards.get_mut(&shard_id).unwrap();
        for (i, status) in statuses.iter().enumerate() {
            shard.channel_statuses[i] = *status;
        }
    }
}
