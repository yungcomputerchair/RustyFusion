use std::{
    collections::HashMap,
    io::Result,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, SystemTime},
};

use ffmonitor::PlayerEvent;
use rusty_fusion::{
    config::config_init,
    database::{db_init, db_shutdown},
    error::{
        log, log_error, log_if_failed, logger_flush, logger_flush_scheduled, logger_init,
        panic_log, FFError, FFResult, Severity,
    },
    monitor::{monitor_flush, monitor_init, monitor_queue, MonitorEvent},
    net::{
        packet::{
            PacketID::{self, *},
            *,
        },
        ClientType, FFClient, FFServer,
    },
    state::{LoginServerState, ServerState},
    tabledata::tdata_init,
    timer::TimerMap,
    unused,
};

fn main() -> Result<()> {
    let mut cleanup = Cleanup::default();

    let config = config_init();
    logger_init(config.login.log_path.get());
    cleanup.db_thread_handle = Some(db_init());
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
        Box::new(|_, _, _| logger_flush_scheduled()),
        Duration::from_secs(config.general.log_write_interval.get()),
        false,
    );
    timers.register_timer(
        Box::new(|t, srv, st| {
            st.as_login()
                .process_shard_connection_requests(srv.get_clients(), t);
            Ok(())
        }),
        Duration::from_millis(250),
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

    if config.login.monitor_enabled.get() {
        let monitor_addr = config.login.monitor_addr.get();
        monitor_init(monitor_addr);

        let monitor_interval = config.login.monitor_interval.get();
        timers.register_timer(
            Box::new(move |_, _, st| send_monitor_update(st.as_login())),
            Duration::from_secs(monitor_interval),
            false,
        );
    }

    let live_check_time = Duration::from_secs(config.general.live_check_time.get());
    while running.load(Ordering::SeqCst) {
        server.poll(&mut state, live_check_time)?;
        timers
            .check_all(&mut server, &mut state)
            .unwrap_or_else(|e| {
                if e.should_dc() {
                    panic_log(e.get_msg());
                } else {
                    log_error(&e);
                }
            });
    }

    log(Severity::Info, "Login server shutting down...");
    Ok(())
}

#[derive(Default)]
struct Cleanup {
    db_thread_handle: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        print!("Cleaning up...");
        if let Some(handle) = self.db_thread_handle.take() {
            db_shutdown();
            handle.join().unwrap();
        }
        if let Err(e) = logger_flush() {
            println!("Could not flush log: {}", e);
        }
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
            log_if_failed(state.end_session(account_id));
            log(
                Severity::Debug,
                &format!("Login session ended for account #{}", account_id),
            );
        }
        ClientType::Unknown => {
            log(
                Severity::Debug,
                &format!("Client disconnected: {}", client.get_addr()),
            );
        }
        _ => {
            log(
                Severity::Warning,
                &format!(
                    "Unhandled disconnect for client {} (type {:?})",
                    client.get_addr(),
                    client.client_type
                ),
            );
        }
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
        P_FE2LS_REQ_AUTH_CHALLENGE => shard::auth_challenge(client),
        P_FE2LS_REQ_CONNECT => shard::connect(client, state, time),
        P_FE2LS_REP_UPDATE_LOGIN_INFO_SUCC => shard::update_login_info_succ(key, clients),
        P_FE2LS_REP_UPDATE_LOGIN_INFO_FAIL => shard::update_login_info_fail(key, clients),
        P_FE2LS_REP_LIVE_CHECK => Ok(()),
        P_FE2LS_UPDATE_PC_STATUSES => shard::update_pc_statuses(client, state),
        P_FE2LS_REQ_MOTD => shard::motd(client),
        P_FE2LS_MOTD_REGISTER => shard::motd_register(client),
        P_FE2LS_ANNOUNCE_MSG => shard::announce_msg(key, clients),
        P_FE2LS_REQ_PC_LOCATION => shard::pc_location(key, clients, state),
        P_FE2LS_REP_PC_LOCATION_SUCC => shard::pc_location_succ(key, clients, state),
        P_FE2LS_REP_PC_LOCATION_FAIL => shard::pc_location_fail(key, clients, state),
        P_FE2LS_REQ_GET_BUDDY_STATE => shard::get_buddy_state(key, clients, state),
        P_FE2LS_DISCONNECTING => shard::handle_disconnecting(key, clients, state),
        P_FE2LS_REQ_LIVE_CHECK => shard::shard_live_check(client),
        P_FE2LS_REQ_SEND_BUDDY_FREECHAT => shard::buddy_freechat(key, clients, state),
        P_FE2LS_REP_SEND_BUDDY_FREECHAT_SUCC => shard::buddy_freechat_succ(key, clients, state),
        P_FE2LS_REQ_SEND_BUDDY_MENUCHAT => shard::buddy_menuchat(key, clients, state),
        P_FE2LS_REP_SEND_BUDDY_MENUCHAT_SUCC => shard::buddy_menuchat_succ(key, clients, state),
        P_FE2LS_REQ_BUDDY_WARP => shard::buddy_warp(key, clients, state),
        P_FE2LS_REP_BUDDY_WARP_SUCC => shard::buddy_warp_succ(key, clients, state),
        P_FE2LS_REP_BUDDY_WARP_FAIL => shard::buddy_warp_fail(key, clients, state),
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
        _ => Err(FFError::build(
            Severity::Warning,
            "Unhandled packet".to_string(),
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

fn send_monitor_update(state: &LoginServerState) -> FFResult<()> {
    for data in state.get_all_shard_player_data() {
        monitor_queue(MonitorEvent::Player(PlayerEvent {
            x_coord: data.x_coord,
            y_coord: data.y_coord,
            name: format!("{} {}", data.first_name, data.last_name),
        }));
    }
    monitor_flush()
}
