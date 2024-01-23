use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::{
    defines::{MAX_NUM_CHANNELS, MAX_NUM_SHARDS},
    enums::ShardChannelStatus,
    error::{FFError, FFResult, Severity},
    player::Player,
};

struct Account {
    username: String,
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
    accounts: HashMap<i64, Account>,
    shard_id_pool: Vec<i32>,
    shards: HashMap<i32, ShardServerInfo>,
    pub player_search_reqeust: Option<PlayerSearchRequest>,
}
impl Default for LoginServerState {
    fn default() -> Self {
        Self {
            server_id: Uuid::new_v4(),
            accounts: HashMap::new(),
            shard_id_pool: (1..=MAX_NUM_SHARDS as i32).collect(),
            shards: HashMap::new(),
            player_search_reqeust: None,
        }
    }
}
impl LoginServerState {
    fn get_account(&self, acc_id: i64) -> FFResult<&Account> {
        self.accounts.get(&acc_id).ok_or(FFError::build(
            Severity::Warning,
            format!("Account {} not logged in", acc_id),
        ))
    }

    fn get_account_mut(&mut self, acc_id: i64) -> FFResult<&mut Account> {
        self.accounts.get_mut(&acc_id).ok_or(FFError::build(
            Severity::Warning,
            format!("Account {} not logged in", acc_id),
        ))
    }

    pub fn set_account(
        &mut self,
        acc_id: i64,
        username: String,
        player_it: impl Iterator<Item = Player>,
    ) {
        let mut players = HashMap::new();
        for player in player_it {
            players.insert(player.get_uid(), player);
        }
        self.accounts.insert(
            acc_id,
            Account {
                username,
                players,
                selected_player_uid: None,
            },
        );
    }

    pub fn unset_account(&mut self, acc_id: i64) {
        self.accounts.remove(&acc_id);
    }

    pub fn set_selected_player_id(&mut self, acc_id: i64, player_uid: i64) {
        let acc = self.accounts.get_mut(&acc_id).unwrap();
        acc.selected_player_uid = Some(player_uid);
    }

    pub fn get_selected_player_id(&self, acc_id: i64) -> Option<i64> {
        let acc = self.get_account(acc_id).unwrap();
        acc.selected_player_uid
    }

    pub fn get_username(&self, acc_id: i64) -> String {
        let acc = self.get_account(acc_id).unwrap();
        acc.username.clone()
    }

    pub fn get_players_mut(&mut self, acc_id: i64) -> &mut HashMap<i64, Player> {
        let acc = self.get_account_mut(acc_id).unwrap();
        &mut acc.players
    }

    pub fn get_lowest_pop_shard_id(&mut self) -> Option<i32> {
        self.shards
            .iter()
            .min_by_key(|(_, shard)| shard.player_uids.len())
            .map(|(shard_id, _)| *shard_id)
    }

    pub fn register_shard(&mut self) -> Option<i32> {
        if self.shard_id_pool.is_empty() {
            None
        } else {
            let shard_id = self.shard_id_pool.remove(0);
            self.shards.insert(shard_id, ShardServerInfo::default());
            Some(shard_id)
        }
    }

    pub fn unregister_shard(&mut self, shard_id: i32) {
        self.shards.remove(&shard_id);
        self.shard_id_pool.push(shard_id);
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
