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
    error::{
        log, log_error, logger_flush, logger_flush_scheduled, logger_init, panic_log, FFError,
        FFResult, Severity,
    },
    net::{
        packet::{
            PacketID::{self, *},
            *,
        },
        ClientType, FFClient, FFServer,
    },
    state::ServerState,
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
        Some(send_live_check),
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

    log(
        Severity::Info,
        &format!(
            "Login server listening on {} (ID: {})",
            server.get_endpoint(),
            state.as_login().server_id
        ),
    );
    let live_check_time = Duration::from_secs(config.general.live_check_time.get());
    while running.load(Ordering::SeqCst) {
        timers
            .check_all(&mut server, &mut state)
            .unwrap_or_else(|e| {
                if e.should_dc() {
                    panic_log(e.get_msg());
                } else {
                    log_error(&e);
                }
            });
        server.poll(&mut state, live_check_time)?;
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
    let state = state.as_login();
    let client = clients.get_mut(&key).unwrap();
    match client.client_type {
        ClientType::ShardServer(shard_id) => {
            state.unregister_shard(shard_id);
            log(
                Severity::Info,
                &format!(
                    "Shard server #{} ({}) disconnected",
                    shard_id,
                    client.get_addr()
                ),
            );
        }
        ClientType::GameClient { account_id, .. } => {
            state.end_session(account_id);
            log(
                Severity::Debug,
                &format!("Login session ended for account #{}", account_id),
            );
        }
        _ => (),
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
        P_FE2LS_UPDATE_PC_SHARD => shard::update_pc_shard(client, state),
        P_FE2LS_UPDATE_CHANNEL_STATUSES => shard::update_channel_statuses(client, state),
        P_FE2LS_REQ_MOTD => shard::motd(client),
        P_FE2LS_MOTD_REGISTER => shard::motd_register(client),
        P_FE2LS_ANNOUNCE_MSG => shard::announce_msg(key, clients),
        P_FE2LS_REQ_PC_LOCATION => shard::pc_location(key, clients, state),
        P_FE2LS_REP_PC_LOCATION_SUCC => shard::pc_location_succ(key, clients, state),
        P_FE2LS_REP_PC_LOCATION_FAIL => shard::pc_location_fail(key, clients, state),
        //
        P_CL2LS_REQ_LOGIN => login::login(client, state, time),
        P_CL2LS_REQ_PC_EXIT_DUPLICATE => login::pc_exit_duplicate(key, clients, state),
        P_CL2LS_REQ_SHARD_LIST_INFO => login::shard_list_info(client, state),
        P_CL2LS_REQ_CHECK_CHAR_NAME => login::check_char_name(client),
        P_CL2LS_REQ_SAVE_CHAR_NAME => login::save_char_name(client, state),
        P_CL2LS_REQ_CHAR_CREATE => login::char_create(client, state),
        P_CL2LS_REQ_CHAR_DELETE => login::char_delete(client, state),
        P_CL2LS_REQ_SAVE_CHAR_TUTOR => login::save_char_tutor(client, state),
        P_CL2LS_REQ_CHAR_SELECT => login::char_select(key, clients, state),
        P_CL2LS_REQ_SHARD_SELECT => login::shard_select(key, clients, state, time),
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
        ClientType::GameClient { .. } => {
            let pkt = sP_LS2CL_REQ_LIVE_CHECK {
                iTempValue: unused!(),
            };
            client.send_packet(P_LS2CL_REQ_LIVE_CHECK, &pkt)
        }
        ClientType::ShardServer(_) => {
            let pkt = sP_LS2FE_REQ_LIVE_CHECK {
                iTempValue: unused!(),
            };
            client.send_packet(P_LS2FE_REQ_LIVE_CHECK, &pkt)
        }
        _ => Ok(()),
    }
}
