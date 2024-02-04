use rusty_fusion::{
    defines::MSG_BOX_DURATION_DEFAULT,
    enums::TargetSearchBy,
    error::{codes::PlayerSearchReqErr, log_if_failed, FFError, Severity},
    player::PlayerSearchQuery,
    unused, util,
};
use uuid::Uuid;

use super::*;

use std::net::SocketAddr;

pub fn login_connect_req(server: &mut FFClient) {
    let pkt = sP_FE2LS_REQ_CONNECT {
        iTempValue: unused!(),
    };
    log_if_failed(server.send_packet(P_FE2LS_REQ_CONNECT, &pkt));
}

pub fn login_connect_succ(server: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: &sP_LS2FE_REP_CONNECT_SUCC = server.get_packet(P_LS2FE_REP_CONNECT_SUCC)?;
    let login_server_id = Uuid::from_bytes_le(pkt.aLS_UID);
    let shard_id = pkt.iFE_ID;
    let conn_time: u64 = pkt.uiSvrTime;

    let iv1: i32 = pkt.aLS_UID.into_iter().reduce(|a, b| a ^ b).unwrap() as i32;
    let iv2: i32 = shard_id + 1;
    server.e_key = gen_key(conn_time, iv1, iv2);

    let pkt = sP_FE2LS_UPDATE_CHANNEL_STATUSES {
        aChannelStatus: state.entity_map.get_channel_statuses().map(|s| s as u8),
    };
    server.send_packet(P_FE2LS_UPDATE_CHANNEL_STATUSES, &pkt)?;

    state.login_server_conn_id = Some(login_server_id);
    state.shard_id = Some(shard_id);
    log(
        Severity::Info,
        &format!(
            "Connected to login server {} ({}) as shard #{}",
            login_server_id,
            server.get_addr(),
            shard_id
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
            iShardID: state.shard_id.unwrap(),
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
                iPC_ID: pkt.iPC_ID,
                sResp: resp,
            };
            log_if_failed(login_server.send_packet(P_FE2LS_REP_PC_LOCATION_SUCC, &resp));
        }
    } else if let Some(login_server) = clients.get_login_server() {
        let resp = sP_FE2LS_REP_PC_LOCATION_FAIL {
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
