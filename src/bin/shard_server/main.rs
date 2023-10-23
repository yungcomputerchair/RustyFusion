use std::{
    cell::RefCell,
    collections::HashMap,
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
    Entity, Result,
};

const SHARD_LISTEN_ADDR: &str = "127.0.0.1:23001";
const SHARD_PUBLIC_ADDR: &str = SHARD_LISTEN_ADDR;

const LOGIN_SERVER_ADDR: &str = "127.0.0.1:23000";

const CONN_ID_DISCONNECTED: i64 = -1;

pub struct ShardServerState {
    login_server_conn_id: i64,
    login_data: HashMap<i64, LoginData>,
    entities: EntityMap,
}

impl ShardServerState {
    fn new() -> Self {
        Self {
            login_server_conn_id: CONN_ID_DISCONNECTED,
            login_data: HashMap::new(),
            entities: EntityMap::default(),
        }
    }

    pub fn get_login_server_conn_id(&self) -> i64 {
        self.login_server_conn_id
    }

    pub fn set_login_server_conn_id(&mut self, conn_id: i64) {
        self.login_server_conn_id = conn_id;
    }

    pub fn update_player(&mut self, pc_uid: i64, f: impl FnOnce(&mut Player, &mut Self)) {
        // to avoid a double-borrow, we create a copy of the player and then replace it
        let mut player = *self.entities.get_player(pc_uid).unwrap();
        f(&mut player, self);
        *self.entities.get_player(pc_uid).unwrap() = player;
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

mod gm;
mod login;
mod pc;
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
        P_CL2FE_REQ_PC_ENTER => pc::pc_enter(clients.get_self(), key, state),
        P_CL2FE_REQ_PC_LOADING_COMPLETE => pc::pc_loading_complete(clients.get_self()),
        P_CL2FE_REQ_PC_MOVE => pc::pc_move(&mut clients, state),
        P_CL2FE_REQ_PC_JUMP => pc::pc_jump(&mut clients, state),
        P_CL2FE_REQ_PC_STOP => pc::pc_stop(&mut clients, state),
        P_CL2FE_REQ_PC_GOTO => pc::pc_goto(clients.get_self()),
        //
        P_CL2FE_GM_REQ_PC_SET_VALUE => gm::gm_pc_set_value(clients.get_self()),
        //
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
