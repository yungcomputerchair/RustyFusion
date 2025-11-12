use std::{collections::HashMap, net::SocketAddr};

use uuid::Uuid;

use rusty_fusion::{
    chunk::InstanceID,
    config::{self, config_get},
    database::db_run_sync,
    defines::*,
    entity::{Entity, EntityID, PlayerSearchQuery},
    enums::*,
    error::{
        codes::{BuddyWarpErr, PlayerSearchReqErr},
        *,
    },
    net::{
        crypto,
        packet::{PacketID::*, *},
        ClientMap, FFClient, LoginData,
    },
    state::ShardServerState,
    unused, util, Position,
};

pub fn login_connect_req(server: &mut FFClient) {
    let pkt = sP_FE2LS_REQ_AUTH_CHALLENGE {
        iTempValue: unused!(),
    };
    log_if_failed(server.send_packet(P_FE2LS_REQ_AUTH_CHALLENGE, &pkt));
}

pub fn login_connect_challenge(server: &mut FFClient, state: &ShardServerState) -> FFResult<()> {
    let pkt: &sP_LS2FE_REP_AUTH_CHALLENGE = server.get_packet(P_LS2FE_REP_AUTH_CHALLENGE)?;
    let key = config_get().general.server_key.get().clone();
    let mut challenge = pkt.aChallenge;
    crypto::decrypt_payload(&mut challenge[..], key.as_bytes());
    let pkt = sP_FE2LS_REQ_CONNECT {
        aChallengeSolved: challenge,
        iShardID: state.shard_id,
        iNumChannels: config_get().shard.num_channels.get() as i8,
        iMaxChannelPop: config_get().shard.max_channel_pop.get() as i32,
    };
    server.send_packet(P_FE2LS_REQ_CONNECT, &pkt)
}

pub fn login_connect_succ(server: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: &sP_LS2FE_REP_CONNECT_SUCC = server.get_packet(P_LS2FE_REP_CONNECT_SUCC)?;
    let login_server_id = Uuid::from_bytes_le(pkt.aLS_UID);
    let conn_time: u64 = pkt.uiSvrTime;

    let iv1: i32 = pkt.aLS_UID.into_iter().reduce(|a, b| a ^ b).unwrap() as i32;
    let iv2: i32 = state.shard_id + 1;
    server.e_key = crypto::gen_key(conn_time, iv1, iv2);
    state.login_server_conn_id = Some(login_server_id);

    log(
        Severity::Info,
        &format!(
            "Connected to login server {} ({})",
            login_server_id,
            server.get_addr(),
        ),
    );
    Ok(())
}

pub fn login_connect_fail(server: &mut FFClient) -> FFResult<()> {
    let pkt: &sP_LS2FE_REP_CONNECT_FAIL = server.get_packet(P_LS2FE_REP_CONNECT_FAIL)?;
    Err(FFError::build(
        Severity::Warning,
        format!("Login server refused to connect (error {})", {
            pkt.iErrorCode
        }),
    ))
}

pub fn login_update_info(server: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let public_addr: SocketAddr = config_get()
        .shard
        .external_addr
        .get()
        .parse()
        .expect("Bad public address");
    let mut ip_buf: [u8; 16] = [0; 16];
    let ip_str: &str = &public_addr.ip().to_string();
    let ip_bytes: &[u8] = ip_str.as_bytes();
    ip_buf[..ip_bytes.len()].copy_from_slice(ip_bytes);

    let pkt: &sP_LS2FE_REQ_UPDATE_LOGIN_INFO = server.get_packet(P_LS2FE_REQ_UPDATE_LOGIN_INFO)?;
    let resp = sP_FE2LS_REP_UPDATE_LOGIN_INFO_SUCC {
        iEnterSerialKey: pkt.iEnterSerialKey,
        g_FE_ServerIP: ip_buf,
        g_FE_ServerPort: public_addr.port() as i32,
    };

    let serial_key = resp.iEnterSerialKey;
    let ld = &mut state.login_data;
    if ld.contains_key(&serial_key) {
        // this serial key was already registered...
        // extremely unlikely?
        let resp = sP_FE2LS_REP_UPDATE_LOGIN_INFO_FAIL {
            iEnterSerialKey: serial_key,
            iErrorCode: 1,
        };
        server.send_packet(P_FE2LS_REP_UPDATE_LOGIN_INFO_FAIL, &resp)?;
        return Ok(());
    }
    ld.insert(
        serial_key,
        LoginData {
            iAccountID: pkt.iAccountID,
            iPC_UID: pkt.iPC_UID,
            uiFEKey: pkt.uiFEKey,
            uiSvrTime: pkt.uiSvrTime,
            iChannelRequestNum: pkt.iChannelRequestNum,
        },
    );

    server.send_packet(P_FE2LS_REP_UPDATE_LOGIN_INFO_SUCC, &resp)
}

pub fn login_live_check(client: &mut FFClient) -> FFResult<()> {
    let resp = sP_FE2LS_REP_LIVE_CHECK {
        iTempValue: unused!(),
    };
    client.send_packet(P_FE2LS_REP_LIVE_CHECK, &resp)?;
    Ok(())
}

pub fn login_motd(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: &sP_LS2FE_REP_MOTD = clients.get_self().get_packet(P_LS2FE_REP_MOTD)?;
    let player = state.get_player(pkt.iPC_ID)?;
    let pkt = sP_FE2CL_PC_MOTD_LOGIN {
        iType: unused!(),
        szSystemMsg: pkt.szMessage,
    };
    if let Some(client) = player.get_client(clients) {
        client.send_packet(P_FE2CL_PC_MOTD_LOGIN, &pkt)
    } else {
        Ok(())
    }
}

pub fn login_announce_msg(clients: &mut ClientMap) -> FFResult<()> {
    let pkt: sP_LS2FE_ANNOUNCE_MSG = *clients.get_self().get_packet(P_LS2FE_ANNOUNCE_MSG)?;
    clients.get_all_gameclient().for_each(|c| {
        log_if_failed(c.send_packet(P_FE2CL_ANNOUNCE_MSG, &pkt));
    });
    Ok(())
}

pub fn login_pc_location(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_LS2FE_REQ_PC_LOCATION = *clients.get_self().get_packet(P_LS2FE_REQ_PC_LOCATION)?;
    let req = pkt.sReq;
    let search_mode: TargetSearchBy = req.eTargetSearchBy.try_into()?;
    let search_query = match search_mode {
        TargetSearchBy::PlayerID => PlayerSearchQuery::ByID(req.iTargetPC_ID),
        TargetSearchBy::PlayerName => PlayerSearchQuery::ByName(
            util::parse_utf16(&req.szTargetPC_FirstName)?,
            util::parse_utf16(&req.szTargetPC_LastName)?,
        ),
        TargetSearchBy::PlayerUID => PlayerSearchQuery::ByUID(req.iTargetPC_UID),
    };
    if let Some(pc_id) = search_query.execute(state) {
        let player = state.get_player(pc_id)?;
        let pos = player.get_position();
        let resp = sP_FE2CL_GM_REP_PC_LOCATION {
            iTargetPC_UID: player.get_uid(),
            iTargetPC_ID: pc_id,
            iShardID: state.shard_id,
            iMapType: if player.instance_id.instance_num.is_some() {
                1 // instance
            } else {
                0 // non-instance
            },
            iMapID: player.instance_id.instance_num.unwrap_or(0) as i32,
            iMapNum: player.instance_id.map_num as i32,
            iX: pos.x,
            iY: pos.y,
            iZ: pos.z,
            szTargetPC_FirstName: util::encode_utf16(&player.first_name),
            szTargetPC_LastName: util::encode_utf16(&player.last_name),
        };
        if let Some(login_server) = clients.get_login_server() {
            let resp = sP_FE2LS_REP_PC_LOCATION_SUCC {
                iReqShard_ID: pkt.iReqShard_ID,
                iPC_ID: pkt.iPC_ID,
                sResp: resp,
            };
            log_if_failed(login_server.send_packet(P_FE2LS_REP_PC_LOCATION_SUCC, &resp));
        }
    } else if let Some(login_server) = clients.get_login_server() {
        let resp = sP_FE2LS_REP_PC_LOCATION_FAIL {
            iReqShard_ID: pkt.iReqShard_ID,
            iPC_ID: pkt.iPC_ID,
            sReq: pkt.sReq,
            iErrorCode: PlayerSearchReqErr::NotFound as i32,
        };
        log_if_failed(login_server.send_packet(P_FE2LS_REP_PC_LOCATION_FAIL, &resp));
    }
    Ok(())
}

pub fn login_pc_location_succ(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let pkt: &sP_LS2FE_REP_PC_LOCATION_SUCC = clients
        .get_self()
        .get_packet(P_LS2FE_REP_PC_LOCATION_SUCC)?;
    let resp = pkt.sResp;
    let player = state.get_player(pkt.iPC_ID)?;
    let client = player.get_client(clients).unwrap();
    log_if_failed(client.send_packet(P_FE2CL_GM_REP_PC_LOCATION, &resp));
    Ok(())
}

pub fn login_pc_location_fail(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let pkt: &sP_LS2FE_REP_PC_LOCATION_FAIL = clients
        .get_self()
        .get_packet(P_LS2FE_REP_PC_LOCATION_FAIL)?;
    let err_code: PlayerSearchReqErr = pkt.iErrorCode.try_into()?;
    let err_msg = match err_code {
        PlayerSearchReqErr::NotFound => {
            let req = pkt.sReq;
            let search_mode: TargetSearchBy = req.eTargetSearchBy.try_into()?;
            let search_query = match search_mode {
                TargetSearchBy::PlayerID => PlayerSearchQuery::ByID(req.iTargetPC_ID),
                TargetSearchBy::PlayerName => PlayerSearchQuery::ByName(
                    util::parse_utf16(&req.szTargetPC_FirstName)?,
                    util::parse_utf16(&req.szTargetPC_LastName)?,
                ),
                TargetSearchBy::PlayerUID => PlayerSearchQuery::ByUID(req.iTargetPC_UID),
            };
            format!("Player not found: {:?}", search_query)
        }
        PlayerSearchReqErr::SearchInProgress => {
            "A search is already in progress, please try again".to_string()
        }
    };

    // let the GM know the search failed
    let player = state.get_player(pkt.iPC_ID)?;
    let client = player.get_client(clients).unwrap();
    let pkt = sP_FE2CL_ANNOUNCE_MSG {
        iAnnounceType: unused!(),
        iDuringTime: MSG_BOX_DURATION_DEFAULT,
        szAnnounceMsg: util::encode_utf16(&err_msg),
    };
    log_if_failed(client.send_packet(P_FE2CL_ANNOUNCE_MSG, &pkt));
    Ok(())
}

pub fn login_pc_exit_duplicate(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let pkt: &sP_LS2FE_REQ_PC_EXIT_DUPLICATE = clients
        .get_self()
        .get_packet(P_LS2FE_REQ_PC_EXIT_DUPLICATE)?;
    let pc_uid = pkt.iPC_UID;
    let pc_id = state
        .get_player_by_uid(pc_uid)
        .map(|p| p.get_player_id())
        .ok_or(FFError::build(
            Severity::Warning,
            format!("Couldn't find player with UID {}", pc_uid),
        ))?;
    let player = state.get_player_mut(pc_id).unwrap();
    let client = player.get_client(clients).unwrap();
    let pkt = sP_FE2CL_REP_PC_EXIT_DUPLICATE {
        iErrorCode: unused!(),
    };
    log_if_failed(client.send_packet(P_FE2CL_REP_PC_EXIT_DUPLICATE, &pkt));
    client.disconnect();
    Ok(())
}

pub fn login_get_buddy_state(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let pkt: sP_LS2FE_REP_GET_BUDDY_STATE =
        *clients.get_self().get_packet(P_LS2FE_REP_GET_BUDDY_STATE)?;
    let pc_uid = pkt.iPC_UID;

    // buddy list may have changed during flight, so we can't just index into the query
    let buddy_uids = pkt.aBuddyUID;
    let buddy_states = pkt.aBuddyState;
    let query_results: HashMap<i64, u8> = buddy_uids
        .iter()
        .zip(buddy_states.iter())
        .filter_map(
            |(id, state)| {
                if *id == 0 {
                    None
                } else {
                    Some((*id, *state))
                }
            },
        )
        .collect();

    let pc_id = state
        .get_player_by_uid(pc_uid)
        .map(|p| p.get_player_id())
        .ok_or(FFError::build(
            Severity::Warning,
            format!("Couldn't find player with UID {}", pc_uid),
        ))?;
    let player = state.get_player(pc_id).unwrap();
    let buddy_info = player.get_all_buddy_info();

    let mut resp = sP_FE2CL_REP_GET_BUDDY_STATE_SUCC {
        aBuddyID: [0; SIZEOF_BUDDYLIST_SLOT as usize],
        aBuddyState: [0; SIZEOF_BUDDYLIST_SLOT as usize],
    };
    for (i, buddy_uid) in buddy_info.iter().map(|info| info.pc_uid).enumerate() {
        let online = query_results.get(&buddy_uid).is_some_and(|v| *v != 0);
        resp.aBuddyState[i] = if online { 1 } else { 0 };
        if online {
            // lookup shard-local ID
            let buddy_id = state
                .get_player_by_uid(buddy_uid)
                .map(|p| p.get_player_id())
                .unwrap_or(0);
            resp.aBuddyID[i] = buddy_id;
        }
    }

    let client = player.get_client(clients).unwrap();
    log_if_failed(client.send_packet(P_FE2CL_REP_GET_BUDDY_STATE_SUCC, &resp));
    Ok(())
}

pub fn login_buddy_freechat(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_LS2FE_REQ_SEND_BUDDY_FREECHAT =
        *client.get_packet(P_LS2FE_REQ_SEND_BUDDY_FREECHAT)?;

    if let Some(buddy) = state.get_player_by_uid(pkt.iToPCUID) {
        if let Some(buddy_client) = buddy.get_client(clients) {
            let response_pkt = sP_FE2CL_REP_SEND_BUDDY_FREECHAT_MESSAGE_SUCC {
                iFromPCUID: pkt.iFromPCUID,
                iToPCUID: pkt.iToPCUID,
                szFreeChat: pkt.szFreeChat,
                iEmoteCode: pkt.iEmoteCode,
            };
            buddy_client
                .send_packet(P_FE2CL_REP_SEND_BUDDY_FREECHAT_MESSAGE_SUCC, &response_pkt)?;

            if let Some(login_server) = clients.get_login_server() {
                let succ_pkt = sP_FE2LS_REP_SEND_BUDDY_FREECHAT_SUCC {
                    iFromPCUID: pkt.iFromPCUID,
                    iToPCUID: pkt.iToPCUID,
                    szFreeChat: pkt.szFreeChat,
                    iEmoteCode: pkt.iEmoteCode,
                };
                login_server.send_packet(P_FE2LS_REP_SEND_BUDDY_FREECHAT_SUCC, &succ_pkt)?;
            } else {
                return Err(FFError::build(
                    Severity::Warning,
                    "No login server found to forward buddy freechat message".to_string(),
                ));
            }
        }
    }

    Ok(())
}

pub fn buddy_freechat_succ(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let login_server = clients.get_login_server().unwrap();
    let pkt: sP_LS2FE_REP_SEND_BUDDY_FREECHAT_SUCC =
        *login_server.get_packet(P_LS2FE_REP_SEND_BUDDY_FREECHAT_SUCC)?;

    let response_pkt = sP_FE2CL_REP_SEND_BUDDY_FREECHAT_MESSAGE_SUCC {
        iFromPCUID: pkt.iFromPCUID,
        iToPCUID: pkt.iToPCUID,
        szFreeChat: pkt.szFreeChat,
        iEmoteCode: pkt.iEmoteCode,
    };

    if let Some(sender) = state.get_player_by_uid(pkt.iFromPCUID) {
        if let Some(sender_client) = sender.get_client(clients) {
            sender_client
                .send_packet(P_FE2CL_REP_SEND_BUDDY_FREECHAT_MESSAGE_SUCC, &response_pkt)?;
        }
    }

    Ok(())
}

pub fn login_buddy_menuchat(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_LS2FE_REQ_SEND_BUDDY_MENUCHAT =
        *client.get_packet(P_LS2FE_REQ_SEND_BUDDY_MENUCHAT)?;

    if let Some(buddy) = state.get_player_by_uid(pkt.iToPCUID) {
        if let Some(buddy_client) = buddy.get_client(clients) {
            let response_pkt = sP_FE2CL_REP_SEND_BUDDY_MENUCHAT_MESSAGE_SUCC {
                iFromPCUID: pkt.iFromPCUID,
                iToPCUID: pkt.iToPCUID,
                szFreeChat: pkt.szFreeChat,
                iEmoteCode: pkt.iEmoteCode,
            };
            buddy_client
                .send_packet(P_FE2CL_REP_SEND_BUDDY_MENUCHAT_MESSAGE_SUCC, &response_pkt)?;

            if let Some(login_server) = clients.get_login_server() {
                let succ_pkt = sP_FE2LS_REP_SEND_BUDDY_MENUCHAT_SUCC {
                    iFromPCUID: pkt.iFromPCUID,
                    iToPCUID: pkt.iToPCUID,
                    szFreeChat: pkt.szFreeChat,
                    iEmoteCode: pkt.iEmoteCode,
                };
                login_server.send_packet(P_FE2LS_REP_SEND_BUDDY_MENUCHAT_SUCC, &succ_pkt)?;
            } else {
                return Err(FFError::build(
                    Severity::Warning,
                    "No login server found to forward buddy menuchat message".to_string(),
                ));
            }
        }
    }

    Ok(())
}

pub fn buddy_menuchat_succ(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let login_server = clients.get_login_server().unwrap();
    let pkt: sP_LS2FE_REP_SEND_BUDDY_MENUCHAT_SUCC =
        *login_server.get_packet(P_LS2FE_REP_SEND_BUDDY_MENUCHAT_SUCC)?;

    let response_pkt = sP_FE2CL_REP_SEND_BUDDY_MENUCHAT_MESSAGE_SUCC {
        iFromPCUID: pkt.iFromPCUID,
        iToPCUID: pkt.iToPCUID,
        szFreeChat: pkt.szFreeChat,
        iEmoteCode: pkt.iEmoteCode,
    };

    if let Some(sender) = state.get_player_by_uid(pkt.iFromPCUID) {
        if let Some(sender_client) = sender.get_client(clients) {
            sender_client
                .send_packet(P_FE2CL_REP_SEND_BUDDY_MENUCHAT_MESSAGE_SUCC, &response_pkt)?;
        }
    }

    Ok(())
}

pub fn login_buddy_warp(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_LS2FE_REQ_BUDDY_WARP = *client.get_packet(P_LS2FE_REQ_BUDDY_WARP)?;

    let fail_pkt = sP_FE2LS_REP_BUDDY_WARP_FAIL {
        iBuddyPCUID: pkt.iBuddyPCUID,
        iFromPCUID: pkt.iFromPCUID,
        iErrorCode: BuddyWarpErr::CantWarpToLocation as i32,
    };

    let mut invalid_warp = |msg: String| {
        log(Severity::Info, &msg);
        client.send_packet(P_FE2LS_REP_BUDDY_WARP_FAIL, &fail_pkt)
    };

    let buddy_uid = pkt.iBuddyPCUID;
    let buddy = match state.get_player_by_uid(buddy_uid) {
        Some(buddy) => buddy,
        None => {
            return client.send_packet(P_FE2LS_REP_BUDDY_WARP_FAIL, &fail_pkt);
        }
    };

    let buddy_is_on_skyway = buddy.get_skyway_ride().is_some();
    let buddy_payzone_flag = buddy.get_payzone_flag();
    let buddy_instance_id = buddy.get_instance_id();
    let buddy_position = buddy.get_position();

    if buddy_is_on_skyway {
        return invalid_warp(format!(
            "Player {} is currently on a skyway ride",
            buddy_uid
        ));
    }

    if pkt.iPCPayzoneFlag != buddy_payzone_flag as i8 {
        return invalid_warp(format!(
            "Buddy {} is in a different payzone state",
            buddy_uid,
        ));
    }

    if buddy_instance_id.map_num != ID_OVERWORLD {
        return invalid_warp(format!(
            "Buddy {} is not in the overworld instance",
            buddy_uid,
        ));
    }

    let max_channel_pop = config::config_get().shard.max_channel_pop.get();
    let current_channel_pop = state
        .entity_map
        .get_channel_population(buddy.instance_id.channel_num);

    if current_channel_pop >= max_channel_pop {
        return invalid_warp(format!(
            "Buddy {}'s channel is at max population",
            buddy_uid,
        ));
    }

    let resp_pkt = sP_FE2LS_REP_BUDDY_WARP_SUCC {
        iBuddyPCUID: pkt.iBuddyPCUID,
        iFromPCUID: pkt.iFromPCUID,
        iChannelNum: buddy.instance_id.channel_num,
        iInstanceNum: buddy.instance_id.instance_num.unwrap_or(0),
        iMapNum: buddy.instance_id.map_num,
        iX: buddy_position.x,
        iY: buddy_position.y,
        iZ: buddy_position.z,
    };

    client.send_packet(P_FE2LS_REP_BUDDY_WARP_SUCC, &resp_pkt)
}

pub fn login_buddy_warp_succ(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let login_server = clients.get_login_server().ok_or_else(|| {
        FFError::build(
            Severity::Warning,
            "No login server connected for buddy warp".to_string(),
        )
    })?;

    let pkt: sP_LS2FE_REP_BUDDY_WARP_SUCC =
        *login_server.get_packet(P_LS2FE_REP_BUDDY_WARP_SUCC)?;

    let player_pcuid = pkt.iFromPCUID;

    catch_fail(
        (|| {
            let player = state.get_player_by_uid(player_pcuid).ok_or_else(|| {
                FFError::build(
                    Severity::Warning,
                    format!("Couldn't find player with UID {}", player_pcuid),
                )
            })?;
            let pc_id = player.get_player_id();
            let player = state.get_player_mut(pc_id).unwrap();
            player.set_instance_id(InstanceID {
                map_num: pkt.iMapNum,
                channel_num: pkt.iChannelNum,
                instance_num: None,
            });
            player.set_position(Position {
                x: pkt.iX,
                y: pkt.iY,
                z: pkt.iZ,
            });

            let player_saved = player.clone();

            log_if_failed(db_run_sync(move |db| db.save_player(&player_saved)));

            state
                .entity_map
                .update(EntityID::Player(pc_id), None, Some(clients));

            let other_shard_succ_pkt = sP_FE2CL_REP_PC_BUDDY_WARP_OTHER_SHARD_SUCC {
                iBuddyPCUID: pkt.iBuddyPCUID,
                iChannelNum: 0,
                iShardNum: pkt.iShardNum,
            };

            let player = state.get_player_mut(pc_id).unwrap();

            let player_client = player.get_client(clients).ok_or_else(|| {
                FFError::build(
                    Severity::Warning,
                    format!("Couldn't find client for player UID {}", player_pcuid),
                )
            })?;

            player_client.send_packet(
                P_FE2CL_REP_PC_BUDDY_WARP_OTHER_SHARD_SUCC,
                &other_shard_succ_pkt,
            )
        })(),
        || {
            let response = sP_FE2CL_REP_PC_BUDDY_WARP_FAIL {
                iBuddyPCUID: pkt.iBuddyPCUID,
                iErrorCode: BuddyWarpErr::CantWarpToLocation as i32,
            };

            let player = state.get_player_by_uid(player_pcuid).ok_or_else(|| {
                FFError::build(
                    Severity::Warning,
                    format!("Couldn't find player with UID {}", player_pcuid),
                )
            })?;

            let player_client = player.get_client(clients).ok_or_else(|| {
                FFError::build(
                    Severity::Warning,
                    format!("Couldn't find client for player UID {}", player_pcuid),
                )
            })?;
            player_client.send_packet(P_FE2CL_REP_PC_BUDDY_WARP_FAIL, &response)
        },
    )
}

pub fn login_buddy_warp_fail(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    catch_fail(
        (|| {
            let login_server = clients.get_login_server().ok_or_else(|| {
                FFError::build(
                    Severity::Warning,
                    "No login server connected for buddy warp".to_string(),
                )
            })?;
            let pkt: sP_LS2FE_REP_BUDDY_WARP_FAIL =
                *login_server.get_packet(P_LS2FE_REP_BUDDY_WARP_FAIL)?;

            let player_pcuid = pkt.iFromPCUID;

            let response = sP_FE2CL_REP_PC_BUDDY_WARP_FAIL {
                iBuddyPCUID: pkt.iBuddyPCUID,
                iErrorCode: pkt.iErrorCode,
            };

            let player = state.get_player_by_uid(player_pcuid).ok_or_else(|| {
                FFError::build(
                    Severity::Warning,
                    format!("Couldn't find player with UID {}", player_pcuid),
                )
            })?;

            let player_client = player.get_client(clients).ok_or_else(|| {
                FFError::build(
                    Severity::Warning,
                    format!("Couldn't find client for player UID {}", player_pcuid),
                )
            })?;
            player_client.send_packet(P_FE2CL_REP_PC_BUDDY_WARP_FAIL, &response)
        })(),
        || {
            Err(FFError::build(
                Severity::Warning,
                "Failed to process buddy warp fail".to_string(),
            ))
        },
    )
}
