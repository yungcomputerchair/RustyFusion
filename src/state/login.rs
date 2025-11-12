use std::{
    collections::{HashMap, HashSet},
    time::{Duration, SystemTime},
};

use uuid::Uuid;

use crate::{
    defines::*,
    entity::{Player, PlayerMetadata},
    enums::ShardChannelStatus,
    error::{log_if_failed, FFError, FFResult, Severity},
    net::{
        packet::{PacketID::*, *},
        ClientType, FFClient,
    },
    util,
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

struct ShardConnectionRequest {
    pub shard_id: Option<i32>,
    pub channel_num: Option<u8>,
    pub expire_time: SystemTime,
}

struct LoginSession {
    account: Account,
    players: HashMap<i64, Player>,
    selected_player_uid: Option<i64>,
    shard_connection_request: Option<ShardConnectionRequest>,
}

struct ShardServerInfo {
    num_channels: u8,
    max_channel_pop: usize,
    players: HashMap<i64, PlayerMetadata>,
}
impl ShardServerInfo {
    fn get_channel_population(&self, channel_num: u8) -> usize {
        self.players
            .values()
            .filter(|player| player.channel == channel_num)
            .count()
    }

    fn get_channel_status(&self, channel_num: u8) -> ShardChannelStatus {
        let max_pop = self.max_channel_pop;
        let pop = self.get_channel_population(channel_num);
        if pop >= max_pop {
            ShardChannelStatus::Closed
        } else {
            let pop_fraction = pop as f64 / max_pop as f64;
            if pop_fraction >= 0.75 {
                ShardChannelStatus::Busy
            } else if pop_fraction >= 0.25 {
                ShardChannelStatus::Normal
            } else {
                ShardChannelStatus::Empty
            }
        }
    }

    fn get_channel_statuses(&self) -> [ShardChannelStatus; MAX_NUM_CHANNELS] {
        let mut channels = [ShardChannelStatus::Closed; MAX_NUM_CHANNELS];
        for i in 0..self.num_channels {
            channels[i as usize] = self.get_channel_status(i);
        }
        channels
    }
}

pub struct PlayerSearchRequest {
    pub searching_shard_ids: HashSet<i32>,
}

pub struct LoginServerState {
    pub server_id: Uuid,
    sessions: HashMap<i64, LoginSession>,
    shards: HashMap<i32, ShardServerInfo>,
    pub player_search_reqeusts: HashMap<(i32, i32), PlayerSearchRequest>,
    pub pending_channel_requests: HashMap<i64, u8>,
}
impl Default for LoginServerState {
    fn default() -> Self {
        Self {
            server_id: Uuid::new_v4(),
            sessions: HashMap::new(),
            shards: HashMap::new(),
            player_search_reqeusts: HashMap::new(),
            pending_channel_requests: HashMap::new(),
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
                shard_connection_request: None,
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
            .min_by_key(|(_, shard)| shard.players.len())
            .map(|(shard_id, _)| *shard_id)
    }

    pub fn register_shard(
        &mut self,
        shard_id: i32,
        num_channels: u8,
        max_channel_pop: usize,
    ) -> FFResult<()> {
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

        self.shards.insert(
            shard_id,
            ShardServerInfo {
                num_channels,
                max_channel_pop,
                players: HashMap::new(),
            },
        );
        Ok(())
    }

    pub fn unregister_shard(&mut self, shard_id: i32) {
        self.shards.remove(&shard_id);
    }

    pub fn get_shard_ids(&self) -> Vec<i32> {
        self.shards.keys().copied().collect()
    }

    pub fn clear_shard_players(&mut self, shard_id: i32) {
        let shard = self.shards.get_mut(&shard_id).unwrap();
        shard.players.clear();
    }

    pub fn set_player_shard(
        &mut self,
        player_uid: i64,
        player_data: PlayerMetadata,
        shard_id: i32,
    ) -> Option<i32> {
        let old_shard_id = self.get_player_shard(player_uid);
        if let Some(old_shard_id) = old_shard_id {
            let old_shard = self.shards.get_mut(&old_shard_id).unwrap();
            old_shard.players.remove(&player_uid);
        }

        let shard = self.shards.get_mut(&shard_id).unwrap();
        shard.players.insert(player_uid, player_data);
        old_shard_id
    }

    pub fn get_player_shard(&self, player_uid: i64) -> Option<i32> {
        for (shard_id, shard) in self.shards.iter() {
            if shard.players.contains_key(&player_uid) {
                return Some(*shard_id);
            }
        }
        None
    }

    pub fn get_all_shard_player_data<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = &'a PlayerMetadata> + 'a> {
        Box::new(
            self.shards
                .values()
                .flat_map(|shard| shard.players.values()),
        )
    }

    pub fn get_shard_channel_statuses(
        &self,
        shard_id: i32,
    ) -> [ShardChannelStatus; MAX_NUM_CHANNELS] {
        let shard = self.shards.get(&shard_id).unwrap();
        shard.get_channel_statuses()
    }

    pub fn request_shard_connection(
        &mut self,
        acc_id: i64,
        shard_id: Option<i32>,
        channel_num: Option<u8>,
    ) -> FFResult<()> {
        const SHARD_CONN_TIMEOUT_SEC: u64 = 20;
        let session = self.get_session_mut(acc_id)?;
        session.shard_connection_request = Some(ShardConnectionRequest {
            shard_id,
            channel_num,
            expire_time: SystemTime::now() + Duration::from_secs(SHARD_CONN_TIMEOUT_SEC),
        });
        Ok(())
    }

    pub fn set_pending_channel_request(&mut self, player_uid: i64, channel_num: u8) {
        self.pending_channel_requests
            .insert(player_uid, channel_num);
    }

    pub fn get_pending_channel_request(&mut self, player_uid: i64) -> Option<u8> {
        self.pending_channel_requests.remove(&player_uid)
    }

    pub fn process_shard_connection_requests(
        &mut self,
        clients: &mut HashMap<usize, FFClient>,
        time: SystemTime,
    ) {
        let lowest_pop_shard_id = self.get_lowest_pop_shard_id();
        let client_keys = clients.keys().copied().collect::<Vec<_>>();
        for client_key in client_keys {
            let client = clients.get_mut(&client_key).unwrap();
            let fe_key = client.get_fe_key_uint();
            let Ok(acc_id) = client.get_account_id() else {
                continue;
            };
            let Ok(serial_key) = client.get_serial_key() else {
                continue;
            };
            let pc_uid = if let Some(session) = self.sessions.get(&acc_id) {
                session.selected_player_uid
            } else {
                continue;
            };
            let pc_uid = match pc_uid {
                Some(uid) => uid,
                None => continue,
            };
            let Some(session) = self.sessions.get_mut(&acc_id) else {
                continue;
            };
            let Some(request) = &session.shard_connection_request else {
                continue;
            };

            let channel_num = request.channel_num.unwrap_or(0) as i8;

            if request.expire_time < time {
                let resp = sP_LS2CL_REP_SHARD_SELECT_FAIL {
                    iErrorCode: 1, // "Shard connection error"
                };
                log_if_failed(client.send_packet(P_LS2CL_REP_SHARD_SELECT_FAIL, &resp));
                session.shard_connection_request = None;
                continue;
            }

            let shard_id = match request.shard_id {
                Some(shard_id) => shard_id,
                None => {
                    // no specific shard requested. if there's one online (lowest population) use it
                    if let Some(shard_id) = lowest_pop_shard_id {
                        shard_id
                    } else {
                        continue;
                    }
                }
            };

            let Some(shard) = clients
                .values_mut()
                .find(|c| matches!(c.client_type, ClientType::ShardServer(sid) if sid == shard_id))
            else {
                continue;
            };

            let login_info = sP_LS2FE_REQ_UPDATE_LOGIN_INFO {
                iAccountID: acc_id,
                iEnterSerialKey: serial_key,
                iPC_UID: pc_uid,
                uiFEKey: fe_key,
                uiSvrTime: util::get_timestamp_ms(time),
                iChannelRequestNum: channel_num as u8,
            };

            if shard
                .send_packet(P_LS2FE_REQ_UPDATE_LOGIN_INFO, &login_info)
                .is_err()
            {
                let resp = sP_LS2CL_REP_SHARD_SELECT_FAIL {
                    iErrorCode: 1, // "Shard connection error"
                };
                let client = clients.get_mut(&client_key).unwrap();
                log_if_failed(client.send_packet(P_LS2CL_REP_SHARD_SELECT_FAIL, &resp));
            }
            session.shard_connection_request = None;
        }
    }
}
