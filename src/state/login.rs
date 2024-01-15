use std::collections::{HashMap, HashSet};

use crate::{
    error::{FFError, FFResult, Severity},
    player::Player,
};

struct Account {
    username: String,
    players: HashMap<i64, Player>,
}

struct ShardServerInfo {
    player_uids: HashSet<i64>,
}
impl Default for ShardServerInfo {
    fn default() -> Self {
        Self {
            player_uids: HashSet::new(),
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
        self.accounts.insert(acc_id, Account { username, players });
    }

    pub fn unset_account(&mut self, acc_id: i64) {
        self.accounts.remove(&acc_id);
    }

    pub fn get_username(&self, acc_id: i64) -> FFResult<String> {
        let acc = self.get_account(acc_id)?;
        Ok(acc.username.clone())
    }

    pub fn get_players_mut(&mut self, acc_id: i64) -> FFResult<&mut HashMap<i64, Player>> {
        let acc = self.get_account_mut(acc_id)?;
        Ok(&mut acc.players)
    }

    pub fn get_next_shard_id(&mut self) -> usize {
        let next = self.next_shard_id;
        self.next_shard_id += 1;
        next
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
}
