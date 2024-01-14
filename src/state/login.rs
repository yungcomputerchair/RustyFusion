use std::collections::HashMap;

use crate::{
    error::{FFError, FFResult, Severity},
    player::Player,
};

struct Account {
    username: String,
    players: HashMap<i64, Player>,
}

pub struct LoginServerState {
    server_id: i64,
    next_shard_id: usize,
    accounts: HashMap<i64, Account>,
}
impl Default for LoginServerState {
    fn default() -> Self {
        Self {
            server_id: rand::random(),
            next_shard_id: 1,
            accounts: HashMap::new(),
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

    pub fn get_id(&self) -> i64 {
        self.server_id
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
}
