use std::{
    cell::RefCell,
    collections::HashMap,
    io::Result,
    time::{Duration, SystemTime},
};

use rusty_fusion::{
    config::{config_get, config_init},
    error::{log, FFError, FFResult, Severity},
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
    tabledata::{tdata_get_npcs, tdata_init},
    util::get_time,
    Entity, EntityID,
};
use state::ShardServerState;

const CONN_ID_DISCONNECTED: i64 = -1;

mod state;

fn main() -> Result<()> {
    config_init();

    let polling_interval = Duration::from_millis(50);
    let listen_addr = config_get()
        .shard
        .listen_addr
        .unwrap_or("127.0.0.1:23001".to_string());
    let mut server = FFServer::new(&listen_addr, Some(polling_interval))?;

    let login_server_conn_interval =
        Duration::from_secs(config_get().shard.login_server_conn_interval.unwrap_or(10));
    let mut login_server_conn_time = SystemTime::UNIX_EPOCH;

    tdata_init();

    let state = RefCell::new(ShardServerState::new());
    for npc in tdata_get_npcs() {
        let mut state = state.borrow_mut();
        let chunk_pos = npc.get_position().chunk_coords();
        let entity_map = state.get_entity_map();
        let id = entity_map.track(Box::new(npc));
        entity_map.update(id, Some(chunk_pos), None);
    }

    let mut pkt_handler = |key, clients: &mut HashMap<usize, FFClient>, pkt_id| -> FFResult<()> {
        handle_packet(key, clients, pkt_id, &mut state.borrow_mut())
    };
    let mut dc_handler = |key, clients: &mut HashMap<usize, FFClient>| {
        handle_disconnect(key, clients, &mut state.borrow_mut());
    };

    log(
        Severity::Info,
        &format!("Shard server listening on {}", server.get_endpoint()),
    );
    loop {
        let time_now = SystemTime::now();
        if !is_login_server_connected(&state.borrow())
            && time_now.duration_since(login_server_conn_time).unwrap() > login_server_conn_interval
        {
            let login_server_addr = config_get()
                .shard
                .login_server_addr
                .unwrap_or("127.0.0.1:23000".to_string());
            log(
                Severity::Info,
                &format!("Connecting to login server at {}...", login_server_addr),
            );
            let conn = server.connect(&login_server_addr, ClientType::LoginServer);
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
        ClientType::LoginServer => {
            log(
                Severity::Info,
                &format!("Login server ({}) disconnected", client.get_addr()),
            );
            state.set_login_server_conn_id(CONN_ID_DISCONNECTED);
        }
        ClientType::GameClient {
            pc_uid: Some(pc_uid),
            ..
        } => {
            let id = EntityID::Player(pc_uid);
            let entity_map = state.get_entity_map();
            entity_map.update(id, None, Some(&mut clients));
            entity_map.untrack(id);
        }
        _ => (),
    }
}

mod chat;
mod gm;
mod item;
mod login;
mod pc;
fn handle_packet(
    key: usize,
    clients: &mut HashMap<usize, FFClient>,
    pkt_id: PacketID,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let mut clients = ClientMap::new(key, clients);
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
        P_CL2FE_REQ_PC_FIRST_USE_FLAG_SET => pc::pc_first_use_flag_set(clients.get_self(), state),
        //
        P_CL2FE_REQ_PC_GIVE_ITEM => gm::gm_pc_give_item(clients.get_self(), state),
        P_CL2FE_GM_REQ_PC_SET_VALUE => gm::gm_pc_set_value(clients.get_self(), state),
        //
        P_CL2FE_REQ_SEND_FREECHAT_MESSAGE => chat::send_freechat_message(&mut clients, state),
        P_CL2FE_REQ_SEND_MENUCHAT_MESSAGE => chat::send_menuchat_message(&mut clients, state),
        P_CL2FE_REQ_PC_AVATAR_EMOTES_CHAT => chat::pc_avatar_emotes_chat(&mut clients, state),
        //
        P_CL2FE_REQ_ITEM_MOVE => item::item_move(&mut clients, state),
        //
        other => Err(FFError::build(
            Severity::Warning,
            format!("Unhandled packet: {:?}", other),
        )),
    }
}

fn wrong_server(client: &mut FFClient) -> FFResult<()> {
    let pkt: &sP_CL2LS_REQ_LOGIN = client.get_packet(P_CL2LS_REQ_LOGIN);
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
