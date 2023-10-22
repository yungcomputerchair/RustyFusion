use super::*;

use std::net::SocketAddr;

pub fn login_connect_req(server: &mut FFClient) {
    let pkt = sP_FE2LS_REQ_CONNECT { iTempValue: 0 };
    server.send_packet(P_FE2LS_REQ_CONNECT, &pkt).unwrap();
}

pub fn login_connect_succ(server: &mut FFClient, state: &mut ShardServerState) -> Result<()> {
    let pkt: &sP_LS2FE_REP_CONNECT_SUCC = server.get_packet();
    let conn_id: i64 = pkt.iConn_UID;
    let conn_time: u64 = pkt.uiSvrTime;

    let iv1: i32 = (conn_id + 1) as i32;
    let iv2: i32 = 69;
    server.set_e_key(gen_key(conn_time, iv1, iv2));

    state.set_login_server_conn_id(conn_id);
    println!("Connected to login server ({})", server.get_addr());
    Ok(())
}

pub fn login_connect_fail(server: &mut FFClient) -> Result<()> {
    let pkt: &sP_LS2FE_REP_CONNECT_FAIL = server.get_packet();
    println!("Login server refused to connect (error {})", {
        pkt.iErrorCode
    });
    Ok(())
}

pub fn login_update_info(server: &mut FFClient, state: &mut ShardServerState) -> Result<()> {
    let public_addr: SocketAddr = SHARD_PUBLIC_ADDR.parse().expect("Bad public address");
    let mut ip_buf: [u8; 16] = [0; 16];
    let ip_str: &str = &public_addr.ip().to_string();
    let ip_bytes: &[u8] = ip_str.as_bytes();
    ip_buf[..ip_bytes.len()].copy_from_slice(ip_bytes);

    let pkt: &sP_LS2FE_REQ_UPDATE_LOGIN_INFO = server.get_packet();
    let resp = sP_FE2LS_REP_UPDATE_LOGIN_INFO_SUCC {
        iEnterSerialKey: pkt.iEnterSerialKey,
        g_FE_ServerIP: ip_buf,
        g_FE_ServerPort: public_addr.port() as i32,
    };

    let serial_key = resp.iEnterSerialKey;
    let ld: &mut HashMap<i64, LoginData> = &mut state.login_data;
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
            iPC_UID: pkt.iPC_UID,
            uiFEKey: pkt.uiFEKey,
            uiSvrTime: pkt.uiSvrTime,
            // this should ideally be fetched from DB
            player: pkt.player,
        },
    );

    server.send_packet(P_FE2LS_REP_UPDATE_LOGIN_INFO_SUCC, &resp)?;
    Ok(())
}