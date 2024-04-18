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
    config::{config_get, config_init},
    database::{db_init, db_run_parallel},
    defines::SHARD_TICKS_PER_SECOND,
    entity::Player,
    error::{
        log, log_error, log_if_failed, logger_flush, logger_flush_scheduled, logger_init,
        panic_log, FFError, FFResult, Severity,
    },
    net::{
        packet::{
            PacketID::{self, *},
            *,
        },
        ClientMap, ClientType, FFClient, FFServer,
    },
    state::{FFReceiver, ServerState, ShardServerState},
    tabledata::tdata_init,
    timer::TimerMap,
    unused,
};

fn main() -> Result<()> {
    let _cleanup = Cleanup {};

    let config = config_init();
    let shard_id = config.shard.shard_id.get();
    logger_init(config.shard.log_path.get());
    log(
        Severity::Info,
        &format!("Shard server #{} starting up...", shard_id),
    );
    drop(db_init());
    tdata_init();

    let polling_interval = Duration::from_millis(50);
    let listen_addr = config_get().shard.listen_addr.get();
    let mut server = FFServer::new(
        &listen_addr,
        handle_packet,
        Some(handle_disconnect),
        Some(send_live_check),
        Some(polling_interval),
    )?;

    let mut state = ServerState::new_shard(shard_id);

    let mut timers = TimerMap::default();
    timers.register_timer(
        logger_flush_scheduled,
        Duration::from_secs(config.general.log_write_interval.get()),
        false,
    );
    timers.register_timer(
        connect_to_login_server,
        Duration::from_secs(config.shard.login_server_conn_interval.get()),
        true,
    );
    timers.register_timer(
        |t, srv, st| {
            st.as_shard()
                .check_for_expired_vehicles(t, &mut srv.get_client_map());
            Ok(())
        },
        Duration::from_secs(60),
        false,
    );
    timers.register_timer(
        |t, srv, st| {
            st.as_shard().tick_entities(t, &mut srv.get_client_map());
            Ok(())
        },
        Duration::from_millis(1000 / SHARD_TICKS_PER_SECOND as u64),
        false,
    );
    timers.register_timer(
        |t, _, st| do_autosave(t, st.as_shard()),
        Duration::from_secs(config.shard.autosave_interval.get() * 60),
        false,
    );
    timers.register_timer(
        |_, _, st| {
            st.as_shard().check_receivers();
            Ok(())
        },
        Duration::from_secs(1),
        false,
    );

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Couldn't set signal handler");

    log(
        Severity::Info,
        &format!("Shard server listening on {}", server.get_endpoint()),
    );
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

    log(Severity::Info, "Shard server shutting down...");
    log_if_failed(do_autosave(SystemTime::now(), state.as_shard()));
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
    let state = state.as_shard();
    let mut clients = ClientMap::new(key, clients);
    let client = clients.get_self();
    match client.client_type {
        ClientType::LoginServer => {
            log(
                Severity::Warning,
                &format!("Login server ({}) disconnected", client.get_addr()),
            );
            state.login_server_conn_id = None;
        }
        ClientType::GameClient {
            pc_id: Some(pc_id), ..
        } => {
            // dirty exit; clean exit happens in P_CL2FE_REQ_PC_EXIT handler
            Player::disconnect(pc_id, state, &mut clients);
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

mod chat;
mod combat;
mod gm;
mod item;
mod login;
mod mission;
mod nano;
mod npc;
mod pc;
mod trade;
mod transport;
fn handle_packet(
    key: usize,
    clients: &mut HashMap<usize, FFClient>,
    pkt_id: PacketID,
    state: &mut ServerState,
    time: SystemTime,
) -> FFResult<()> {
    let state = state.as_shard();
    let mut clients = ClientMap::new(key, clients);
    match pkt_id {
        P_LS2FE_REP_AUTH_CHALLENGE => login::login_connect_challenge(clients.get_self(), state),
        P_LS2FE_REP_CONNECT_SUCC => login::login_connect_succ(clients.get_self(), state),
        P_LS2FE_REP_CONNECT_FAIL => login::login_connect_fail(clients.get_self()),
        P_LS2FE_REQ_UPDATE_LOGIN_INFO => login::login_update_info(clients.get_self(), state),
        P_LS2FE_REQ_LIVE_CHECK => login::login_live_check(clients.get_self()),
        P_LS2FE_REP_MOTD => login::login_motd(&mut clients, state),
        P_LS2FE_ANNOUNCE_MSG => login::login_announce_msg(&mut clients),
        P_LS2FE_REQ_PC_LOCATION => login::login_pc_location(&mut clients, state),
        P_LS2FE_REP_PC_LOCATION_SUCC => login::login_pc_location_succ(&mut clients, state),
        P_LS2FE_REP_PC_LOCATION_FAIL => login::login_pc_location_fail(&mut clients, state),
        P_LS2FE_REQ_PC_EXIT_DUPLICATE => login::login_pc_exit_duplicate(&mut clients, state),
        P_LS2FE_REP_LIVE_CHECK => Ok(()),
        //
        P_CL2LS_REQ_LOGIN => wrong_server(clients.get_self()),
        //
        P_CL2FE_REQ_PC_ENTER => pc::pc_enter(&mut clients, key, state, time),
        P_CL2FE_REQ_PC_LOADING_COMPLETE => pc::pc_loading_complete(&mut clients, state),
        P_CL2FE_REQ_PC_CHANNEL_NUM => pc::pc_channel_num(clients.get_self(), state),
        P_CL2FE_REQ_CHANNEL_INFO => pc::pc_channel_info(clients.get_self(), state),
        P_CL2FE_REQ_PC_WARP_CHANNEL => pc::pc_warp_channel(&mut clients, state),
        P_CL2FE_REQ_PC_MOVE => pc::pc_move(&mut clients, state, time),
        P_CL2FE_REQ_PC_JUMP => pc::pc_jump(&mut clients, state, time),
        P_CL2FE_REQ_PC_STOP => pc::pc_stop(&mut clients, state, time),
        P_CL2FE_REQ_PC_MOVETRANSPORTATION => pc::pc_movetransportation(&mut clients, state, time),
        P_CL2FE_REQ_PC_TRANSPORT_WARP => pc::pc_transport_warp(clients.get_self(), state),
        P_CL2FE_REQ_PC_VEHICLE_ON => pc::pc_vehicle_on(&mut clients, state),
        P_CL2FE_REQ_PC_VEHICLE_OFF => pc::pc_vehicle_off(&mut clients, state),
        P_CL2FE_REQ_PC_SPECIAL_STATE_SWITCH => pc::pc_special_state_switch(&mut clients, state),
        P_CL2FE_REQ_PC_COMBAT_BEGIN => pc::pc_combat_begin_end(&mut clients, state, true),
        P_CL2FE_REQ_PC_COMBAT_END => pc::pc_combat_begin_end(&mut clients, state, false),
        P_CL2FE_REQ_PC_FIRST_USE_FLAG_SET => pc::pc_first_use_flag_set(clients.get_self(), state),
        P_CL2FE_REQ_PC_CHANGE_MENTOR => pc::pc_change_mentor(clients.get_self(), state),
        P_CL2FE_REQ_PC_EXIT => pc::pc_exit(&mut clients, state),
        //
        P_CL2FE_REQ_PC_GIVE_ITEM => gm::gm_pc_give_item(clients.get_self(), state),
        P_CL2FE_GM_REQ_PC_SET_VALUE => gm::gm_pc_set_value(&mut clients, state),
        P_CL2FE_REQ_PC_GIVE_NANO => gm::gm_pc_give_nano(&mut clients, state),
        P_CL2FE_REQ_PC_GOTO => gm::gm_pc_goto(&mut clients, state),
        P_CL2FE_GM_REQ_PC_SPECIAL_STATE_SWITCH => {
            gm::gm_pc_special_state_switch(&mut clients, state)
        }
        P_CL2FE_GM_REQ_PC_MOTD_REGISTER => gm::gm_pc_motd_register(&mut clients, state),
        P_CL2FE_GM_REQ_PC_ANNOUNCE => gm::gm_pc_announce(&mut clients, state),
        P_CL2FE_GM_REQ_PC_LOCATION => gm::gm_pc_location(&mut clients, state),
        P_CL2FE_GM_REQ_TARGET_PC_SPECIAL_STATE_ONOFF => {
            gm::gm_target_pc_special_state_onoff(&mut clients, state)
        }
        P_CL2FE_GM_REQ_TARGET_PC_TELEPORT => gm::gm_target_pc_teleport(&mut clients, state),
        P_CL2FE_GM_REQ_KICK_PLAYER => gm::gm_kick_player(&mut clients, state),
        P_CL2FE_GM_REQ_REWARD_RATE => gm::gm_reward_rate(clients.get_self(), state),
        P_CL2FE_REQ_PC_TASK_COMPLETE => gm::gm_pc_task_complete(clients.get_self(), state),
        P_CL2FE_REQ_PC_MISSION_COMPLETE => gm::gm_pc_mission_complete(clients.get_self(), state),
        P_CL2FE_REQ_NPC_SUMMON => gm::gm_npc_summon(&mut clients, state),
        P_CL2FE_REQ_NPC_GROUP_SUMMON => gm::gm_npc_group_summon(&mut clients, state),
        P_CL2FE_REQ_NPC_UNSUMMON => gm::gm_npc_unsummon(&mut clients, state),
        P_CL2FE_REQ_SHINY_SUMMON => gm::gm_shiny_summon(&mut clients, state),
        //
        P_CL2FE_REQ_NPC_INTERACTION => npc::npc_interaction(clients.get_self(), state),
        P_CL2FE_REQ_BARKER => npc::npc_bark(clients.get_self(), state),
        //
        P_CL2FE_REQ_SEND_FREECHAT_MESSAGE => chat::send_freechat_message(&mut clients, state),
        P_CL2FE_REQ_SEND_MENUCHAT_MESSAGE => chat::send_menuchat_message(&mut clients, state),
        P_CL2FE_REQ_PC_AVATAR_EMOTES_CHAT => chat::pc_avatar_emotes_chat(&mut clients, state),
        //
        P_CL2FE_REQ_PC_ATTACK_NPCs => combat::pc_attack_npcs(&mut clients, state),
        //
        P_CL2FE_REQ_ITEM_MOVE => item::item_move(&mut clients, state),
        P_CL2FE_REQ_PC_ITEM_DELETE => item::item_delete(clients.get_self(), state),
        P_CL2FE_REQ_PC_ITEM_COMBINATION => item::item_combination(clients.get_self(), state),
        P_CL2FE_REQ_ITEM_CHEST_OPEN => item::item_chest_open(clients.get_self(), state),
        P_CL2FE_REQ_PC_VENDOR_START => item::vendor_start(clients.get_self(), state),
        P_CL2FE_REQ_PC_VENDOR_TABLE_UPDATE => item::vendor_table_update(clients.get_self()),
        P_CL2FE_REQ_PC_VENDOR_ITEM_BUY => item::vendor_item_buy(clients.get_self(), state, time),
        P_CL2FE_REQ_PC_VENDOR_ITEM_SELL => item::vendor_item_sell(clients.get_self(), state),
        P_CL2FE_REQ_PC_VENDOR_ITEM_RESTORE_BUY => {
            item::vendor_item_restore_buy(clients.get_self(), state)
        }
        P_CL2FE_REQ_PC_VENDOR_BATTERY_BUY => item::vendor_battery_buy(clients.get_self(), state),
        P_CL2FE_PC_STREETSTALL_REQ_CANCEL => item::streetstall_cancel(clients.get_self()),
        //
        P_CL2FE_REQ_NANO_EQUIP => nano::nano_equip(&mut clients, state),
        P_CL2FE_REQ_NANO_UNEQUIP => nano::nano_unequip(&mut clients, state),
        P_CL2FE_REQ_NANO_ACTIVE => nano::nano_active(&mut clients, state),
        P_CL2FE_REQ_NANO_TUNE => nano::nano_tune(clients.get_self(), state),
        //
        P_CL2FE_REQ_PC_TRADE_OFFER => trade::trade_offer(&mut clients, state),
        P_CL2FE_REQ_PC_TRADE_OFFER_REFUSAL => trade::trade_offer_refusal(&mut clients, state),
        P_CL2FE_REQ_PC_TRADE_OFFER_ACCEPT => trade::trade_offer_accept(&mut clients, state),
        P_CL2FE_REQ_PC_TRADE_OFFER_CANCEL => trade::trade_offer_cancel(&mut clients, state),
        P_CL2FE_REQ_PC_TRADE_CASH_REGISTER => trade::trade_cash_register(&mut clients, state),
        P_CL2FE_REQ_PC_TRADE_ITEM_REGISTER => trade::trade_item_register(&mut clients, state),
        P_CL2FE_REQ_PC_TRADE_ITEM_UNREGISTER => trade::trade_item_unregister(&mut clients, state),
        P_CL2FE_REQ_PC_TRADE_CONFIRM_CANCEL => trade::trade_confirm_cancel(&mut clients, state),
        P_CL2FE_REQ_PC_TRADE_CONFIRM => trade::trade_confirm(&mut clients, state),
        P_CL2FE_REQ_PC_TRADE_EMOTES_CHAT => trade::trade_emotes_chat(&mut clients, state),
        //
        P_CL2FE_REQ_REGIST_TRANSPORTATION_LOCATION => {
            transport::regist_transportation_location(clients.get_self(), state)
        }
        P_CL2FE_REQ_PC_WARP_USE_TRANSPORTATION => {
            transport::warp_use_transportation(&mut clients, state)
        }
        P_CL2FE_REQ_PC_WARP_USE_NPC => transport::warp_use_npc(&mut clients, state),
        P_CL2FE_REQ_PC_TIME_TO_GO_WARP => transport::time_to_go_warp(&mut clients, state),
        //
        P_CL2FE_REQ_PC_TASK_START => mission::task_start(clients.get_self(), state),
        P_CL2FE_REQ_PC_TASK_STOP => mission::task_stop(clients.get_self(), state),
        P_CL2FE_REQ_PC_TASK_END => mission::task_end(&mut clients, state),
        P_CL2FE_REQ_PC_SET_CURRENT_MISSION_ID => {
            mission::set_current_mission_id(clients.get_self(), state)
        }
        //
        P_CL2FE_REP_LIVE_CHECK => Ok(()),
        //
        _ => Err(FFError::build(
            Severity::Warning,
            "Unhandled packet".to_string(),
        )),
    }
}

fn wrong_server(client: &mut FFClient) -> FFResult<()> {
    let pkt: &sP_CL2LS_REQ_LOGIN = client.get_packet(P_CL2LS_REQ_LOGIN)?;
    let resp = sP_LS2CL_REP_LOGIN_FAIL {
        iErrorCode: 4, // "Login error"
        szID: pkt.szID,
    };
    client.send_packet(P_LS2CL_REP_LOGIN_FAIL, &resp)?;

    Ok(())
}

fn connect_to_login_server(
    _time: SystemTime,
    shard_server: &mut FFServer,
    state: &mut ServerState,
) -> FFResult<()> {
    let state = state.as_shard();
    if is_login_server_connected(state) {
        return Ok(());
    }

    let login_server_addr = config_get().shard.login_server_addr.get();
    log(
        Severity::Info,
        &format!("Connecting to login server at {}...", login_server_addr),
    );
    let conn = shard_server.connect(&login_server_addr, ClientType::LoginServer);
    if let Some(login_server) = conn {
        login::login_connect_req(login_server);
    }

    Ok(())
}

fn is_login_server_connected(state: &ShardServerState) -> bool {
    state.login_server_conn_id.is_some()
}

fn send_live_check(client: &mut FFClient) -> FFResult<()> {
    match client.client_type {
        ClientType::GameClient { .. } => {
            let pkt = sP_FE2CL_REQ_LIVE_CHECK {
                iTempValue: unused!(),
            };
            client.send_packet(P_FE2CL_REQ_LIVE_CHECK, &pkt)
        }
        ClientType::LoginServer => {
            let pkt = sP_FE2LS_REQ_LIVE_CHECK {
                iTempValue: unused!(),
            };
            client.send_packet(P_FE2LS_REQ_LIVE_CHECK, &pkt)
        }
        _ => Ok(()),
    }
}

fn do_autosave(time: SystemTime, state: &mut ShardServerState) -> FFResult<()> {
    let pc_ids: Vec<i32> = state.entity_map.get_player_ids().collect();
    if pc_ids.is_empty() {
        return Ok(());
    }

    log(
        Severity::Info,
        &format!("Autosaving {} player(s)...", pc_ids.len()),
    );
    let players: Vec<Player> = pc_ids
        .iter()
        .map(|pc_id| state.get_player(*pc_id).unwrap().clone())
        .collect();
    let rx = db_run_parallel(move |db| {
        let player_refs: Vec<&Player> = players.iter().collect();
        db.save_players(&player_refs, Some(time))
    })
    .map_err(|e| {
        FFError::build(
            Severity::Warning,
            format!("Autosave cancelled: {}", e.get_msg()),
        )
    })?;

    state.autosave_rx = Some(FFReceiver::new(time, rx));
    Ok(())
}
