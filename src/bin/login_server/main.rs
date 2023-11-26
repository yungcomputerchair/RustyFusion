use std::{cell::RefCell, collections::HashMap, io::Result, time::Duration};

use rusty_fusion::{
    error::{log, FFError, FFResult, Severity},
    net::{
        crypto::{gen_key, DEFAULT_KEY},
        ffclient::FFClient,
        ffserver::FFServer,
        packet::{
            sP_LS2FE_REP_CONNECT_SUCC,
            PacketID::{self, *},
        },
    },
    player::Player,
    util::get_time,
};

const LOGIN_LISTEN_ADDR: &str = "127.0.0.1:23000";

pub struct LoginServerState {
    next_pc_uid: i64,
    next_shard_id: i64,
    pub players: HashMap<i64, Player>,
}

impl LoginServerState {
    fn new() -> Self {
        Self {
            next_pc_uid: 1,
            next_shard_id: 1,
            players: HashMap::new(),
        }
    }

    pub fn get_next_pc_uid(&mut self) -> i64 {
        let next = self.next_pc_uid;
        self.next_pc_uid += 1;
        next
    }

    pub fn get_next_shard_id(&mut self) -> i64 {
        let next = self.next_shard_id;
        self.next_shard_id += 1;
        next
    }
}

fn main() -> Result<()> {
    let polling_interval: Duration = Duration::from_millis(50);
    let mut server: FFServer = FFServer::new(LOGIN_LISTEN_ADDR, Some(polling_interval))?;

    let state = RefCell::new(LoginServerState::new());
    let mut pkt_handler = |key, clients: &mut HashMap<usize, FFClient>, pkt_id| -> FFResult<()> {
        handle_packet(key, clients, pkt_id, &mut state.borrow_mut())
    };

    log(
        Severity::Info,
        &format!("Login server listening on {}", server.get_endpoint()),
    );
    loop {
        server.poll(&mut pkt_handler, None)?;
    }
}

mod login;
mod shard;
fn handle_packet(
    key: usize,
    clients: &mut HashMap<usize, FFClient>,
    pkt_id: PacketID,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let client: &mut FFClient = clients.get_mut(&key).unwrap();
    match pkt_id {
        P_FE2LS_REQ_CONNECT => shard::connect(client, state),
        P_FE2LS_REP_UPDATE_LOGIN_INFO_SUCC => shard::update_login_info_succ(key, clients),
        P_FE2LS_REP_UPDATE_LOGIN_INFO_FAIL => shard::update_login_info_fail(key, clients),
        //
        P_CL2LS_REQ_LOGIN => login::login(client, state),
        P_CL2LS_REQ_CHECK_CHAR_NAME => login::check_char_name(client),
        P_CL2LS_REQ_SAVE_CHAR_NAME => login::save_char_name(client, state),
        P_CL2LS_REQ_CHAR_CREATE => login::char_create(client, state),
        P_CL2LS_REQ_SAVE_CHAR_TUTOR => login::save_char_tutor(client, state),
        P_CL2LS_REQ_CHAR_SELECT => login::char_select(key, clients, state),
        other => Err(FFError::new(
            Severity::Warning,
            format!("Unhandled packet: {:?}", other),
        )),
    }
}
