use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::SystemTime,
};

use rusty_fusion::{
    config::config_get,
    entity::PlayerMetadata,
    error::{
        codes::{BuddyWarpErr, PlayerSearchReqErr},
        log, FFError, FFResult, Severity,
    },
    monitor::{monitor_queue, monitor_update_from_packet},
    net::{
        crypto::{self, AUTH_CHALLENGE_MAX_SIZE},
        packet::{PacketID::*, *},
        ClientType, FFClient,
    },
    state::{LoginServerState, PlayerSearchRequest},
    unused, util,
};

pub fn auth_challenge(server: &FFClient) -> FFResult<()> {
    let key = config_get().general.server_key.get().clone();
    let chall_decrypted = crypto::gen_auth_challenge();

    let (nonce, chall_encrypted) = crypto::encrypt_payload_aes(&chall_decrypted, &key);
    assert!(chall_encrypted.len() <= AUTH_CHALLENGE_MAX_SIZE);

    server.set_client_type(ClientType::UnauthedShardServer(Arc::new(chall_decrypted)));

    let mut chall_arr = [0u8; AUTH_CHALLENGE_MAX_SIZE];
    chall_arr[..chall_encrypted.len()].copy_from_slice(&chall_encrypted);

    let resp = sP_LS2FE_REP_AUTH_CHALLENGE {
        uiChallengeLength: chall_encrypted.len() as u32,
        aChallenge: chall_arr,
        aNonce: nonce,
    };

    server.send_packet(P_LS2FE_REP_AUTH_CHALLENGE, &resp);
    Ok(())
}

pub fn connect(
    pkt: Packet,
    server: &FFClient,
    state: &mut LoginServerState,
    time: SystemTime,
) -> FFResult<()> {
    let pkt: &sP_FE2LS_REQ_CONNECT = pkt.get(P_FE2LS_REQ_CONNECT)?;
    let num_channels = pkt.iNumChannels;
    let max_channel_pop = pkt.iMaxChannelPop;
    let public_addr = util::socket_addr_from_parts(pkt.uiPublicIp, pkt.uiPublicPort);

    let chall_decrypted = pkt.aChallengeSolved[..pkt.uiChallengeSolvedLength as usize].to_vec();

    let ClientType::UnauthedShardServer(challenge) = &server.get_client_type() else {
        return Err(FFError::build(
            Severity::Warning,
            format!(
                "Shard server tried to connect before challenge: {:?}",
                server.get_client_type()
            ),
        ));
    };

    if chall_decrypted != challenge[..] {
        let resp = sP_LS2FE_REP_CONNECT_FAIL { iErrorCode: 1 };
        server.send_packet(P_LS2FE_REP_CONNECT_FAIL, &resp);
        return Err(FFError::build_dc(
            Severity::Warning,
            format!(
                "Shard server {} tried to connect with wrong password",
                server.get_addr()
            ),
        ));
    }

    let shard_id =
        match state.register_shard(num_channels as u8, max_channel_pop as usize, public_addr) {
            Ok(id) => id,
            Err(e) => {
                let resp = sP_LS2FE_REP_CONNECT_FAIL { iErrorCode: 2 };
                server.send_packet(P_LS2FE_REP_CONNECT_FAIL, &resp);
                return Err(e);
            }
        };

    server.set_client_type(ClientType::ShardServer(shard_id));
    let resp = sP_LS2FE_REP_CONNECT_SUCC {
        iShardID: shard_id,
        uiSvrTime: util::get_timestamp_ms(time),
        aLS_UID: state.server_id.to_bytes_le(),
    };

    server.send_packet(P_LS2FE_REP_CONNECT_SUCC, &resp);

    let iv1: i32 = resp.aLS_UID.into_iter().reduce(|a, b| a ^ b).unwrap() as i32;
    let iv2: i32 = shard_id + 1;
    let e_key = crypto::gen_key(resp.uiSvrTime, iv1, iv2);
    server.update_encryption(Some(e_key), None, None);

    log(
        Severity::Info,
        &format!(
            "Connected to shard server #{}: {} ({}) [{} channel(s), {} players per channel]",
            shard_id,
            public_addr,
            server.get_addr(),
            num_channels,
            max_channel_pop
        ),
    );

    Ok(())
}

pub fn shard_live_check(client: &FFClient) -> FFResult<()> {
    let resp = sP_LS2FE_REP_LIVE_CHECK {
        iTempValue: unused!(),
    };
    client.send_packet(P_LS2FE_REP_LIVE_CHECK, &resp);
    Ok(())
}

pub fn update_login_info_succ(
    pkt: Packet,
    shard_key: usize,
    clients: &HashMap<usize, FFClient>,
    state: &LoginServerState,
) -> FFResult<()> {
    let server = clients.get(&shard_key).unwrap();
    let shard_id = server.get_shard_id()?;
    let pkt: &sP_FE2LS_REP_UPDATE_LOGIN_INFO_SUCC = pkt.get(P_FE2LS_REP_UPDATE_LOGIN_INFO_SUCC)?;

    let shard_public_addr = state.get_shard_public_addr(shard_id).unwrap();
    let resp = sP_LS2CL_REP_SHARD_SELECT_SUCC {
        g_FE_ServerIP: util::encode_utf8(shard_public_addr.ip().to_string().as_str())?,
        g_FE_ServerPort: shard_public_addr.port() as i32,
        iEnterSerialKey: pkt.iEnterSerialKey,
    };

    let client = clients
        .values()
        .find(|c| match c.get_client_type() {
            ClientType::GameClient {
                serial_key: key, ..
            } => key == resp.iEnterSerialKey,
            _ => false,
        })
        .unwrap();
    client.send_packet(P_LS2CL_REP_SHARD_SELECT_SUCC, &resp);
    client.disconnect();

    Ok(())
}

pub fn update_login_info_fail(pkt: Packet, clients: &HashMap<usize, FFClient>) -> FFResult<()> {
    let pkt: &sP_FE2LS_REP_UPDATE_LOGIN_INFO_FAIL = pkt.get(P_FE2LS_REP_UPDATE_LOGIN_INFO_FAIL)?;
    let resp = sP_LS2CL_REP_CHAR_SELECT_FAIL {
        iErrorCode: pkt.iErrorCode,
    };

    let serial_key = pkt.iEnterSerialKey;
    let client = clients
        .values()
        .find(|c| match c.get_client_type() {
            ClientType::GameClient {
                serial_key: key, ..
            } => key == serial_key,
            _ => false,
        })
        .unwrap();

    client.send_packet(P_LS2CL_REP_CHAR_SELECT_FAIL, &resp);
    Ok(())
}

pub fn update_pc_statuses(
    pkt: Packet,
    client: &FFClient,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let mut reader = PacketReader::new(&pkt);
    let pkt: &sP_FE2LS_UPDATE_PC_STATUSES = reader.get_packet(P_FE2LS_UPDATE_PC_STATUSES)?;
    let count = pkt.iCnt;
    let shard_id = client.get_shard_id().expect("Packet filter failed");

    state.clear_shard_players(shard_id);
    for _ in 0..count {
        let data: &sPlayerMetadata = reader.get_struct()?;
        let player_uid = data.iPC_UID;
        let player_data = PlayerMetadata {
            first_name: util::parse_utf16(&data.szFirstName)?,
            last_name: util::parse_utf16(&data.szLastName)?,
            x_coord: data.iX,
            y_coord: data.iY,
            z_coord: data.iZ,
            channel: data.iChannelNum as u8,
        };
        state.set_player_shard(player_uid, player_data, shard_id);
    }

    Ok(())
}

pub fn update_monitor(pkt: Packet) -> FFResult<()> {
    let pkt: &sP_FE2LS_UPDATE_MONITOR = pkt.get(P_FE2LS_UPDATE_MONITOR)?;
    let update = monitor_update_from_packet(pkt)?;
    for event in update.get_events() {
        monitor_queue(event);
    }

    Ok(())
}

pub fn motd(pkt: Packet, client: &FFClient) -> FFResult<()> {
    let pkt: &sP_FE2LS_REQ_MOTD = pkt.get(P_FE2LS_REQ_MOTD)?;

    // load the MOTD from the MOTD file
    let motd_path = config_get().login.motd_path.get();
    let motd = match util::get_text_file_contents(&motd_path) {
        Ok(motd) => motd.trim().to_string(),
        Err(e) => {
            log(
                Severity::Warning,
                &format!("Couldn't load MOTD, using default ({})", e.get_msg()),
            );
            "Welcome to RustyFusion!".to_string()
        }
    };

    let resp = sP_LS2FE_REP_MOTD {
        iPC_ID: pkt.iPC_ID,
        szMessage: util::encode_utf16(&motd)?,
    };

    client.send_packet(P_LS2FE_REP_MOTD, &resp);
    Ok(())
}

pub fn motd_register(pkt: Packet) -> FFResult<()> {
    let pkt: &sP_FE2LS_MOTD_REGISTER = pkt.get(P_FE2LS_MOTD_REGISTER)?;
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

pub fn announce_msg(pkt: Packet, clients: &HashMap<usize, FFClient>) -> FFResult<()> {
    let pkt: &sP_FE2LS_ANNOUNCE_MSG = pkt.get(P_FE2LS_ANNOUNCE_MSG)?;
    clients.iter().for_each(|(_, client)| {
        if let ClientType::ShardServer(_) = client.get_client_type() {
            client.send_packet(P_LS2FE_ANNOUNCE_MSG, pkt);
        }
    });
    Ok(())
}

pub fn pc_location(
    pkt: Packet,
    shard_key: usize,
    clients: &HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let server = clients.get(&shard_key).unwrap();
    let pkt: &sP_FE2LS_REQ_PC_LOCATION = pkt.get(P_FE2LS_REQ_PC_LOCATION)?;
    let req_shard_id = server.get_shard_id()?;
    let request_key = (req_shard_id, pkt.iPC_ID);
    if state.player_search_reqeusts.contains_key(&request_key) {
        let resp = sP_LS2FE_REP_PC_LOCATION_FAIL {
            iPC_ID: pkt.iPC_ID,
            sReq: pkt.sReq,
            iErrorCode: PlayerSearchReqErr::SearchInProgress as i32,
        };
        server.send_packet(P_LS2FE_REP_PC_LOCATION_FAIL, &resp);
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
        .get_connected_shard_ids()
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

    clients.iter().for_each(|(_, client)| {
        if let ClientType::ShardServer(shard_id) = client.get_client_type() {
            if shard_id == req_shard_id {
                return;
            }
            client.send_packet(P_LS2FE_REQ_PC_LOCATION, &pkt);
        }
    });
    Ok(())
}

pub fn pc_location_succ(
    pkt: Packet,
    clients: &HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let pkt: &sP_FE2LS_REP_PC_LOCATION_SUCC = pkt.get(P_FE2LS_REP_PC_LOCATION_SUCC)?;
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
        .values()
        .find(|c| match c.get_client_type() {
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
    client.send_packet(P_LS2FE_REP_PC_LOCATION_SUCC, &resp);
    Ok(())
}

pub fn pc_location_fail(
    pkt: Packet,
    shard_key: usize,
    clients: &HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let server = clients.get(&shard_key).unwrap();
    let pkt: &sP_FE2LS_REP_PC_LOCATION_FAIL = pkt.get(P_FE2LS_REP_PC_LOCATION_FAIL)?;
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
            .values()
            .find(|c| match c.get_client_type() {
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
        shard.send_packet(P_LS2FE_REP_PC_LOCATION_FAIL, &resp);
    }
    Ok(())
}

pub fn get_buddy_state(
    pkt: Packet,
    shard_key: usize,
    clients: &HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let server = clients.get(&shard_key).unwrap();
    let pkt: &sP_FE2LS_REQ_GET_BUDDY_STATE = pkt.get(P_FE2LS_REQ_GET_BUDDY_STATE)?;

    let mut resp = sP_LS2FE_REP_GET_BUDDY_STATE {
        iPC_UID: pkt.iPC_UID,
        aBuddyUID: pkt.aBuddyUID,
        aBuddyState: [0; 50],
    };

    let uids = pkt.aBuddyUID;
    for (i, &buddy_uid) in uids.iter().enumerate() {
        if buddy_uid == 0 {
            continue;
        }
        if state.get_player_shard(buddy_uid).is_some() {
            resp.aBuddyState[i] = 1;
        }
    }

    server.send_packet(P_LS2FE_REP_GET_BUDDY_STATE, &resp);
    Ok(())
}

pub fn handle_disconnecting(
    shard_key: usize,
    clients: &HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let server = clients.get(&shard_key).unwrap();
    let shard_id = server.get_shard_id()?;

    // this packet unregisters the shard early
    // to mitigate race conditions with i.e. dupe player logins
    state.unregister_shard(shard_id);
    Ok(())
}

pub fn buddy_freechat(
    pkt: Packet,
    clients: &HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let pkt: &sP_FE2LS_REQ_SEND_BUDDY_FREECHAT = pkt.get(P_FE2LS_REQ_SEND_BUDDY_FREECHAT)?;
    let req = sP_LS2FE_REQ_SEND_BUDDY_FREECHAT {
        iFromPCUID: pkt.iFromPCUID,
        iToPCUID: pkt.iToPCUID,
        szFreeChat: pkt.szFreeChat,
        iEmoteCode: pkt.iEmoteCode,
    };

    let to_shard_id = match state.get_player_shard(pkt.iToPCUID) {
        Some(shard_id) => shard_id,
        None => {
            return Ok(());
        }
    };

    let client = clients
        .values()
        .find(|c| match c.get_client_type() {
            ClientType::ShardServer(shard_id) => shard_id == to_shard_id,
            _ => false,
        })
        .ok_or(FFError::build(
            Severity::Warning,
            format!(
                "Shard {}, which should host buddy chat recipient, not found",
                to_shard_id
            ),
        ))?;
    client.send_packet(P_LS2FE_REQ_SEND_BUDDY_FREECHAT, &req);

    Ok(())
}

pub fn buddy_freechat_succ(
    pkt: Packet,
    clients: &HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let pkt: &sP_FE2LS_REP_SEND_BUDDY_FREECHAT_SUCC =
        pkt.get(P_FE2LS_REP_SEND_BUDDY_FREECHAT_SUCC)?;

    let succ_pkt = sP_LS2FE_REP_SEND_BUDDY_FREECHAT_SUCC {
        iFromPCUID: pkt.iFromPCUID,
        iToPCUID: pkt.iToPCUID,
        szFreeChat: pkt.szFreeChat,
        iEmoteCode: pkt.iEmoteCode,
    };

    if let Some(from_shard_id) = state.get_player_shard(pkt.iFromPCUID) {
        if let Some(from_shard) = clients
            .values()
            .find(|c| c.get_shard_id().is_ok_and(|id| id == from_shard_id))
        {
            from_shard.send_packet(P_LS2FE_REP_SEND_BUDDY_FREECHAT_SUCC, &succ_pkt);
        }
    }

    Ok(())
}

pub fn buddy_menuchat(
    pkt: Packet,
    clients: &HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let pkt: &sP_FE2LS_REQ_SEND_BUDDY_MENUCHAT = pkt.get(P_FE2LS_REQ_SEND_BUDDY_MENUCHAT)?;
    let req = sP_LS2FE_REQ_SEND_BUDDY_MENUCHAT {
        iFromPCUID: pkt.iFromPCUID,
        iToPCUID: pkt.iToPCUID,
        szFreeChat: pkt.szFreeChat,
        iEmoteCode: pkt.iEmoteCode,
    };

    let to_shard_id = match state.get_player_shard(pkt.iToPCUID) {
        Some(shard_id) => shard_id,
        None => {
            return Ok(());
        }
    };

    let client = clients
        .values()
        .find(|c| match c.get_client_type() {
            ClientType::ShardServer(shard_id) => shard_id == to_shard_id,
            _ => false,
        })
        .ok_or(FFError::build(
            Severity::Warning,
            format!(
                "Shard {}, which should host buddy chat recipient, not found",
                to_shard_id
            ),
        ))?;
    client.send_packet(P_LS2FE_REQ_SEND_BUDDY_MENUCHAT, &req);

    Ok(())
}

pub fn buddy_menuchat_succ(
    pkt: Packet,
    clients: &HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let pkt: &sP_FE2LS_REP_SEND_BUDDY_MENUCHAT_SUCC =
        pkt.get(P_FE2LS_REP_SEND_BUDDY_MENUCHAT_SUCC)?;

    let succ_pkt = sP_LS2FE_REP_SEND_BUDDY_MENUCHAT_SUCC {
        iFromPCUID: pkt.iFromPCUID,
        iToPCUID: pkt.iToPCUID,
        szFreeChat: pkt.szFreeChat,
        iEmoteCode: pkt.iEmoteCode,
    };

    if let Some(from_shard_id) = state.get_player_shard(pkt.iFromPCUID) {
        if let Some(from_shard) = clients
            .values()
            .find(|c| c.get_shard_id().is_ok_and(|id| id == from_shard_id))
        {
            from_shard.send_packet(P_LS2FE_REP_SEND_BUDDY_MENUCHAT_SUCC, &succ_pkt);
        }
    }

    Ok(())
}

pub fn buddy_warp(
    pkt: Packet,
    shard_key: usize,
    clients: &HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let pkt: &sP_FE2LS_REQ_BUDDY_WARP = pkt.get(P_FE2LS_REQ_BUDDY_WARP)?;

    let fail_pkt = sP_LS2FE_REP_BUDDY_WARP_FAIL {
        iBuddyPCUID: pkt.iBuddyPCUID,
        iErrorCode: BuddyWarpErr::CantWarpToLocation as i32,
        iFromPCUID: pkt.iFromPCUID,
    };

    let buddy_shard_id = match state.get_player_shard(pkt.iBuddyPCUID) {
        Some(shard_id) => shard_id,
        None => {
            let player_shard = clients.get(&shard_key).unwrap();
            player_shard.send_packet(P_LS2FE_REP_BUDDY_WARP_FAIL, &fail_pkt);
            return Ok(());
        }
    };

    let buddy_shard: &FFClient = match clients.values().find(|c| match c.get_client_type() {
        ClientType::ShardServer(shard_id) => shard_id == buddy_shard_id,
        _ => false,
    }) {
        Some(shard) => shard,
        None => {
            let player_shard = clients.get(&shard_key).unwrap();
            player_shard.send_packet(P_LS2FE_REP_BUDDY_WARP_FAIL, &fail_pkt);
            return Ok(());
        }
    };

    let req_pkt = sP_LS2FE_REQ_BUDDY_WARP {
        iPCPayzoneFlag: pkt.iPCPayzoneFlag,
        iBuddyPCUID: pkt.iBuddyPCUID,
        iFromPCUID: pkt.iFromPCUID,
    };

    buddy_shard.send_packet(P_LS2FE_REQ_BUDDY_WARP, &req_pkt);
    Ok(())
}

pub fn buddy_warp_succ(
    pkt: Packet,
    clients: &HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let pkt: &sP_FE2LS_REP_BUDDY_WARP_SUCC = pkt.get(P_FE2LS_REP_BUDDY_WARP_SUCC)?;

    let buddy_pcuid = pkt.iBuddyPCUID;

    let buddy_shard_id = state.get_player_shard(buddy_pcuid).ok_or(FFError::build(
        Severity::Warning,
        format!("Couldn't find shard for buddy PC UID {}", buddy_pcuid),
    ))?;

    let resp_pkt = sP_LS2FE_REP_BUDDY_WARP_SUCC {
        iBuddyPCUID: pkt.iBuddyPCUID,
        iFromPCUID: pkt.iFromPCUID,
        iChannelNum: pkt.iChannelNum,
        iInstanceNum: pkt.iInstanceNum,
        iMapNum: pkt.iMapNum,
        iShardNum: buddy_shard_id as i8,
        iX: pkt.iX,
        iY: pkt.iY,
        iZ: pkt.iZ,
        iBuddyWarpTime: pkt.iBuddyWarpTime,
    };

    let pcuid = pkt.iFromPCUID;

    state.set_pending_channel_request(pcuid, pkt.iChannelNum);
    state.buddy_warp_times.insert(pcuid, pkt.iBuddyWarpTime);

    log(
        Severity::Info,
        &format!(
            "Set pending channel request with values PC UID {} -> Channel {}",
            pcuid, pkt.iChannelNum
        ),
    );

    if let Some(from_shard_id) = state.get_player_shard(pkt.iFromPCUID) {
        if let Some(from_shard) = clients
            .values()
            .find(|c| c.get_shard_id().is_ok_and(|id| id == from_shard_id))
        {
            from_shard.send_packet(P_LS2FE_REP_BUDDY_WARP_SUCC, &resp_pkt);
        }
    }

    Ok(())
}

pub fn buddy_warp_fail(
    pkt: Packet,
    clients: &HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let pkt: &sP_FE2LS_REP_BUDDY_WARP_FAIL = pkt.get(P_FE2LS_REP_BUDDY_WARP_FAIL)?;

    let resp_pkt = sP_LS2FE_REP_BUDDY_WARP_FAIL {
        iBuddyPCUID: pkt.iBuddyPCUID,
        iErrorCode: pkt.iErrorCode,
        iFromPCUID: pkt.iFromPCUID,
    };

    if let Some(from_shard_id) = state.get_player_shard(pkt.iFromPCUID) {
        if let Some(from_shard) = clients
            .values()
            .find(|c| c.get_shard_id().is_ok_and(|id| id == from_shard_id))
        {
            from_shard.send_packet(P_LS2FE_REP_BUDDY_WARP_FAIL, &resp_pkt);
        }
    }

    Ok(())
}
