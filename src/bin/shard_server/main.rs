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
    database::db_init,
    error::{log, logger_flush, logger_flush_scheduled, logger_init, FFError, FFResult, Severity},
    net::{
        crypto::{gen_key, EncryptionMode},
        ffclient::{ClientType, FFClient},
        ffserver::FFServer,
        packet::{
            PacketID::{self, *},
            *,
        },
        ClientMap, LoginData, CONN_ID_DISCONNECTED,
    },
    state::{shard::ShardServerState, ServerState},
    tabledata::tdata_init,
    timer::TimerMap,
    unused, Entity, EntityID,
};

fn main() -> Result<()> {
    let _cleanup = Cleanup {};

    let config = config_init();
    logger_init(config.shard.log_path.get());
    drop(db_init());
    tdata_init();

    let polling_interval = Duration::from_millis(50);
    let listen_addr = config_get().shard.listen_addr.get();
    let mut server = FFServer::new(
        &listen_addr,
        handle_packet,
        Some(handle_disconnect),
        Some(polling_interval),
    )?;

    let mut state = ServerState::new_shard();

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
        |t, srv, st| FFServer::do_live_checks(t, srv, st, send_live_check),
        Duration::from_secs(config.general.live_check_time.get()) / 2,
        false,
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

    log(Severity::Info, "Shard server shutting down...");
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
                Severity::Info,
                &format!("Login server ({}) disconnected", client.get_addr()),
            );
            state.set_login_server_conn_id(CONN_ID_DISCONNECTED);
        }
        ClientType::GameClient {
            pc_id: Some(pc_id), ..
        } => {
            let id = EntityID::Player(pc_id);
            let entity_map = &mut state.entity_map;
            entity_map.update(id, None, Some(&mut clients));
            let mut player = entity_map.untrack(id);
            player.cleanup(&mut clients, state);
        }
        _ => (),
    }
}

mod chat;
mod gm;
mod item;
mod login;
mod nano;
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
        P_LS2FE_REP_CONNECT_SUCC => login::login_connect_succ(clients.get_self(), state),
        P_LS2FE_REP_CONNECT_FAIL => login::login_connect_fail(clients.get_self()),
        P_LS2FE_REQ_UPDATE_LOGIN_INFO => login::login_update_info(clients.get_self(), state),
        P_LS2FE_REQ_LIVE_CHECK => login::login_live_check(clients.get_self()),
        //
        P_CL2LS_REQ_LOGIN => wrong_server(clients.get_self()),
        //
        P_CL2FE_REQ_PC_ENTER => pc::pc_enter(clients.get_self(), key, state, time),
        P_CL2FE_REQ_PC_LOADING_COMPLETE => pc::pc_loading_complete(&mut clients, state),
        P_CL2FE_REQ_PC_MOVE => pc::pc_move(&mut clients, state, time),
        P_CL2FE_REQ_PC_JUMP => pc::pc_jump(&mut clients, state, time),
        P_CL2FE_REQ_PC_STOP => pc::pc_stop(&mut clients, state, time),
        P_CL2FE_REQ_PC_VEHICLE_ON => pc::pc_vehicle_on(&mut clients, state),
        P_CL2FE_REQ_PC_VEHICLE_OFF => pc::pc_vehicle_off(&mut clients, state),
        P_CL2FE_REQ_PC_SPECIAL_STATE_SWITCH => pc::pc_special_state_switch(&mut clients, state),
        P_CL2FE_REQ_PC_FIRST_USE_FLAG_SET => pc::pc_first_use_flag_set(clients.get_self(), state),
        P_CL2FE_REQ_PC_CHANGE_MENTOR => pc::pc_change_mentor(clients.get_self(), state),
        P_CL2FE_REQ_PC_EXIT => pc::pc_exit(clients.get_self()),
        //
        P_CL2FE_REQ_PC_GIVE_ITEM => gm::gm_pc_give_item(clients.get_self(), state),
        P_CL2FE_GM_REQ_PC_SET_VALUE => gm::gm_pc_set_value(clients.get_self(), state),
        P_CL2FE_REQ_PC_GIVE_NANO => gm::gm_pc_give_nano(&mut clients, state),
        P_CL2FE_REQ_PC_GOTO => gm::gm_pc_goto(&mut clients, state),
        //
        P_CL2FE_REQ_SEND_FREECHAT_MESSAGE => chat::send_freechat_message(&mut clients, state),
        P_CL2FE_REQ_SEND_MENUCHAT_MESSAGE => chat::send_menuchat_message(&mut clients, state),
        P_CL2FE_REQ_PC_AVATAR_EMOTES_CHAT => chat::pc_avatar_emotes_chat(&mut clients, state),
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
        //
        P_CL2FE_REQ_NANO_EQUIP => nano::nano_equip(&mut clients, state),
        P_CL2FE_REQ_NANO_UNEQUIP => nano::nano_unequip(&mut clients, state),
        P_CL2FE_REQ_NANO_ACTIVE => nano::nano_active(&mut clients, state),
        P_CL2FE_REQ_NANO_TUNE => nano::nano_tune(clients.get_self(), state),
        //
        P_CL2FE_REQ_PC_TRADE_OFFER => trade::trade_offer(&mut clients, state),
        P_CL2FE_REQ_PC_TRADE_OFFER_REFUSAL => trade::trade_offer_refusal(&mut clients),
        P_CL2FE_REQ_PC_TRADE_OFFER_ACCEPT => trade::trade_offer_accept(&mut clients, state),
        P_CL2FE_REQ_PC_TRADE_OFFER_CANCEL => trade::trade_offer_cancel(&mut clients, state),
        P_CL2FE_REQ_PC_TRADE_CASH_REGISTER => trade::trade_cash_register(&mut clients, state),
        P_CL2FE_REQ_PC_TRADE_ITEM_REGISTER => trade::trade_item_register(&mut clients, state),
        P_CL2FE_REQ_PC_TRADE_ITEM_UNREGISTER => trade::trade_item_unregister(&mut clients, state),
        P_CL2FE_REQ_PC_TRADE_CONFIRM_CANCEL => trade::trade_confirm_cancel(&mut clients, state),
        P_CL2FE_REQ_PC_TRADE_CONFIRM => trade::trade_confirm(&mut clients, state),
        P_CL2FE_REQ_PC_TRADE_EMOTES_CHAT => trade::trade_emotes_chat(&mut clients),
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
        P_CL2FE_REP_LIVE_CHECK => Ok(()),
        //
        other => Err(FFError::build(
            Severity::Warning,
            format!("Unhandled packet: {:?}", other),
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
    state.get_login_server_conn_id() != CONN_ID_DISCONNECTED
}

fn send_live_check(client: &mut FFClient) -> FFResult<()> {
    match client.client_type {
        ClientType::GameClient {
            serial_key: _,
            pc_id: _,
        } => {
            client.live_check_pending = true;
            let pkt = sP_FE2CL_REQ_LIVE_CHECK {
                iTempValue: unused!(),
            };
            client.send_packet(P_FE2CL_REQ_LIVE_CHECK, &pkt)
        }
        _ => Ok(()),
    }
}
