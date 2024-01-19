use std::collections::{HashMap, HashSet};

use crate::{
    defines::MAX_NUM_CHANNELS,
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

pub struct LoginServerState {
    pub server_id: i64,
    accounts: HashMap<i64, Account>,
    next_shard_id: usize,
    shards: HashMap<usize, ShardServerInfo>,
}
impl Default for LoginServerState {
    fn default() -> Self {
        Self {
            server_id: rand::random(),
            accounts: HashMap::new(),
            next_shard_id: 1,
            shards: HashMap::new(),
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

    pub fn get_next_shard_id(&mut self) -> usize {
        let next = self.next_shard_id;
        self.next_shard_id += 1;
        next
    }

    pub fn get_lowest_pop_shard_id(&mut self) -> usize {
        *self
            .shards
            .iter()
            .min_by_key(|(_, shard)| shard.player_uids.len())
            .unwrap()
            .0
    }

    pub fn register_shard(&mut self, shard_id: usize) {
        self.shards.insert(shard_id, ShardServerInfo::default());
    }

    pub fn unregister_shard(&mut self, shard_id: usize) {
        self.shards.remove(&shard_id);
    }

    pub fn unset_player_shard(&mut self, player_uid: i64) -> Option<usize> {
        for (shard_id, shard) in self.shards.iter_mut() {
            if shard.player_uids.remove(&player_uid) {
                return Some(*shard_id);
            }
        }
        None
    }

    pub fn set_player_shard(&mut self, player_uid: i64, shard_id: usize) -> Option<usize> {
        let old_shard_id = self.unset_player_shard(player_uid);
        let shard = self.shards.get_mut(&shard_id).unwrap();
        shard.player_uids.insert(player_uid);
        old_shard_id
    }

    pub fn get_shard_channel_statuses(
        &self,
        shard_id: usize,
    ) -> [ShardChannelStatus; MAX_NUM_CHANNELS] {
        let shard = self.shards.get(&shard_id).unwrap();
        shard.channel_statuses
    }

    pub fn update_shard_channel_statuses(
        &mut self,
        shard_id: usize,
        statuses: [ShardChannelStatus; MAX_NUM_CHANNELS],
    ) {
        let shard = self.shards.get_mut(&shard_id).unwrap();
        for (i, status) in statuses.iter().enumerate() {
            shard.channel_statuses[i] = *status;
        }
    }
}
