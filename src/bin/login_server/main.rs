use std::{
    collections::HashMap,
    io::Result,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, SystemTime},
};

use rusty_fusion::{
    config::config_init,
    database::db_init,
    error::{log, logger_flush, logger_flush_scheduled, logger_init, FFError, FFResult, Severity},
    net::{
        crypto::{gen_key, DEFAULT_KEY},
        ffclient::{ClientType, FFClient},
        ffserver::FFServer,
        packet::{
            sP_LS2CL_REQ_LIVE_CHECK, sP_LS2FE_REP_CONNECT_SUCC, sP_LS2FE_REQ_LIVE_CHECK,
            PacketID::{self, *},
        },
    },
    player::Player,
    state::{login::LoginServerState, ServerState},
    tabledata::tdata_init,
    timer::TimerMap,
    unused,
};

fn main() -> Result<()> {
    let _cleanup = Cleanup {};

    let config = config_init();
    logger_init(config.login.log_path.get());
    drop(db_init());
    tdata_init();

    let polling_interval = Duration::from_millis(50);
    let listen_addr = config.login.listen_addr.get();
    let mut server = FFServer::new(
        &listen_addr,
        handle_packet,
        Some(handle_disconnect),
        Some(polling_interval),
    )?;

    let mut state = ServerState::new_login();

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Couldn't set signal handler");

    let mut timers = TimerMap::default();
    timers.register_timer(
        logger_flush_scheduled,
        Duration::from_secs(config.general.log_write_interval.get()),
        false,
    );
    timers.register_timer(
        |t, srv, st| FFServer::do_live_checks(t, srv, st, send_live_check),
        Duration::from_secs(config.general.live_check_time.get()) / 2,
        false,
    );

    log(
        Severity::Info,
        &format!("Login server listening on {}", server.get_endpoint()),
    );
    while running.load(Ordering::SeqCst) {
        timers
            .check_all(&mut server, &mut state)
            .unwrap_or_else(|e| {
                log(e.get_severity(), e.get_msg());
                if e.should_dc() {
                    panic!()
                }
            });
        server.poll(&mut state)?;
    }

    log(Severity::Info, "Login server shutting down...");
    Ok(())
}

struct Cleanup {}
impl Drop for Cleanup {
    fn drop(&mut self) {
        print!("Cleaning up...");
        logger_flush().expect("Errors writing final log");
        println!("done");
    }
}

fn handle_disconnect(key: usize, clients: &mut HashMap<usize, FFClient>, state: &mut ServerState) {
    let _state = state.as_login();
    let client = clients.get_mut(&key).unwrap();
    if let ClientType::ShardServer(shard_id) = client.client_type {
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
    state: &mut ServerState,
    time: SystemTime,
) -> FFResult<()> {
    let state = state.as_login();
    let client = clients.get_mut(&key).unwrap();
    match pkt_id {
        P_FE2LS_REQ_CONNECT => shard::connect(client, state, time),
        P_FE2LS_REP_UPDATE_LOGIN_INFO_SUCC => shard::update_login_info_succ(key, clients),
        P_FE2LS_REP_UPDATE_LOGIN_INFO_FAIL => shard::update_login_info_fail(key, clients),
        P_FE2LS_REP_LIVE_CHECK => Ok(()),
        //
        P_CL2LS_REQ_LOGIN => login::login(client, state, time),
        P_CL2LS_REQ_CHECK_CHAR_NAME => login::check_char_name(client),
        P_CL2LS_REQ_SAVE_CHAR_NAME => login::save_char_name(client, state),
        P_CL2LS_REQ_CHAR_CREATE => login::char_create(client, state),
        P_CL2LS_REQ_SAVE_CHAR_TUTOR => login::save_char_tutor(client, state),
        P_CL2LS_REQ_CHAR_SELECT => login::char_select(key, clients, state, time),
        P_CL2LS_REP_LIVE_CHECK => Ok(()),
        //
        other => Err(FFError::build(
            Severity::Warning,
            format!("Unhandled packet: {:?}", other),
        )),
    }
}

fn send_live_check(client: &mut FFClient) -> FFResult<()> {
    match client.client_type {
        ClientType::GameClient {
            serial_key: _,
            pc_id: _,
        } => {
            client.live_check_pending = true;
            let pkt = sP_LS2CL_REQ_LIVE_CHECK {
                iTempValue: unused!(),
            };
            client.send_packet(P_LS2CL_REQ_LIVE_CHECK, &pkt)
        }
        ClientType::ShardServer(_) => {
            client.live_check_pending = true;
            let pkt = sP_LS2FE_REQ_LIVE_CHECK {
                iTempValue: unused!(),
            };
            client.send_packet(P_LS2FE_REQ_LIVE_CHECK, &pkt)
        }
        _ => Ok(()),
    }
}
