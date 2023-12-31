use std::time::SystemTime;

use super::*;

use rand::random;

use rusty_fusion::{
    defines::*,
    enums::{ItemLocation, ItemType},
    error::{FFError, FFResult, Severity},
    net::{ffclient::ClientType, packet::*},
    placeholder,
    player::TEST_ACC_UID_START,
    unused, util, Combatant, Entity, Item, Position,
};

pub fn login(
    client: &mut FFClient,
    state: &mut LoginServerState,
    time: SystemTime,
) -> FFResult<()> {
    // TODO failure
    let pkt: &sP_CL2LS_REQ_LOGIN = client.get_packet(P_CL2LS_REQ_LOGIN)?;

    let mut players = Vec::new();
    let mut username = util::parse_utf16(&pkt.szID);
    let mut _password = util::parse_utf16(&pkt.szPassword);
    if username.is_empty() {
        username = util::parse_utf8(&pkt.szCookie_TEGid);
        _password = util::parse_utf8(&pkt.szCookie_authid);
    }
    if username.eq("test") {
        let mut player = Player::new(TEST_ACC_UID_START);
        player.set_name(1, util::encode_utf16("TestF"), util::encode_utf16("TestL"));
        player.set_god_mode(true);
        player.set_position(Position {
            x: 534829,
            y: 538992,
            z: -1029,
        }); // Mt. Blackhead
        player.set_item(
            ItemLocation::Equip,
            EQUIP_SLOT_VEHICLE as usize,
            Some(Item::new(ItemType::Vehicle, ID_KND_HOVERBOARD)),
        )?;
        players.push(player);

        let mut player = Player::new(TEST_ACC_UID_START + 1);
        player.set_name(1, util::encode_utf16("TestF"), util::encode_utf16("TestL"));
        players.push(player);
    }

    let resp = sP_LS2CL_REP_LOGIN_SUCC {
        iCharCount: players.len() as i8,
        iSlotNum: placeholder!(1),
        iPaymentFlag: 1,
        iTempForPacking4: unused!(),
        uiSvrTime: util::get_timestamp_ms(time),
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

    client.e_key = gen_key(e_base, e_iv1, e_iv2);
    client.fe_key = gen_key(fe_base, fe_iv1, fe_iv2);

    let serial_key: i64 = random();
    client.client_type = ClientType::GameClient {
        serial_key,
        pc_id: None,
    };

    players
        .into_iter()
        .enumerate()
        .try_for_each(|(slot, player)| {
            let pos = player.get_position();
            let pkt = sP_LS2CL_REP_CHAR_INFO {
                iSlot: (slot + 1) as i8,
                iLevel: player.get_level(),
                sPC_Style: player.get_style(),
                sPC_Style2: player.get_style_2(),
                iX: pos.x,
                iY: pos.y,
                iZ: pos.z,
                aEquip: player.get_equipped().map(Option::<Item>::into),
            };
            state.players.insert(pkt.sPC_Style.iPC_UID, player);
            client.send_packet(P_LS2CL_REP_CHAR_INFO, &pkt)
        })
}

pub fn check_char_name(client: &mut FFClient) -> FFResult<()> {
    // TODO failure
    let pkt: &sP_CL2LS_REQ_CHECK_CHAR_NAME = client.get_packet(P_CL2LS_REQ_CHECK_CHAR_NAME)?;
    let resp = sP_LS2CL_REP_CHECK_CHAR_NAME_SUCC {
        szFirstName: pkt.szFirstName,
        szLastName: pkt.szLastName,
    };
    client.send_packet(P_LS2CL_REP_CHECK_CHAR_NAME_SUCC, &resp)
}

pub fn save_char_name(client: &mut FFClient, state: &mut LoginServerState) -> FFResult<()> {
    // TODO failure
    let pkt: &sP_CL2LS_REQ_SAVE_CHAR_NAME = client.get_packet(P_CL2LS_REQ_SAVE_CHAR_NAME)?;

    let pc_uid = util::get_uid();
    // TODO check with DB if UID is in use and reroll

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

pub fn char_create(client: &mut FFClient, state: &mut LoginServerState) -> FFResult<()> {
    let pkt: &sP_CL2LS_REQ_CHAR_CREATE = client.get_packet(P_CL2LS_REQ_CHAR_CREATE)?;

    let pc_uid = pkt.PCStyle.iPC_UID;
    if let Some(player) = state.players.get_mut(&pc_uid) {
        player.style = pkt.PCStyle.try_into()?;
        player.set_item(
            ItemLocation::Equip,
            EQUIP_SLOT_UPPERBODY as usize,
            Some(Item::new(ItemType::UpperBody, pkt.sOn_Item.iEquipUBID)),
        )?;
        player.set_item(
            ItemLocation::Equip,
            EQUIP_SLOT_LOWERBODY as usize,
            Some(Item::new(ItemType::LowerBody, pkt.sOn_Item.iEquipLBID)),
        )?;
        player.set_item(
            ItemLocation::Equip,
            EQUIP_SLOT_FOOT as usize,
            Some(Item::new(ItemType::Foot, pkt.sOn_Item.iEquipFootID)),
        )?;

        let resp = sP_LS2CL_REP_CHAR_CREATE_SUCC {
            iLevel: player.get_level(),
            sPC_Style: player.get_style(),
            sPC_Style2: player.get_style_2(),
            sOn_Item: pkt.sOn_Item,
        };

        client.send_packet(P_LS2CL_REP_CHAR_CREATE_SUCC, &resp)
    } else {
        Err(FFError::build(
            Severity::Warning,
            format!("Couldn't get player {}", pc_uid),
        ))
    }
}

pub fn save_char_tutor(client: &mut FFClient, state: &mut LoginServerState) -> FFResult<()> {
    let pkt: &sP_CL2LS_REQ_SAVE_CHAR_TUTOR = client.get_packet(P_CL2LS_REQ_SAVE_CHAR_TUTOR)?;
    let pc_uid = pkt.iPC_UID;
    let player = state.players.get_mut(&pc_uid).ok_or(FFError::build(
        Severity::Warning,
        format!("Couldn't get player {}", pc_uid),
    ))?;
    if pkt.iTutorialFlag == 1 {
        player.set_tutorial_done();
        Ok(())
    } else {
        Err(FFError::build(
            Severity::Warning,
            format!("Bad iTutorialFlag value {}", pkt.iTutorialFlag),
        ))
    }
}

pub fn char_select(
    client_key: usize,
    clients: &mut HashMap<usize, FFClient>,
    state: &mut LoginServerState,
    time: SystemTime,
) -> FFResult<()> {
    let client = clients.get_mut(&client_key).unwrap();
    if let ClientType::GameClient { serial_key, .. } = client.client_type {
        let pkt: &sP_CL2LS_REQ_CHAR_SELECT = client.get_packet(P_CL2LS_REQ_CHAR_SELECT)?;
        let pc_uid = pkt.iPC_UID;
        if !state.players.contains_key(&pc_uid) {
            return Err(FFError::build(
                Severity::Warning,
                format!("Couldn't get player {}", pc_uid),
            ));
        }

        let login_info = sP_LS2FE_REQ_UPDATE_LOGIN_INFO {
            iEnterSerialKey: serial_key,
            iPC_UID: pc_uid,
            uiFEKey: client.get_fe_key_uint(),
            uiSvrTime: util::get_timestamp_ms(time),
            player: state.players.remove(&pc_uid).unwrap(),
        };

        let shard_server = clients
            .values_mut()
            .find(|c| matches!(c.client_type, ClientType::ShardServer(_)));

        match shard_server {
            Some(shard) => {
                let _ = shard.send_packet(P_LS2FE_REQ_UPDATE_LOGIN_INFO, &login_info);
                Ok(())
            }
            None => {
                log(Severity::Warning, "No shard servers available");
                let resp = sP_LS2CL_REP_CHAR_SELECT_FAIL { iErrorCode: 1 };
                let client: &mut FFClient = clients.get_mut(&client_key).unwrap();
                client.send_packet(P_LS2CL_REP_CHAR_SELECT_FAIL, &resp)
            }
        }
    } else {
        Err(FFError::build(
            Severity::Warning,
            format!("Client is not a game client ({:?})", client.client_type),
        ))
    }
}
