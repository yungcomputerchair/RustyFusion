use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use crossterm::event::{self as ce, KeyCode};
use ffmonitor::PlayerEvent;

use rusty_fusion::{
    config::config_init,
    database::db_init,
    error::{log, log_error, log_if_failed, log_init, FFResult, Logger, Severity},
    geo::geo_init,
    monitor::{monitor_flush, monitor_init, monitor_queue, MonitorEvent},
    net::FFServer,
    server::login,
    state::LoginServerState,
    tabledata::tdata_init,
    tui::{LoginTui, Tui as _},
    util,
};

use futures::StreamExt;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> FFResult<()> {
    color_eyre::install().unwrap();

    let log_rx = log_init();
    let config = config_init()?;
    let mut logger = Logger::new(log_rx, &config.login.log_path.get());

    let mut tui = if config.general.enable_tui.get() {
        let terminal = ratatui::init();
        let tui = LoginTui::default();
        let ke = ce::EventStream::new();
        Some((terminal, tui, ke))
    } else {
        None
    };

    tdata_init()?;

    let mut tui_timer = util::make_timer(Duration::from_millis(250), true);
    let mut logger_timer = util::make_timer(
        Duration::from_secs(config.general.log_write_interval.get()),
        false,
    );
    let mut shard_conn_timer = util::make_timer(Duration::from_millis(250), false);
    let mut db_conn_timer = util::make_timer(
        Duration::from_secs(config.general.db_conn_retry_interval.get()),
        true,
    );
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

    let state = LoginServerState::default();
    let server_id = state.server_id;

    let state = Arc::new(Mutex::new(state));
    let live_check_time = Duration::from_secs(config.general.live_check_time.get());
    let listen_addr = config.login.listen_addr.get();
    let mut server = FFServer::new(
        listen_addr,
        login::handle_packet,
        Some(login::handle_disconnect),
        Some((live_check_time, login::send_live_check)),
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

    let mut fatal_error = None;
    loop {
        tokio::select! {
            res = server.poll() => {
                if let Err(e) = res {
                    let fatal = e.get_severity() == Severity::Fatal;
                    if fatal {
                        log_error(e.clone());
                        fatal_error = Some(e);
                        break;
                    }

                    log_error(e);
                }
            }
            ke = async { tui.as_mut().unwrap().2.next().await }, if tui.is_some() => {
                match ke {
                    Some(Ok(event)) => {
                        if let ce::Event::Key(key_event) = event {
                            if util::is_ctrl_c(&key_event) {
                                break;
                            }

                            let t = &mut tui.as_mut().unwrap().1;
                            match key_event.code {
                                KeyCode::Up => t.state.scroll(1),
                                KeyCode::Down => t.state.scroll(-1),
                                KeyCode::PageUp => t.state.scroll(10),
                                KeyCode::PageDown => t.state.scroll(-10),
                                KeyCode::Esc => t.state.reset_scroll(),
                                _ => {}
                            }
                        }
                    }
                    Some(Err(e)) => {
                        log(Severity::Warning, &format!("Error reading key event: {}", e));
                    }
                    None => {
                        tui = None;
                        ratatui::restore();
                        logger.disable_buffer();
                        log(
                            Severity::Warning,
                            "Key event stream ended; TUI disabled",
                        );
                    }
                }
            }
            _ = tokio::signal::ctrl_c(), if tui.is_none() => {
                break;
            }
            _ = tui_timer.tick() => {
                logger.drain();
                if let Some((terminal, tui, _)) = &mut tui {
                    let clients = server.get_clients().await;
                    let state = state.lock().await;
                    if let Err(e) = terminal.draw(|frame| tui.render(frame, &state, &clients, logger.buffer().unwrap())) {
                        log(
                            Severity::Warning,
                            &format!("Failed to draw TUI; skipping this frame: {}", e),
                        );
                    }
                }
            }
            _ = shard_conn_timer.tick() => {
                let clients = server.get_clients().await;
                state.lock().await
                    .process_shard_connection_requests(&clients, SystemTime::now());
            }
            _ = db_conn_timer.tick() => {
                log_if_failed(db_init(Severity::Warning).await);
            }
            _ = monitor_timer.tick() => {
                if monitor_enabled {
                    log_if_failed(send_monitor_update(&*state.lock().await));
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

    if let Some((terminal, tui, _)) = &mut tui {
        let _ =
            terminal.draw(|frame| tui.render(frame, &state, &clients, logger.buffer().unwrap()));
    }

    // disable TUI
    if tui.is_some() {
        ratatui::restore();
    }

    if let Some(e) = fatal_error {
        Err(e)
    } else {
        Ok(())
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
