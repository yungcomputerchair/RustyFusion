use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    time::{Duration, SystemTime},
};

use uuid::Uuid;

use crate::{
    defines::*,
    entity::{Player, PlayerMetadata},
    enums::ShardChannelStatus,
    error::{FFError, FFResult, Severity},
    geo::{self, GeoInfo},
    net::{
        packet::{PacketID::*, *},
        ClientType, FFClient,
    },
    util,
};

pub struct Cookie {
    pub token: String,
    pub expires: SystemTime,
}

pub struct Account {
    pub id: i64,
    pub username: String,
    pub password_hashed: String,
    pub cookie: Option<Cookie>,
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
    handoff_fe_key: u64,
}

struct ShardServerInfo {
    num_channels: u8,
    max_channel_pop: usize,
    players: HashMap<i64, PlayerMetadata>,
    public_addr: SocketAddr,
    geo: Option<GeoInfo>,
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
    next_shard_id: i32,
    shard_id_reservations: HashMap<SocketAddr, i32>,
    shards: HashMap<i32, ShardServerInfo>,
    pub player_search_reqeusts: HashMap<(i32, i32), PlayerSearchRequest>,
    pub pending_channel_requests: HashMap<i64, u8>,
    pub buddy_warp_times: HashMap<i64, u32>,
}
impl Default for LoginServerState {
    fn default() -> Self {
        Self {
            server_id: Uuid::new_v4(),
            sessions: HashMap::new(),
            next_shard_id: 1,
            shard_id_reservations: HashMap::new(),
            shards: HashMap::new(),
            player_search_reqeusts: HashMap::new(),
            pending_channel_requests: HashMap::new(),
            buddy_warp_times: HashMap::new(),
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

    fn get_lowest_pop_shard(&self) -> Option<i32> {
        self.shards
            .iter()
            .min_by_key(|(_, shard)| shard.players.len())
            .map(|(shard_id, _)| *shard_id)
    }

    fn get_nominal_shard_for_client(&self, client: &FFClient) -> Option<i32> {
        if let Some(client_geo) = geo::do_lookup(client.get_ip()) {
            // find shard with lowest haversine distance to client
            let geo_shards: Vec<_> = self
                .shards
                .iter()
                .filter_map(|(shard_id, shard)| {
                    shard.geo.as_ref().map(|geo| (shard_id, geo.coords))
                })
                .collect();

            if geo_shards.len() >= 2 {
                let closest_shard_id = geo_shards
                    .into_iter()
                    .min_by_key(|(_, coords)| {
                        // convert distance to an integer for min_by_key
                        let dist = geo::haversine_distance(client_geo.coords, *coords);
                        (dist * 1000.0) as u64
                    })
                    .map(|(shard_id, _)| *shard_id);

                if let Some(closest_shard_id) = closest_shard_id {
                    return Some(closest_shard_id);
                }
            }
        }

        self.get_lowest_pop_shard()
    }

    pub fn is_session_active(&self, acc_id: i64) -> bool {
        self.sessions.contains_key(&acc_id)
    }

    pub fn start_session(
        &mut self,
        account: Account,
        player_it: impl Iterator<Item = Player>,
        handoff_fe_key: u64,
    ) {
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
                handoff_fe_key,
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

    pub fn get_current_and_max_pop_for_shard(&self, shard_id: i32) -> Option<(usize, usize)> {
        self.shards.get(&shard_id).map(|shard| {
            (
                shard.players.len(),
                shard.num_channels as usize * shard.max_channel_pop,
            )
        })
    }

    pub fn get_shard_public_addr(&self, shard_id: i32) -> Option<SocketAddr> {
        self.shards.get(&shard_id).map(|shard| shard.public_addr)
    }

    pub fn get_shard_city(&self, shard_id: i32) -> Option<&str> {
        let geo = self
            .shards
            .get(&shard_id)
            .and_then(|shard| shard.geo.as_ref())?;

        Some(geo.city_name.as_deref().unwrap_or("Unknown"))
    }

    pub fn register_shard(
        &mut self,
        num_channels: u8,
        max_channel_pop: usize,
        public_addr: SocketAddr,
    ) -> FFResult<i32> {
        let shard_id = match self.shard_id_reservations.get(&public_addr) {
            Some(id) => *id,
            None => {
                let id = self.next_shard_id;
                if id > MAX_NUM_SHARDS as i32 {
                    return Err(FFError::build_dc(
                        Severity::Warning,
                        "Maximum number of shards reached".to_string(),
                    ));
                }
                self.next_shard_id += 1;
                self.shard_id_reservations.insert(public_addr, id);
                id
            }
        };

        if self.shards.contains_key(&shard_id) {
            return Err(FFError::build_dc(
                Severity::Warning,
                format!("Shard ID {} is already registered", shard_id),
            ));
        }

        self.shards.insert(
            shard_id,
            ShardServerInfo {
                num_channels,
                max_channel_pop,
                players: HashMap::new(),
                public_addr,
                geo: geo::do_lookup(public_addr.ip()),
            },
        );
        Ok(shard_id)
    }

    pub fn unregister_shard(&mut self, shard_id: i32) {
        self.shards.remove(&shard_id);
    }

    pub fn get_reserved_shard_ids(&self) -> Vec<i32> {
        self.shard_id_reservations.values().copied().collect()
    }

    pub fn get_connected_shard_ids(&self) -> Vec<i32> {
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
        clients: &HashMap<usize, FFClient>,
        time: SystemTime,
    ) {
        let client_keys = clients.keys().copied().collect::<Vec<_>>();
        for client_key in client_keys {
            let client = clients.get(&client_key).unwrap();
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

            let nominal_shard_id = self.get_nominal_shard_for_client(client);

            let Some(session) = self.sessions.get_mut(&acc_id) else {
                continue;
            };
            let fe_key = session.handoff_fe_key;

            let Some(request) = &session.shard_connection_request else {
                continue;
            };

            let channel_num = request.channel_num.unwrap_or(0) as i8;

            if request.expire_time < time {
                let resp = sP_LS2CL_REP_SHARD_SELECT_FAIL {
                    iErrorCode: 1, // "Shard connection error"
                };
                client.send_packet(P_LS2CL_REP_SHARD_SELECT_FAIL, &resp);
                session.shard_connection_request = None;
                continue;
            }

            let shard_id = match request.shard_id {
                Some(shard_id) => shard_id,
                None => {
                    // no specific shard requested. if there's one online (lowest population) use it
                    if let Some(shard_id) = nominal_shard_id {
                        shard_id
                    } else {
                        continue;
                    }
                }
            };

            let Some(shard) = clients.values().find(|c| {
                let meta = c.meta.read();
                meta.client_type == ClientType::ShardServer(shard_id)
            }) else {
                continue;
            };

            let login_info = sP_LS2FE_REQ_UPDATE_LOGIN_INFO {
                iAccountID: acc_id,
                iEnterSerialKey: serial_key,
                iPC_UID: pc_uid,
                uiFEKey: fe_key,
                uiSvrTime: util::get_timestamp_ms(time),
                iChannelRequestNum: channel_num as u8,
                iBuddyWarpTime: self.buddy_warp_times.get(&pc_uid).copied().unwrap_or(0),
            };

            shard.send_packet(P_LS2FE_REQ_UPDATE_LOGIN_INFO, &login_info);
            session.shard_connection_request = None;
        }
    }
}
