use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicI64, Ordering},
        Mutex, OnceLock,
    },
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
        LoginData,
    },
    Result,
};

const SHARD_LISTEN_ADDR: &str = "127.0.0.1:23001";
const SHARD_PUBLIC_ADDR: &str = SHARD_LISTEN_ADDR;

const LOGIN_SERVER_ADDR: &str = "127.0.0.1:23000";

const CONN_ID_DISCONNECTED: i64 = -1;
static LOGIN_SERVER_CONN_ID: AtomicI64 = AtomicI64::new(CONN_ID_DISCONNECTED);

fn main() -> Result<()> {
    let polling_interval: Duration = Duration::from_millis(50);
    let mut server: CNServer = CNServer::new(SHARD_LISTEN_ADDR, Some(polling_interval))?;

    let ls: &mut CNClient = server.connect(LOGIN_SERVER_ADDR, ClientType::LoginServer);
    login::login_connect_req(ls);
    thread::sleep(Duration::from_millis(2000));
    server.poll(&handle_packet)?;
    verify_login_server_conn();

    println!("Shard server listening on {}", server.get_endpoint());
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
        P_LS2FE_REP_CONNECT_SUCC => login::login_connect_succ(client),
        P_LS2FE_REP_CONNECT_FAIL => login::login_connect_fail(client),
        P_LS2FE_REQ_UPDATE_LOGIN_INFO => login::login_update_info(client),
        //
        P_CL2LS_REQ_LOGIN => wrong_server(client),
        //
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

fn verify_login_server_conn() {
    let conn_id: i64 = LOGIN_SERVER_CONN_ID.load(Ordering::Relaxed);
    if conn_id == CONN_ID_DISCONNECTED {
        panic!("Couldn't handshake with login server in time");
    }
}

fn login_data() -> &'static Mutex<HashMap<i64, LoginData>> {
    static MAP: OnceLock<Mutex<HashMap<i64, LoginData>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

mod login {
    use std::net::SocketAddr;

    use super::*;

    pub fn login_connect_req(server: &mut CNClient) {
        let pkt = sP_FE2LS_REQ_CONNECT { iTempValue: 0 };
        server
            .send_packet(P_FE2LS_REQ_CONNECT, &pkt)
            .expect("Couldn't connect to login server");
    }

    pub fn login_connect_succ(server: &mut CNClient) -> Result<()> {
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

    pub fn login_connect_fail(server: &mut CNClient) -> Result<()> {
        let pkt: &sP_LS2FE_REP_CONNECT_FAIL = server.get_packet();
        panic!("Login server refused to connect (error {})", {
            pkt.iErrorCode
        });
    }

    pub fn login_update_info(server: &mut CNClient) -> Result<()> {
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

        let mut ld = login_data().lock().unwrap();
        ld.insert(
            resp.iEnterSerialKey,
            LoginData {
                iPC_UID: pkt.iPC_UID,
                uiFEKey: pkt.uiFEKey,
                uiSvrTime: pkt.uiSvrTime,
            },
        );

        server.send_packet(P_FE2LS_REP_UPDATE_LOGIN_INFO_SUCC, &resp)?;
        Ok(())
    }
}
