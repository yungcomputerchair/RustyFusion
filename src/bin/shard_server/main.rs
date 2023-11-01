use std::{
    cell::RefCell,
    collections::HashMap,
    time::{Duration, SystemTime},
};

use rusty_fusion::{
    chunk::{pos_to_chunk_coords, EntityMap},
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
    npc::NPC,
    player::Player,
    tabledata::{tdata_get_npcs, tdata_init},
    util::get_time,
    Entity, EntityID, Result,
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

    pub fn get_player_mut(&mut self, pc_uid: i64) -> &mut Player {
        self.entities.get_player(pc_uid).unwrap()
    }

    pub fn update_player(&mut self, pc_uid: i64, f: impl FnOnce(&mut Player, &mut Self)) {
        // to avoid a double-borrow, we create a copy of the player and then replace it
        let mut player = *self.entities.get_player(pc_uid).unwrap();
        f(&mut player, self);
        *self.entities.get_player(pc_uid).unwrap() = player;
    }

    pub fn update_npc(&mut self, npc_id: i32, f: impl FnOnce(&mut NPC, &mut Self)) {
        // same as above
        let mut npc = *self.entities.get_npc(npc_id).unwrap();
        f(&mut npc, self);
        *self.entities.get_npc(npc_id).unwrap() = npc;
    }
}

fn main() -> Result<()> {
    let polling_interval: Duration = Duration::from_millis(50);
    let mut server: FFServer = FFServer::new(SHARD_LISTEN_ADDR, Some(polling_interval))?;

    let login_server_conn_interval: Duration = Duration::from_secs(10);
    let mut login_server_conn_time: SystemTime = SystemTime::UNIX_EPOCH;

    tdata_init();

    let state = RefCell::new(ShardServerState::new());
    for npc in tdata_get_npcs() {
        let mut state = state.borrow_mut();
        let chunk_pos = pos_to_chunk_coords(npc.get_position());
        let id = state.entities.track(Box::new(npc));
        state.entities.update(id, Some(chunk_pos), None);
    }

    let mut pkt_handler = |key, clients: &mut HashMap<usize, FFClient>, pkt_id| -> Result<()> {
        handle_packet(key, clients, pkt_id, &mut state.borrow_mut())
    };
    let mut dc_handler = |key, clients: &mut HashMap<usize, FFClient>| {
        handle_disconnect(key, clients, &mut state.borrow_mut());
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

fn handle_disconnect(
    key: usize,
    clients: &mut HashMap<usize, FFClient>,
    state: &mut ShardServerState,
) {
    let mut clients = ClientMap::new(key, clients);
    let client = clients.get_self();
    match client.get_client_type() {
        ClientType::LoginServer => state.set_login_server_conn_id(CONN_ID_DISCONNECTED),
        ClientType::GameClient {
            pc_uid: Some(pc_uid),
            ..
        } => {
            let id = EntityID::Player(pc_uid);
            state.entities.update(id, None, Some(&mut clients));
            state.entities.untrack(id);
        }
        _ => (),
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
        P_CL2FE_REQ_PC_SPECIAL_STATE_SWITCH => pc::pc_special_state_switch(&mut clients, state),
        //
        P_CL2FE_GM_REQ_PC_SET_VALUE => gm::gm_pc_set_value(clients.get_self(), state),
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
