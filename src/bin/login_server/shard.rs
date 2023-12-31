use std::time::SystemTime;

use super::*;

use rusty_fusion::{
    net::{ffclient::ClientType, packet::*},
    util,
};

pub fn connect(
    server: &mut FFClient,
    state: &mut LoginServerState,
    time: SystemTime,
) -> FFResult<()> {
    let conn_id = state.get_next_shard_id();
    server.client_type = ClientType::ShardServer(conn_id);
    let resp = sP_LS2FE_REP_CONNECT_SUCC {
        uiSvrTime: util::get_timestamp_ms(time),
        iConn_UID: conn_id,
    };
    server.send_packet(P_LS2FE_REP_CONNECT_SUCC, &resp)?;

    let iv1: i32 = (conn_id + 1) as i32;
    let iv2: i32 = 69;
    server.e_key = gen_key(resp.uiSvrTime, iv1, iv2);

    log(
        Severity::Info,
        &format!(
            "Connected to shard server #{} ({})",
            conn_id,
            server.get_addr()
        ),
    );

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
