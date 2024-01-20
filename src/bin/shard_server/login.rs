use rusty_fusion::{
    error::{FFError, Severity},
    unused,
};

use super::*;

use std::net::SocketAddr;

pub fn login_connect_req(server: &mut FFClient) {
    let pkt = sP_FE2LS_REQ_CONNECT {
        iTempValue: unused!(),
    };
    let _ = server.send_packet(P_FE2LS_REQ_CONNECT, &pkt);
}

pub fn login_connect_succ(server: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: &sP_LS2FE_REP_CONNECT_SUCC = server.get_packet(P_LS2FE_REP_CONNECT_SUCC)?;
    let login_server_id: i64 = pkt.iLS_UID;
    let conn_time: u64 = pkt.uiSvrTime;

    let iv1: i32 = (login_server_id + 1) as i32;
    let iv2: i32 = 69;
    server.e_key = gen_key(conn_time, iv1, iv2);

    let pkt = sP_FE2LS_UPDATE_CHANNEL_STATUSES {
        aChannelStatus: state.entity_map.get_channel_statuses().map(|s| s as u8),
    };
    server.send_packet(P_FE2LS_UPDATE_CHANNEL_STATUSES, &pkt)?;

    state.login_server_conn_id = Some(login_server_id);
    log(
        Severity::Info,
        &format!(
            "Connected to login server #{} ({})",
            login_server_id,
            server.get_addr()
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
        let _ = c.send_packet(P_FE2CL_ANNOUNCE_MSG, &pkt);
    });
    Ok(())
}
