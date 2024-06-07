use std::{
    collections::{HashMap, HashSet},
    time::SystemTime,
};

use rusty_fusion::{
    config::config_get,
    defines::MAX_NUM_CHANNELS,
    enums::{PlayerShardStatus, ShardChannelStatus},
    error::{codes::PlayerSearchReqErr, log, log_if_failed, FFError, FFResult, Severity},
    net::{
        crypto,
        packet::{PacketID::*, *},
        ClientType, FFClient,
    },
    state::{LoginServerState, PlayerSearchRequest},
    unused, util,
};

pub fn auth_challenge(server: &mut FFClient) -> FFResult<()> {
    let key = config_get().general.server_key.get().clone();
    let mut challenge = crypto::gen_auth_challenge();
    server.client_type = ClientType::UnauthedShardServer(Box::new(challenge));

    crypto::encrypt_payload(&mut challenge, key.as_bytes());
    let resp = sP_LS2FE_REP_AUTH_CHALLENGE {
        aChallenge: challenge,
    };
    server.send_packet(P_LS2FE_REP_AUTH_CHALLENGE, &resp)
}

pub fn connect(
    server: &mut FFClient,
    state: &mut LoginServerState,
    time: SystemTime,
) -> FFResult<()> {
    let pkt: &sP_FE2LS_REQ_CONNECT = server.get_packet(P_FE2LS_REQ_CONNECT)?;
    let shard_id = pkt.iShardID;
    let challenge_solved = pkt.aChallengeSolved;
    let ClientType::UnauthedShardServer(challenge) = &server.client_type else {
        return Err(FFError::build(
            Severity::Warning,
            format!(
                "Shard server tried to connect a second time: {:?}",
                server.client_type
            ),
        ));
    };

    if challenge_solved != *challenge.clone() {
        let resp = sP_LS2FE_REP_CONNECT_FAIL { iErrorCode: 1 };
        log_if_failed(server.send_packet(P_LS2FE_REP_CONNECT_FAIL, &resp));
        return Err(FFError::build(
            Severity::Warning,
            format!(
                "Shard server {} tried to connect with wrong password",
                server.get_addr()
            ),
        ));
    }

    if let Err(e) = state.register_shard(shard_id) {
        let resp = sP_LS2FE_REP_CONNECT_FAIL { iErrorCode: 2 };
        log_if_failed(server.send_packet(P_LS2FE_REP_CONNECT_FAIL, &resp));
        return Err(e);
    };
    server.client_type = ClientType::ShardServer(shard_id);
    let resp = sP_LS2FE_REP_CONNECT_SUCC {
        uiSvrTime: util::get_timestamp_ms(time),
        aLS_UID: state.server_id.to_bytes_le(),
    };
    server.send_packet(P_LS2FE_REP_CONNECT_SUCC, &resp)?;

    let iv1: i32 = resp.aLS_UID.into_iter().reduce(|a, b| a ^ b).unwrap() as i32;
    let iv2: i32 = shard_id + 1;
    server.e_key = crypto::gen_key(resp.uiSvrTime, iv1, iv2);

    log(
        Severity::Info,
        &format!(
            "Connected to shard server #{} ({})",
            shard_id,
            server.get_addr()
        ),
    );

    Ok(())
}

pub fn shard_live_check(client: &mut FFClient) -> FFResult<()> {
    let resp = sP_LS2FE_REP_LIVE_CHECK {
        iTempValue: unused!(),
    };
    client.send_packet(P_LS2FE_REP_LIVE_CHECK, &resp)?;
    Ok(())
}

pub fn update_login_info_succ(
    shard_key: usize,
    clients: &mut HashMap<usize, FFClient>,
) -> FFResult<()> {
    let server = clients.get_mut(&shard_key).unwrap();
    let pkt: &sP_FE2LS_REP_UPDATE_LOGIN_INFO_SUCC =
        server.get_packet(P_FE2LS_REP_UPDATE_LOGIN_INFO_SUCC)?;

    let resp = sP_LS2CL_REP_SHARD_SELECT_SUCC {
        g_FE_ServerIP: pkt.g_FE_ServerIP,
        g_FE_ServerPort: pkt.g_FE_ServerPort,
        iEnterSerialKey: pkt.iEnterSerialKey,
    };

    let client = clients
        .values_mut()
        .find(|c| match c.client_type {
            ClientType::GameClient {
                serial_key: key, ..
            } => key == resp.iEnterSerialKey,
            _ => false,
        })
        .unwrap();
    client.send_packet(P_LS2CL_REP_SHARD_SELECT_SUCC, &resp)?;
    client.disconnect();

    Ok(())
}

pub fn update_login_info_fail(
    shard_key: usize,
    clients: &mut HashMap<usize, FFClient>,
) -> FFResult<()> {
    let server = clients.get_mut(&shard_key).unwrap();
    let pkt: &sP_FE2LS_REP_UPDATE_LOGIN_INFO_FAIL =
        server.get_packet(P_FE2LS_REP_UPDATE_LOGIN_INFO_FAIL)?;
    let resp = sP_LS2CL_REP_CHAR_SELECT_FAIL {
        iErrorCode: pkt.iErrorCode,
    };

    let serial_key = pkt.iEnterSerialKey;
    let client: &mut FFClient = clients
        .values_mut()
        .find(|c| match c.client_type {
            ClientType::GameClient {
                serial_key: key, ..
            } => key == serial_key,
            _ => false,
        })
        .unwrap();

    client.send_packet(P_LS2CL_REP_CHAR_SELECT_FAIL, &resp)?;

    Ok(())
}

pub fn update_pc_shard(client: &mut FFClient, state: &mut LoginServerState) -> FFResult<()> {
    let pkt: sP_FE2LS_UPDATE_PC_SHARD = *client.get_packet(P_FE2LS_UPDATE_PC_SHARD)?;
    let shard_id = client.get_shard_id().expect("Packet filter failed");
    let pc_uid = pkt.iPC_UID;
    let status: PlayerShardStatus = pkt.ePSS.try_into()?;
    log(
        Severity::Info,
        &format!("Player {} moved (shard {}, {:?})", pc_uid, shard_id, status),
    );

    match status {
        PlayerShardStatus::Entered => {
            let old = state.set_player_shard(pc_uid, shard_id);
            if let Some(old_shard_id) = old {
                log(
                    Severity::Warning,
                    &format!(
                        "Player {} was already tracked in shard {}",
                        pc_uid, old_shard_id
                    ),
                );
            }
        }
        PlayerShardStatus::Exited => {
            if state.unset_player_shard(pc_uid).is_none() {
                log(
                    Severity::Warning,
                    &format!("Player {} was not tracked in shard {}", pc_uid, shard_id),
                );
            }
        }
    };
    Ok(())
}

pub fn update_channel_statuses(
    client: &mut FFClient,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let pkt: sP_FE2LS_UPDATE_CHANNEL_STATUSES =
        *client.get_packet(P_FE2LS_UPDATE_CHANNEL_STATUSES)?;
    let shard_id = client.get_shard_id().expect("Packet filter failed");
    let mut statuses = [ShardChannelStatus::Closed; MAX_NUM_CHANNELS];
    for (channel_num, status_raw) in pkt.aChannelStatus.iter().enumerate() {
        let status: ShardChannelStatus = (*status_raw).try_into()?;
        statuses[channel_num] = status;
    }
    state.update_shard_channel_statuses(shard_id, statuses);
    Ok(())
}

pub fn motd(client: &mut FFClient) -> FFResult<()> {
    let pkt: &sP_FE2LS_REQ_MOTD = client.get_packet(P_FE2LS_REQ_MOTD)?;

    // load the MOTD from the MOTD file
    let motd_path = config_get().login.motd_path.get();
    let motd = if let Ok(motd) = std::fs::read_to_string(motd_path.clone()) {
        motd.trim().to_string()
    } else {
        log(
            Severity::Warning,
            &format!("MOTD file {} not found, using default MOTD", motd_path),
        );
        "Welcome to RustyFusion!".to_string()
    };
    let resp = sP_LS2FE_REP_MOTD {
        iPC_ID: pkt.iPC_ID,
        szMessage: util::encode_utf16(&motd),
    };
    client.send_packet(P_LS2FE_REP_MOTD, &resp)
}

pub fn motd_register(client: &mut FFClient) -> FFResult<()> {
    let pkt: &sP_FE2LS_MOTD_REGISTER = client.get_packet(P_FE2LS_MOTD_REGISTER)?;
    let motd = util::parse_utf16(&pkt.szMessage)?;
    let motd_path = config_get().login.motd_path.get();
    if std::fs::write(motd_path.clone(), motd.as_bytes()).is_err() {
        log(
            Severity::Warning,
            &format!("Failed to write MOTD to {}", motd_path),
        );
    } else {
        log(Severity::Info, &format!("MOTD updated:\n{}", motd));
    }
    Ok(())
}

pub fn announce_msg(shard_key: usize, clients: &mut HashMap<usize, FFClient>) -> FFResult<()> {
    let server = clients.get_mut(&shard_key).unwrap();
    let pkt: sP_FE2LS_ANNOUNCE_MSG = *server.get_packet(P_FE2LS_ANNOUNCE_MSG)?;
    clients.iter_mut().for_each(|(_, client)| {
        if let ClientType::ShardServer(_) = client.client_type {
            log_if_failed(client.send_packet(P_LS2FE_ANNOUNCE_MSG, &pkt));
        }
    });
    Ok(())
}

pub fn pc_location(
    shard_key: usize,
    clients: &mut HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let server = clients.get_mut(&shard_key).unwrap();
    let pkt: sP_FE2LS_REQ_PC_LOCATION = *server.get_packet(P_FE2LS_REQ_PC_LOCATION)?;
    let req_shard_id = server.get_shard_id()?;
    let request_key = (req_shard_id, pkt.iPC_ID);
    if state.player_search_reqeusts.contains_key(&request_key) {
        let resp = sP_LS2FE_REP_PC_LOCATION_FAIL {
            iPC_ID: pkt.iPC_ID,
            sReq: pkt.sReq,
            iErrorCode: PlayerSearchReqErr::SearchInProgress as i32,
        };
        server.send_packet(P_LS2FE_REP_PC_LOCATION_FAIL, &resp)?;
        return Err(FFError::build(
            Severity::Warning,
            format!(
                "Player search request {:?} already in progress",
                request_key
            ),
        ));
    }

    // search all shards except the one that requested the search
    let search_shard_ids: HashSet<i32> = state
        .get_shard_ids()
        .into_iter()
        .filter(|id| *id != req_shard_id)
        .collect();
    state.player_search_reqeusts.insert(
        request_key,
        PlayerSearchRequest {
            searching_shard_ids: search_shard_ids,
        },
    );

    let pkt = sP_LS2FE_REQ_PC_LOCATION {
        iReqShard_ID: req_shard_id,
        iPC_ID: pkt.iPC_ID,
        sReq: pkt.sReq,
    };
    clients.iter_mut().for_each(|(_, client)| {
        if let ClientType::ShardServer(shard_id) = client.client_type {
            if shard_id == req_shard_id {
                return;
            }
            log_if_failed(client.send_packet(P_LS2FE_REQ_PC_LOCATION, &pkt));
        }
    });
    Ok(())
}

pub fn pc_location_succ(
    shard_key: usize,
    clients: &mut HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let server = clients.get_mut(&shard_key).unwrap();
    let pkt: sP_FE2LS_REP_PC_LOCATION_SUCC = *server.get_packet(P_FE2LS_REP_PC_LOCATION_SUCC)?;
    let req_shard_id = pkt.iReqShard_ID;
    let request_key = (req_shard_id, pkt.iPC_ID);

    state
        .player_search_reqeusts
        .remove(&request_key)
        .ok_or(FFError::build(
            Severity::Warning,
            format!("Player search request {:?} not found", request_key),
        ))?;

    // find the shard that requested the search and forward the results
    let client = clients
        .values_mut()
        .find(|c| match c.client_type {
            ClientType::ShardServer(shard_id) => shard_id == req_shard_id,
            _ => false,
        })
        .ok_or(FFError::build(
            Severity::Warning,
            format!(
                "Shard {}, which initiated the player search, not found",
                req_shard_id
            ),
        ))?;

    let resp = sP_LS2FE_REP_PC_LOCATION_SUCC {
        iPC_ID: pkt.iPC_ID,
        sResp: pkt.sResp,
    };
    log_if_failed(client.send_packet(P_LS2FE_REP_PC_LOCATION_SUCC, &resp));
    Ok(())
}

pub fn pc_location_fail(
    shard_key: usize,
    clients: &mut HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let server = clients.get_mut(&shard_key).unwrap();
    let pkt: sP_FE2LS_REP_PC_LOCATION_FAIL = *server.get_packet(P_FE2LS_REP_PC_LOCATION_FAIL)?;
    let req_shard_id = pkt.iReqShard_ID;
    let request_key = (req_shard_id, pkt.iPC_ID);

    let search = match state.player_search_reqeusts.get_mut(&request_key) {
        Some(search) => search,
        None => {
            return Ok(()); // search completed already
        }
    };

    let shard_id = server.get_shard_id()?;
    search.searching_shard_ids.remove(&shard_id);
    if search.searching_shard_ids.is_empty() {
        // every shard got back to us with a failure, so return not found
        state.player_search_reqeusts.remove(&request_key).unwrap();
        let resp = sP_LS2FE_REP_PC_LOCATION_FAIL {
            iPC_ID: pkt.iPC_ID,
            sReq: pkt.sReq,
            iErrorCode: PlayerSearchReqErr::NotFound as i32,
        };
        let shard = clients
            .values_mut()
            .find(|c| match c.client_type {
                ClientType::ShardServer(shard_id) => shard_id == req_shard_id,
                _ => false,
            })
            .ok_or(FFError::build(
                Severity::Warning,
                format!(
                    "Shard {}, which initiated the player search, not found",
                    req_shard_id
                ),
            ))?;
        log_if_failed(shard.send_packet(P_LS2FE_REP_PC_LOCATION_FAIL, &resp));
    }
    Ok(())
}
