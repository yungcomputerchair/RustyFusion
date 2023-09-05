use std::{cell::RefCell, collections::HashMap, time::Duration};

use rusty_fusion::{
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
    Result,
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
    let mut pkt_handler = |key, clients: &mut HashMap<usize, FFClient>, pkt_id| -> Result<()> {
        handle_packet(key, clients, pkt_id, &mut state.borrow_mut())
    };

    println!("Login server listening on {}", server.get_endpoint());
    loop {
        server.poll(&mut pkt_handler, None)?;
    }
}

fn handle_packet(
    key: usize,
    clients: &mut HashMap<usize, FFClient>,
    pkt_id: PacketID,
    state: &mut LoginServerState,
) -> Result<()> {
    let client: &mut FFClient = clients.get_mut(&key).unwrap();
    println!("{} sent {:?}", client.get_addr(), pkt_id);
    match pkt_id {
        P_FE2LS_REQ_CONNECT => shard::connect(client, state),
        P_FE2LS_REP_UPDATE_LOGIN_INFO_SUCC => shard::update_login_info_succ(key, clients),
        P_FE2LS_REP_UPDATE_LOGIN_INFO_FAIL => shard::update_login_info_fail(key, clients),
        //
        P_CL2LS_REQ_LOGIN => handlers::login(client),
        P_CL2LS_REQ_CHECK_CHAR_NAME => handlers::check_char_name(client),
        P_CL2LS_REQ_SAVE_CHAR_NAME => handlers::save_char_name(client, state),
        P_CL2LS_REQ_CHAR_CREATE => handlers::char_create(client, state),
        P_CL2LS_REQ_CHAR_SELECT => handlers::char_select(key, clients, state),
        other => {
            println!("Unhandled packet: {:?}", other);
            Ok(())
        }
    }
}

mod shard {
    use super::*;
    use rusty_fusion::net::{ffclient::ClientType, packet::*};

    pub fn connect(server: &mut FFClient, state: &mut LoginServerState) -> Result<()> {
        let conn_id: i64 = state.get_next_shard_id();
        server.set_client_type(ClientType::ShardServer(conn_id));
        let resp = sP_LS2FE_REP_CONNECT_SUCC {
            uiSvrTime: get_time(),
            iConn_UID: conn_id,
        };
        server.send_packet(P_LS2FE_REP_CONNECT_SUCC, &resp)?;

        let iv1: i32 = (conn_id + 1) as i32;
        let iv2: i32 = 69;
        server.set_e_key(gen_key(resp.uiSvrTime, iv1, iv2));

        Ok(())
    }

    pub fn update_login_info_succ(
        shard_key: usize,
        clients: &mut HashMap<usize, FFClient>,
    ) -> Result<()> {
        let server: &mut FFClient = clients.get_mut(&shard_key).unwrap();
        let pkt: &sP_FE2LS_REP_UPDATE_LOGIN_INFO_SUCC = server.get_packet();

        let resp = sP_LS2CL_REP_SHARD_SELECT_SUCC {
            g_FE_ServerIP: pkt.g_FE_ServerIP,
            g_FE_ServerPort: pkt.g_FE_ServerPort,
            iEnterSerialKey: pkt.iEnterSerialKey,
        };

        let client: &mut FFClient = clients
            .values_mut()
            .find(|c| match c.get_client_type() {
                ClientType::GameClient {
                    serial_key: key, ..
                } => *key == resp.iEnterSerialKey,
                _ => false,
            })
            .unwrap();
        client.send_packet(P_LS2CL_REP_SHARD_SELECT_SUCC, &resp)?;

        Ok(())
    }

    pub fn update_login_info_fail(
        shard_key: usize,
        clients: &mut HashMap<usize, FFClient>,
    ) -> Result<()> {
        let server: &mut FFClient = clients.get_mut(&shard_key).unwrap();
        let pkt: &sP_FE2LS_REP_UPDATE_LOGIN_INFO_FAIL = server.get_packet();
        let resp = sP_LS2CL_REP_CHAR_SELECT_FAIL {
            iErrorCode: pkt.iErrorCode,
        };

        let serial_key: i64 = pkt.iEnterSerialKey;
        let client: &mut FFClient = clients
            .values_mut()
            .find(|c| match c.get_client_type() {
                ClientType::GameClient {
                    serial_key: key, ..
                } => *key == serial_key,
                _ => false,
            })
            .unwrap();

        client.send_packet(P_LS2CL_REP_CHAR_SELECT_FAIL, &resp)?;

        Ok(())
    }
}

mod handlers {
    use super::*;
    use rand::random;
    use rusty_fusion::{
        defines::*,
        error::BadRequest,
        net::{ffclient::ClientType, packet::*},
        placeholder, Combatant, Item,
    };

    pub fn login(client: &mut FFClient) -> Result<()> {
        let pkt: &sP_CL2LS_REQ_LOGIN = client.get_packet();
        let resp = sP_LS2CL_REP_LOGIN_SUCC {
            iCharCount: 0,
            iSlotNum: 0,
            iPaymentFlag: 1,
            iTempForPacking4: 69,
            uiSvrTime: get_time(),
            szID: pkt.szID,
            iOpenBetaFlag: 0,
        };
        let e_base: u64 = resp.uiSvrTime;
        let e_iv1: i32 = (resp.iCharCount + 1) as i32;
        let e_iv2: i32 = (resp.iSlotNum + 1) as i32;
        let fe_base: u64 = u64::from_le_bytes(DEFAULT_KEY.try_into().unwrap());
        let fe_iv1: i32 = pkt.iClientVerC;
        let fe_iv2: i32 = 1;

        client.send_packet(P_LS2CL_REP_LOGIN_SUCC, &resp)?;

        client.set_e_key(gen_key(e_base, e_iv1, e_iv2));
        client.set_fe_key(gen_key(fe_base, fe_iv1, fe_iv2));

        let serial_key: i64 = random();
        client.set_client_type(ClientType::GameClient {
            serial_key,
            pc_uid: None,
        });

        Ok(())
    }

    pub fn check_char_name(client: &mut FFClient) -> Result<()> {
        let pkt: &sP_CL2LS_REQ_CHECK_CHAR_NAME = client.get_packet();
        let resp = sP_LS2CL_REP_CHECK_CHAR_NAME_SUCC {
            szFirstName: pkt.szFirstName,
            szLastName: pkt.szLastName,
        };
        client.send_packet(P_LS2CL_REP_CHECK_CHAR_NAME_SUCC, &resp)?;

        Ok(())
    }

    pub fn save_char_name(client: &mut FFClient, state: &mut LoginServerState) -> Result<()> {
        let pkt: &sP_CL2LS_REQ_SAVE_CHAR_NAME = client.get_packet();

        let pc_uid = state.get_next_pc_uid();
        let mut player = Player::new(pc_uid);
        player.set_name(1, pkt.szFirstName, pkt.szLastName);
        let style = &player.get_style();

        let resp = sP_LS2CL_REP_SAVE_CHAR_NAME_SUCC {
            iPC_UID: pc_uid,
            iSlotNum: placeholder!(0),
            iGender: style.iGender,
            szFirstName: style.szFirstName,
            szLastName: style.szLastName,
        };
        client.send_packet(P_LS2CL_REP_SAVE_CHAR_NAME_SUCC, &resp)?;
        state.players.insert(pc_uid, player);

        Ok(())
    }

    pub fn char_create(client: &mut FFClient, state: &mut LoginServerState) -> Result<()> {
        let pkt: &sP_CL2LS_REQ_CHAR_CREATE = client.get_packet();

        let pc_uid: i64 = pkt.PCStyle.iPC_UID;
        let player = state.players.get_mut(&pc_uid).unwrap();
        player.set_style(pkt.PCStyle);
        player.set_item(
            EQUIP_SLOT_UPPERBODY as usize,
            Item::new(EQUIP_SLOT_UPPERBODY as i16, pkt.sOn_Item.iEquipUBID),
        );
        player.set_item(
            EQUIP_SLOT_LOWERBODY as usize,
            Item::new(EQUIP_SLOT_LOWERBODY as i16, pkt.sOn_Item.iEquipLBID),
        );
        player.set_item(
            EQUIP_SLOT_FOOT as usize,
            Item::new(EQUIP_SLOT_FOOT as i16, pkt.sOn_Item.iEquipFootID),
        );

        let resp = sP_LS2CL_REP_CHAR_CREATE_SUCC {
            iLevel: player.get_level(),
            sPC_Style: player.get_style(),
            sPC_Style2: player.get_style_2(),
            sOn_Item: pkt.sOn_Item,
        };

        client.send_packet(P_LS2CL_REP_CHAR_CREATE_SUCC, &resp)?;
        Ok(())
    }

    pub fn char_select(
        client_key: usize,
        clients: &mut HashMap<usize, FFClient>,
        state: &mut LoginServerState,
    ) -> Result<()> {
        let client: &mut FFClient = clients.get_mut(&client_key).unwrap();
        if let ClientType::GameClient { serial_key, .. } = client.get_client_type() {
            let pkt: &sP_CL2LS_REQ_CHAR_SELECT = client.get_packet();
            let pc_uid: i64 = pkt.iPC_UID;
            let login_info = sP_LS2FE_REQ_UPDATE_LOGIN_INFO {
                iEnterSerialKey: *serial_key,
                iPC_UID: pc_uid,
                uiFEKey: client.get_fe_key_uint(),
                uiSvrTime: get_time(),
                player: *state.players.get(&pc_uid).unwrap(),
            };

            let shard_server = clients
                .values_mut()
                .find(|c| matches!(c.get_client_type(), ClientType::ShardServer(_)));

            match shard_server {
                Some(shard) => {
                    shard.send_packet(P_LS2FE_REQ_UPDATE_LOGIN_INFO, &login_info)?;
                }
                None => {
                    // no shards available
                    let resp = sP_LS2CL_REP_CHAR_SELECT_FAIL { iErrorCode: 1 };
                    let client: &mut FFClient = clients.get_mut(&client_key).unwrap();
                    client.send_packet(P_LS2CL_REP_CHAR_SELECT_FAIL, &resp)?;
                }
            }

            return Ok(());
        }

        Err(Box::new(BadRequest::new(client)))
    }
}
