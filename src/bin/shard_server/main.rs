use std::{
    collections::HashMap,
    sync::atomic::{AtomicI64, Ordering},
    thread,
    time::Duration,
};

use rusty_fusion::{
    net::{
        cnclient::{CNClient, ClientType},
        cnserver::CNServer,
        crypto::gen_key,
        packet::{
            PacketID::{self, *},
            *,
        },
    },
    Result,
};

const CONN_ID_DISCONNECTED: i64 = -1;
static LOGIN_SERVER_CONN_ID: AtomicI64 = AtomicI64::new(CONN_ID_DISCONNECTED);

fn main() -> Result<()> {
    let addr = "127.0.0.1:23001";
    let polling_interval: Duration = Duration::from_millis(50);
    let mut server: CNServer = CNServer::new(addr, Some(polling_interval))?;

    let ls_addr: &str = "127.0.0.1:23000";
    let ls: &mut CNClient = server.connect(ls_addr, ClientType::LoginServer);
    login_server_connect_req(ls);
    thread::sleep(Duration::from_millis(2000));
    server.poll(&handle_packet)?;
    verify_login_server_conn();

    println!("Shard server listening on {addr}");
    loop {
        server.poll(&handle_packet)?;
    }
}

fn handle_packet(
    key: &usize,
    clients: &mut HashMap<usize, CNClient>,
    pkt_id: PacketID,
) -> Result<()> {
    let client: &mut CNClient = clients.get_mut(key).unwrap();
    println!("{} sent {:?}", client.get_addr(), pkt_id);
    match pkt_id {
        P_LS2FE_REP_CONNECT_SUCC => login_server_connect_succ(client),
        P_LS2FE_REP_CONNECT_FAIL => login_server_connect_fail(client),
        //
        P_CL2LS_REQ_LOGIN => wrong_server(client),
        other => {
            println!("Unhandled packet: {:?}", other);
            Ok(())
        }
    }
}

fn wrong_server(client: &mut CNClient) -> Result<()> {
    let pkt: &sP_CL2LS_REQ_LOGIN = client.get_packet();
    let resp = sP_LS2CL_REP_LOGIN_FAIL {
        iErrorCode: 4, // "Login error"
        szID: pkt.szID,
    };
    client.send_packet(P_LS2CL_REP_LOGIN_FAIL, &resp)?;

    Ok(())
}

fn login_server_connect_req(server: &mut CNClient) {
    let pkt = sP_FE2LS_REQ_CONNECT { iTempValue: 0 };
    server
        .send_packet(P_FE2LS_REQ_CONNECT, &pkt)
        .expect("Couldn't connect to login server");
}

fn login_server_connect_succ(server: &mut CNClient) -> Result<()> {
    let pkt: &sP_LS2FE_REP_CONNECT_SUCC = server.get_packet();
    let conn_id: i64 = pkt.iConn_UID;
    let conn_time: u64 = pkt.uiSvrTime;

    let iv1: i32 = (conn_id + 1) as i32;
    let iv2: i32 = 69;
    server.set_e_key(gen_key(conn_time, iv1, iv2));

    LOGIN_SERVER_CONN_ID.store(conn_id, Ordering::Relaxed);
    println!("Connected to login server ({})", server.get_addr());
    Ok(())
}

fn login_server_connect_fail(server: &mut CNClient) -> Result<()> {
    let pkt: &sP_LS2FE_REP_CONNECT_FAIL = server.get_packet();
    panic!("Login server refused to connect (error {})", {
        pkt.iErrorCode
    });
}

fn verify_login_server_conn() {
    let conn_id: i64 = LOGIN_SERVER_CONN_ID.load(Ordering::Relaxed);
    if conn_id == CONN_ID_DISCONNECTED {
        panic!("Couldn't handshake with login server in time");
    }
}
