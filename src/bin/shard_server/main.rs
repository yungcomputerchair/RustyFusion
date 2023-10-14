use std::{
    cell::{RefCell, RefMut},
    collections::HashMap,
    rc::Rc,
    time::{Duration, SystemTime},
};

use rusty_fusion::{
    chunk::EntityMap,
    error::BadRequest,
    net::{
        crypto::{gen_key, EncryptionMode},
        ffclient::{ClientType, FFClient},
        ffserver::FFServer,
        packet::{
            PacketID::{self, *},
            *,
        },
        ClientMap, LoginData,
    },
    player::Player,
    util::get_time,
    Result,
};

const SHARD_LISTEN_ADDR: &str = "127.0.0.1:23001";
const SHARD_PUBLIC_ADDR: &str = SHARD_LISTEN_ADDR;

const LOGIN_SERVER_ADDR: &str = "127.0.0.1:23000";

const CONN_ID_DISCONNECTED: i64 = -1;

pub struct ShardServerState {
    login_server_conn_id: i64,
    login_data: HashMap<i64, LoginData>,
    players: HashMap<i64, Rc<RefCell<Player>>>,
    entities: EntityMap,
}

impl ShardServerState {
    fn new() -> Self {
        Self {
            login_server_conn_id: CONN_ID_DISCONNECTED,
            login_data: HashMap::new(),
            players: HashMap::new(),
            entities: EntityMap::default(),
        }
    }

    pub fn get_login_server_conn_id(&self) -> i64 {
        self.login_server_conn_id
    }

    pub fn set_login_server_conn_id(&mut self, conn_id: i64) {
        self.login_server_conn_id = conn_id;
    }

    pub fn get_player(&mut self, pc_uid: &i64) -> Option<RefMut<Player>> {
        self.players.get(pc_uid).map(|player| player.borrow_mut())
    }
}

fn main() -> Result<()> {
    let polling_interval: Duration = Duration::from_millis(50);
    let mut server: FFServer = FFServer::new(SHARD_LISTEN_ADDR, Some(polling_interval))?;

    let login_server_conn_interval: Duration = Duration::from_secs(10);
    let mut login_server_conn_time: SystemTime = SystemTime::UNIX_EPOCH;

    let state = RefCell::new(ShardServerState::new());
    let mut pkt_handler = |key, clients: &mut HashMap<usize, FFClient>, pkt_id| -> Result<()> {
        handle_packet(key, clients, pkt_id, &mut state.borrow_mut())
    };
    let mut dc_handler = |client: FFClient| {
        handle_disconnect(client, &mut state.borrow_mut());
    };

    println!("Shard server listening on {}", server.get_endpoint());
    loop {
        let time_now = SystemTime::now();
        if !is_login_server_connected(&state.borrow())
            && time_now.duration_since(login_server_conn_time).unwrap() > login_server_conn_interval
        {
            println!("Connecting to login server at {}...", LOGIN_SERVER_ADDR);
            let conn = server.connect(LOGIN_SERVER_ADDR, ClientType::LoginServer);
            if let Some(login_server) = conn {
                login::login_connect_req(login_server);
            }
            login_server_conn_time = time_now;
        }
        server.poll(&mut pkt_handler, Some(&mut dc_handler))?;
    }
}

fn handle_disconnect(client: FFClient, state: &mut ShardServerState) {
    if matches!(client.get_client_type(), ClientType::LoginServer) {
        state.set_login_server_conn_id(CONN_ID_DISCONNECTED);
    }
}

fn handle_packet(
    key: usize,
    clients: &mut HashMap<usize, FFClient>,
    pkt_id: PacketID,
    state: &mut ShardServerState,
) -> Result<()> {
    let mut clients = ClientMap::new(key, clients);
    println!("{} sent {:?}", clients.get_self().get_addr(), pkt_id);
    match pkt_id {
        P_LS2FE_REP_CONNECT_SUCC => login::login_connect_succ(clients.get_self(), state),
        P_LS2FE_REP_CONNECT_FAIL => login::login_connect_fail(clients.get_self()),
        P_LS2FE_REQ_UPDATE_LOGIN_INFO => login::login_update_info(clients.get_self(), state),
        //
        P_CL2LS_REQ_LOGIN => wrong_server(clients.get_self()),
        //
        P_CL2FE_REQ_PC_ENTER => pc_enter(clients.get_self(), key, state),
        P_CL2FE_REQ_PC_LOADING_COMPLETE => pc_loading_complete(clients.get_self()),
        P_CL2FE_REQ_PC_MOVE => pc_move(&mut clients, state),
        P_CL2FE_REQ_PC_JUMP => pc_jump(&mut clients, state),
        P_CL2FE_REQ_PC_STOP => pc_stop(&mut clients, state),
        P_CL2FE_REQ_PC_GOTO => pc_goto(clients.get_self()),
        P_CL2FE_GM_REQ_PC_SET_VALUE => gm_pc_set_value(clients.get_self()),
        other => {
            println!("Unhandled packet: {:?}", other);
            Ok(())
        }
    }
}

fn wrong_server(client: &mut FFClient) -> Result<()> {
    let pkt: &sP_CL2LS_REQ_LOGIN = client.get_packet();
    let resp = sP_LS2CL_REP_LOGIN_FAIL {
        iErrorCode: 4, // "Login error"
        szID: pkt.szID,
    };
    client.send_packet(P_LS2CL_REP_LOGIN_FAIL, &resp)?;

    Ok(())
}

fn is_login_server_connected(state: &ShardServerState) -> bool {
    state.get_login_server_conn_id() != CONN_ID_DISCONNECTED
}

fn pc_enter(client: &mut FFClient, key: usize, state: &mut ShardServerState) -> Result<()> {
    let pkt: &sP_CL2FE_REQ_PC_ENTER = client.get_packet();
    let serial_key: i64 = pkt.iEnterSerialKey;
    let login_data = state.login_data.remove(&serial_key).unwrap();
    let mut player = login_data.player;
    player.set_client_id(key);

    let resp = sP_FE2CL_REP_PC_ENTER_SUCC {
        iID: login_data.iPC_UID as i32,
        PCLoadData2CL: player.get_load_data(),
        uiSvrTime: get_time(),
    };

    client.set_client_type(ClientType::GameClient {
        serial_key: pkt.iEnterSerialKey,
        pc_uid: Some(login_data.iPC_UID),
    });

    let iv1: i32 = resp.iID + 1;
    let iv2: i32 = resp.PCLoadData2CL.iFusionMatter + 1;
    client.set_e_key(gen_key(resp.uiSvrTime, iv1, iv2));
    client.set_fe_key(login_data.uiFEKey.to_le_bytes());
    client.set_enc_mode(EncryptionMode::FEKey);

    let player = Rc::new(RefCell::new(player));
    state.players.insert(login_data.iPC_UID, player.clone());
    state.entities.update(player.clone(), None);

    client.send_packet(P_FE2CL_REP_PC_ENTER_SUCC, &resp)?;
    Ok(())
}

fn pc_loading_complete(client: &mut FFClient) -> Result<()> {
    let pkt: &sP_CL2FE_REQ_PC_LOADING_COMPLETE = client.get_packet();
    let resp = sP_FE2CL_REP_PC_LOADING_COMPLETE_SUCC { iPC_ID: pkt.iPC_ID };
    client.send_packet(P_FE2CL_REP_PC_LOADING_COMPLETE_SUCC, &resp)?;

    Ok(())
}

fn gm_pc_set_value(client: &mut FFClient) -> Result<()> {
    let pkt: &sP_CL2FE_GM_REQ_PC_SET_VALUE = client.get_packet();
    let resp = sP_FE2CL_GM_REP_PC_SET_VALUE {
        iPC_ID: pkt.iPC_ID,
        iSetValue: pkt.iSetValue,
        iSetValueType: pkt.iSetValueType,
    };

    client.send_packet(P_FE2CL_GM_REP_PC_SET_VALUE, &resp)?;

    Ok(())
}

fn pc_goto(client: &mut FFClient) -> Result<()> {
    let pkt: &sP_CL2FE_REQ_PC_GOTO = client.get_packet();
    if let ClientType::GameClient {
        pc_uid: Some(_), ..
    } = client.get_client_type()
    {
        let resp = sP_FE2CL_REP_PC_GOTO_SUCC {
            iX: pkt.iToX,
            iY: pkt.iToY,
            iZ: pkt.iToZ,
        };
        client.send_packet(P_FE2CL_REP_PC_GOTO_SUCC, &resp)?;
        return Ok(());
    }

    Err(Box::new(BadRequest::new(client)))
}

fn pc_move(clients: &mut ClientMap, state: &mut ShardServerState) -> Result<()> {
    let client = clients.get_self();
    let pkt: &sP_CL2FE_REQ_PC_MOVE = client.get_packet();
    if let ClientType::GameClient {
        pc_uid: Some(pc_uid),
        ..
    } = client.get_client_type()
    {
        if let Some(mut player) = state.get_player(pc_uid) {
            player.set_position(pkt.iX, pkt.iY, pkt.iZ);
            let resp = sP_FE2CL_PC_MOVE {
                iCliTime: pkt.iCliTime,
                iX: pkt.iX,
                iY: pkt.iY,
                iZ: pkt.iZ,
                fVX: pkt.fVX,
                fVY: pkt.fVY,
                fVZ: pkt.fVZ,
                iAngle: pkt.iAngle,
                cKeyValue: pkt.cKeyValue,
                iSpeed: pkt.iSpeed,
                iID: *pc_uid as i32,
                iSvrTime: get_time(),
            };
            clients
                .get_all_gameclient_but_self()
                .try_for_each(|c| c.send_packet(P_FE2CL_PC_MOVE, &resp))?;
            return Ok(());
        }
    }

    Err(Box::new(BadRequest::new(client)))
}

fn pc_jump(clients: &mut ClientMap, state: &mut ShardServerState) -> Result<()> {
    let client = clients.get_self();
    let pkt: &sP_CL2FE_REQ_PC_JUMP = client.get_packet();
    if let ClientType::GameClient {
        pc_uid: Some(pc_uid),
        ..
    } = client.get_client_type()
    {
        if let Some(mut player) = state.get_player(pc_uid) {
            player.set_position(pkt.iX, pkt.iY, pkt.iZ);
            let resp = sP_FE2CL_PC_JUMP {
                iCliTime: pkt.iCliTime,
                iX: pkt.iX,
                iY: pkt.iY,
                iZ: pkt.iZ,
                iVX: pkt.iVX,
                iVY: pkt.iVY,
                iVZ: pkt.iVZ,
                iAngle: pkt.iAngle,
                cKeyValue: pkt.cKeyValue,
                iSpeed: pkt.iSpeed,
                iID: *pc_uid as i32,
                iSvrTime: get_time(),
            };
            clients
                .get_all_gameclient_but_self()
                .try_for_each(|c| c.send_packet(P_FE2CL_PC_JUMP, &resp))?;
            return Ok(());
        }
    }

    Err(Box::new(BadRequest::new(client)))
}

fn pc_stop(clients: &mut ClientMap, state: &mut ShardServerState) -> Result<()> {
    let client = clients.get_self();
    let pkt: &sP_CL2FE_REQ_PC_STOP = client.get_packet();
    if let ClientType::GameClient {
        pc_uid: Some(pc_uid),
        ..
    } = client.get_client_type()
    {
        if let Some(mut player) = state.get_player(pc_uid) {
            player.set_position(pkt.iX, pkt.iY, pkt.iZ);
            let resp = sP_FE2CL_PC_STOP {
                iCliTime: pkt.iCliTime,
                iX: pkt.iX,
                iY: pkt.iY,
                iZ: pkt.iZ,
                iID: *pc_uid as i32,
                iSvrTime: get_time(),
            };
            clients
                .get_all_gameclient_but_self()
                .try_for_each(|c| c.send_packet(P_FE2CL_PC_STOP, &resp))?;
            return Ok(());
        }
    }

    Err(Box::new(BadRequest::new(client)))
}

mod login {
    use std::net::SocketAddr;

    use super::*;

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
}
