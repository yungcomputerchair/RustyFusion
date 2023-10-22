use super::*;

use rusty_fusion::net::{ffclient::ClientType, packet::*};

pub fn connect(server: &mut FFClient, state: &mut LoginServerState) -> Result<()> {
    let conn_id: i64 = state.get_next_shard_id();
    server.set_client_type(ClientType::ShardServer(conn_id));
    let resp = sP_LS2FE_REP_CONNECT_SUCC {
        uiSvrTime: get_time(),
        iConn_UID: conn_id,
    };
    server.send_packet(P_LS2FE_REP_CONNECT_SUCC, &resp)?;

    let iv1: i32 = (conn_id + 1) as i32;
    let iv2: i32 = 69;
    server.set_e_key(gen_key(resp.uiSvrTime, iv1, iv2));

    Ok(())
}

pub fn update_login_info_succ(
    shard_key: usize,
    clients: &mut HashMap<usize, FFClient>,
) -> Result<()> {
    let server: &mut FFClient = clients.get_mut(&shard_key).unwrap();
    let pkt: &sP_FE2LS_REP_UPDATE_LOGIN_INFO_SUCC = server.get_packet();

    let resp = sP_LS2CL_REP_SHARD_SELECT_SUCC {
        g_FE_ServerIP: pkt.g_FE_ServerIP,
        g_FE_ServerPort: pkt.g_FE_ServerPort,
        iEnterSerialKey: pkt.iEnterSerialKey,
    };

    let client: &mut FFClient = clients
        .values_mut()
        .find(|c| match c.get_client_type() {
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
) -> Result<()> {
    let server: &mut FFClient = clients.get_mut(&shard_key).unwrap();
    let pkt: &sP_FE2LS_REP_UPDATE_LOGIN_INFO_FAIL = server.get_packet();
    let resp = sP_LS2CL_REP_CHAR_SELECT_FAIL {
        iErrorCode: pkt.iErrorCode,
    };

    let serial_key: i64 = pkt.iEnterSerialKey;
    let client: &mut FFClient = clients
        .values_mut()
        .find(|c| match c.get_client_type() {
            ClientType::GameClient {
                serial_key: key, ..
            } => key == serial_key,
            _ => false,
        })
        .unwrap();

    client.send_packet(P_LS2CL_REP_CHAR_SELECT_FAIL, &resp)?;

    Ok(())
}
