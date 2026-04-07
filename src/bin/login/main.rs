use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime},
};

use crossterm::event::{self as ce, KeyCode};
use ffmonitor::PlayerEvent;

use rusty_fusion::{
    config::config_init,
    database::{db_init, db_shutdown},
    error::{log, log_if_failed, log_init, FFError, FFResult, Logger, Severity},
    geo::geo_init,
    monitor::{monitor_flush, monitor_init, monitor_queue, MonitorEvent},
    net::{
        packet::{PacketID::*, *},
        ClientType, FFClient, FFServer,
    },
    state::{LoginServerState, ServerState},
    tabledata::tdata_init,
    tui::{LoginTui, Tui as _},
    unused, util,
};

use futures::StreamExt;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> FFResult<()> {
    color_eyre::install().unwrap();
    let mut terminal = ratatui::init();
    let log_rx = log_init();
    let mut tui = LoginTui::default();

    let _cleanup = Cleanup {};

    let config = config_init();
    let mut logger = Logger::new(log_rx, &config.login.log_path.get());
    db_init().await;
    tdata_init();

    let mut tui_timer = util::make_timer(Duration::from_millis(100), true);
    let mut logger_timer = util::make_timer(
        Duration::from_secs(config.general.log_write_interval.get()),
        false,
    );
    let mut shard_conn_timer = util::make_timer(Duration::from_millis(250), false);
    let mut monitor_timer = util::make_timer(
        Duration::from_secs(config.login.monitor_interval.get()),
        false,
    );

    let monitor_enabled = config.login.monitor_enabled.get();
    if monitor_enabled {
        let monitor_addr = config.login.monitor_addr.get();
        monitor_init(monitor_addr);
    }

    let geo_db_path = config.login.geo_db_path.get();
    if !geo_db_path.is_empty() {
        if let Err(e) = geo_init(&geo_db_path) {
            log(
                Severity::Warning,
                &format!(
                    "GeoIP initialization failed: {}. Geo-based shard routing disabled.",
                    e
                ),
            );
        } else {
            log(
                Severity::Info,
                "GeoIP database loaded successfully. Geo-based shard routing enabled.",
            );
        }
    } else {
        log(
            Severity::Warning,
            "No GeoIP database configured. Geo-based shard routing disabled.",
        );
    }

    let state = ServerState::new_login();
    let server_id = state.as_login().server_id;

    let state = Arc::new(Mutex::new(state));
    let live_check_time = Duration::from_secs(config.general.live_check_time.get());
    let listen_addr = config.login.listen_addr.get();
    let mut server = FFServer::new(
        listen_addr,
        handle_packet,
        Some(handle_disconnect),
        Some((live_check_time, send_live_check)),
        state.clone(),
    )
    .await?;

    log(
        Severity::Info,
        &format!(
            "Login server listening on {} (ID: {})",
            server.get_endpoint(),
            server_id,
        ),
    );

    let mut key_event_stream = ce::EventStream::new();
    loop {
        // Check timers
        tokio::select! {
            res = server.poll() => {
                if let Err(e) = res {
                    log(Severity::Fatal, &format!("Error during server poll: {}", e));
                    break;
                }
            }
            ke = key_event_stream.next() => {
                match ke {
                    Some(Ok(event)) => {
                        if let ce::Event::Key(key_event) = event {
                            if util::is_ctrl_c(&key_event) {
                                break;
                            }
                            match key_event.code {
                                KeyCode::Up => tui.state.scroll(1),
                                KeyCode::Down => tui.state.scroll(-1),
                                KeyCode::PageUp => tui.state.scroll(10),
                                KeyCode::PageDown => tui.state.scroll(-10),
                                KeyCode::Esc => tui.state.reset_scroll(),
                                _ => {}
                            }
                        }
                    }
                    Some(Err(e)) => {
                        log(Severity::Warning, &format!("Error reading key event: {}", e));
                    }
                    None => {
                        // should not happen
                        log(Severity::Fatal, "Key event stream ended unexpectedly");
                        break;
                    }
                }
            }
            _ = tui_timer.tick() => {
                logger.drain();
                let clients = server.get_clients().await;
                let state = state.lock().await;
                if let Err(e) = terminal.draw(|frame| tui.render(frame, &state, &clients, logger.buffer())) {
                    log(
                        Severity::Warning,
                        &format!("Failed to draw TUI; skipping this frame: {}", e),
                    );
                }
            }
            _ = shard_conn_timer.tick() => {
                let clients = server.get_clients().await;
                state.lock().await.as_login_mut()
                    .process_shard_connection_requests(&clients, SystemTime::now());
            }
            _ = monitor_timer.tick() => {
                if monitor_enabled {
                    log_if_failed(send_monitor_update(state.lock().await.as_login()));
                }
            }
            _ = logger_timer.tick() => {
                logger.flush();
            }
        }
    }

    // final TUI render before cleanup
    log(Severity::Info, "Login server shutting down...");
    logger.drain();
    let clients = server.get_clients().await;
    let state = state.lock().await;
    let _ = terminal.draw(|frame| tui.render(frame, &state, &clients, logger.buffer()));

    Ok(())
}

struct Cleanup;
impl Drop for Cleanup {
    fn drop(&mut self) {
        ratatui::restore();
        db_shutdown();
    }
}

fn handle_disconnect(key: usize, clients: &HashMap<usize, FFClient>, state: &mut ServerState) {
    let state = state.as_login_mut();
    let client = clients.get(&key).unwrap();
    match client.get_client_type() {
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
                Severity::Info,
                &format!("Login session ended for account #{}", account_id),
            );
        }
        ClientType::Unknown => {}
        other => {
            log(
                Severity::Warning,
                &format!(
                    "Unhandled disconnect for client {} (type {})",
                    client.get_addr(),
                    other
                ),
            );
        }
    }
}

mod login;
mod shard;
fn handle_packet(
    pkt: Packet,
    key: usize,
    clients: &HashMap<usize, FFClient>,
    state: &mut ServerState,
) -> FFResult<()> {
    let time = SystemTime::now();
    let state = state.as_login_mut();
    let client = clients.get(&key).unwrap();
    match pkt.id() {
        P_FE2LS_REQ_AUTH_CHALLENGE => shard::auth_challenge(client),
        P_FE2LS_REQ_CONNECT => shard::connect(pkt, client, state, time),
        P_FE2LS_REP_UPDATE_LOGIN_INFO_SUCC => {
            shard::update_login_info_succ(pkt, key, clients, state)
        }
        P_FE2LS_REP_UPDATE_LOGIN_INFO_FAIL => shard::update_login_info_fail(pkt, clients),
        P_FE2LS_UPDATE_PC_STATUSES => shard::update_pc_statuses(pkt, client, state),
        P_FE2LS_UPDATE_MONITOR => shard::update_monitor(pkt),
        P_FE2LS_REQ_MOTD => shard::motd(pkt, client),
        P_FE2LS_MOTD_REGISTER => shard::motd_register(pkt),
        P_FE2LS_ANNOUNCE_MSG => shard::announce_msg(pkt, clients),
        P_FE2LS_REQ_PC_LOCATION => shard::pc_location(pkt, key, clients, state),
        P_FE2LS_REP_PC_LOCATION_SUCC => shard::pc_location_succ(pkt, clients, state),
        P_FE2LS_REP_PC_LOCATION_FAIL => shard::pc_location_fail(pkt, key, clients, state),
        P_FE2LS_REQ_GET_BUDDY_STATE => shard::get_buddy_state(pkt, key, clients, state),
        P_FE2LS_DISCONNECTING => shard::handle_disconnecting(key, clients, state),
        P_FE2LS_REQ_LIVE_CHECK => shard::shard_live_check(client),
        P_FE2LS_REQ_SEND_BUDDY_FREECHAT => shard::buddy_freechat(pkt, clients, state),
        P_FE2LS_REP_SEND_BUDDY_FREECHAT_SUCC => shard::buddy_freechat_succ(pkt, clients, state),
        P_FE2LS_REQ_SEND_BUDDY_MENUCHAT => shard::buddy_menuchat(pkt, clients, state),
        P_FE2LS_REP_SEND_BUDDY_MENUCHAT_SUCC => shard::buddy_menuchat_succ(pkt, clients, state),
        P_FE2LS_REQ_BUDDY_WARP => shard::buddy_warp(pkt, key, clients, state),
        P_FE2LS_REP_BUDDY_WARP_SUCC => shard::buddy_warp_succ(pkt, clients, state),
        P_FE2LS_REP_BUDDY_WARP_FAIL => shard::buddy_warp_fail(pkt, clients, state),
        P_FE2LS_REP_LIVE_CHECK => {
            client.clear_live_check();
            Ok(())
        }
        //
        P_CL2LS_REQ_LOGIN => login::login(pkt, client, state, time),
        P_CL2LS_REQ_PC_EXIT_DUPLICATE => login::pc_exit_duplicate(key, clients, state),
        P_CL2LS_REQ_SHARD_LIST_INFO => login::shard_list_info(client, state),
        P_CL2LS_REQ_CHECK_CHAR_NAME => login::check_char_name(pkt, client),
        P_CL2LS_REQ_SAVE_CHAR_NAME => login::save_char_name(pkt, client, state),
        P_CL2LS_REQ_CHAR_CREATE => login::char_create(pkt, client, state),
        P_CL2LS_REQ_CHAR_DELETE => login::char_delete(pkt, client, state),
        P_CL2LS_REQ_SAVE_CHAR_TUTOR => login::save_char_tutor(pkt, client, state),
        P_CL2LS_REQ_CHAR_SELECT => login::char_select(pkt, key, clients, state),
        P_CL2LS_REQ_SHARD_SELECT => login::shard_select(pkt, key, clients, state, time),
        P_CL2LS_REP_LIVE_CHECK => {
            client.clear_live_check();
            Ok(())
        }
        //
        _ => Err(FFError::build(
            Severity::Warning,
            "Unhandled packet".to_string(),
        )),
    }
}

fn send_live_check(client: &FFClient) {
    match client.get_client_type() {
        ClientType::GameClient { .. } => {
            let pkt = sP_LS2CL_REQ_LIVE_CHECK {
                iTempValue: unused!(),
            };
            client.send_packet(P_LS2CL_REQ_LIVE_CHECK, &pkt);
        }
        ClientType::ShardServer(_) | ClientType::UnauthedShardServer(_) => {
            let pkt = sP_LS2FE_REQ_LIVE_CHECK {
                iTempValue: unused!(),
            };
            client.send_packet(P_LS2FE_REQ_LIVE_CHECK, &pkt);
        }
        _ => {}
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
