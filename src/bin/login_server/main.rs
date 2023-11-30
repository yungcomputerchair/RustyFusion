use std::{
    cell::RefCell,
    collections::HashMap,
    io::Result,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use rusty_fusion::{
    config::config_init,
    error::{log, logger_init, logger_shutdown, FFError, FFResult, Severity},
    net::{
        crypto::{gen_key, DEFAULT_KEY},
        ffclient::{ClientType, FFClient},
        ffserver::FFServer,
        packet::{
            sP_LS2FE_REP_CONNECT_SUCC,
            PacketID::{self, *},
        },
    },
    player::Player,
    util::get_time,
};

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
    let _cleanup = Cleanup {};

    let config = config_init().login;
    logger_init(config.log_path.unwrap_or("login.log".to_string()));

    let polling_interval: Duration = Duration::from_millis(50);
    let listen_addr = config.listen_addr.unwrap_or("127.0.0.1:23000".to_string());
    let mut server: FFServer = FFServer::new(&listen_addr, Some(polling_interval))?;

    let state = RefCell::new(LoginServerState::new());
    let mut pkt_handler = |key, clients: &mut HashMap<usize, FFClient>, pkt_id| -> FFResult<()> {
        handle_packet(key, clients, pkt_id, &mut state.borrow_mut())
    };
    let mut dc_handler = |key, clients: &mut HashMap<usize, FFClient>| {
        handle_disconnect(key, clients);
    };

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Couldn't set signal handler");

    log(
        Severity::Info,
        &format!("Login server listening on {}", server.get_endpoint()),
    );
    while running.load(Ordering::SeqCst) {
        server.poll(&mut pkt_handler, Some(&mut dc_handler))?;
    }

    log(Severity::Info, "Login server shutting down...");
    Ok(())
}

struct Cleanup {}
impl Drop for Cleanup {
    fn drop(&mut self) {
        println!("Cleaning up...");
        logger_shutdown().expect("Errors shutting down logging");
    }
}

fn handle_disconnect(key: usize, clients: &mut HashMap<usize, FFClient>) {
    let client = clients.get_mut(&key).unwrap();
    if let ClientType::ShardServer(shard_id) = client.get_client_type() {
        log(
            Severity::Info,
            &format!(
                "Shard server #{} ({}) disconnected",
                shard_id,
                client.get_addr()
            ),
        );
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
    let client = clients.get_mut(&key).unwrap();
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
        other => Err(FFError::build(
            Severity::Warning,
            format!("Unhandled packet: {:?}", other),
        )),
    }
}
