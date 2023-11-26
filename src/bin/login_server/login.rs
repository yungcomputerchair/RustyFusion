use super::*;

use rand::random;

use rusty_fusion::{
    defines::*,
    error::{FFError, Severity},
    net::{ffclient::ClientType, packet::*},
    placeholder, unused, util, Combatant, Entity, Item,
};

pub fn login(client: &mut FFClient, state: &mut LoginServerState) -> Result<()> {
    let pkt: &sP_CL2LS_REQ_LOGIN = client.get_packet(P_CL2LS_REQ_LOGIN);

    let mut players: Vec<Player> = Vec::new();
    let mut username = util::parse_utf16(&pkt.szID);
    let mut _password = util::parse_utf16(&pkt.szPassword);
    if username.is_empty() {
        username = util::parse_utf8(&pkt.szCookie_TEGid);
        _password = util::parse_utf8(&pkt.szCookie_authid);
    }
    if username.eq("test") {
        let mut player = Player::new(i64::MAX);
        player.set_name(1, util::encode_utf16("TestF"), util::encode_utf16("TestL"));
        player.set_appearance_flag();
        player.set_tutorial_flag();
        players.push(player);
    }

    let resp = sP_LS2CL_REP_LOGIN_SUCC {
        iCharCount: players.len() as i8,
        iSlotNum: placeholder!(1),
        iPaymentFlag: 1,
        iTempForPacking4: unused!(),
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

pub fn check_char_name(client: &mut FFClient) -> Result<()> {
    let pkt: &sP_CL2LS_REQ_CHECK_CHAR_NAME = client.get_packet(P_CL2LS_REQ_CHECK_CHAR_NAME);
    let resp = sP_LS2CL_REP_CHECK_CHAR_NAME_SUCC {
        szFirstName: pkt.szFirstName,
        szLastName: pkt.szLastName,
    };
    client.send_packet(P_LS2CL_REP_CHECK_CHAR_NAME_SUCC, &resp)
}

pub fn save_char_name(client: &mut FFClient, state: &mut LoginServerState) -> Result<()> {
    let pkt: &sP_CL2LS_REQ_SAVE_CHAR_NAME = client.get_packet(P_CL2LS_REQ_SAVE_CHAR_NAME);

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
    let pkt: &sP_CL2LS_REQ_CHAR_CREATE = client.get_packet(P_CL2LS_REQ_CHAR_CREATE);

    let pc_uid: i64 = pkt.PCStyle.iPC_UID;
    if let Some(player) = state.players.get_mut(&pc_uid) {
        player.set_style(pkt.PCStyle);
        player.set_item(
            EQUIP_SLOT_UPPERBODY as usize,
            Some(Item::new(
                EQUIP_SLOT_UPPERBODY as i16,
                pkt.sOn_Item.iEquipUBID,
            )),
        )?;
        player.set_item(
            EQUIP_SLOT_LOWERBODY as usize,
            Some(Item::new(
                EQUIP_SLOT_LOWERBODY as i16,
                pkt.sOn_Item.iEquipLBID,
            )),
        )?;
        player.set_item(
            EQUIP_SLOT_FOOT as usize,
            Some(Item::new(EQUIP_SLOT_FOOT as i16, pkt.sOn_Item.iEquipFootID)),
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

pub fn save_char_tutor(client: &mut FFClient, state: &mut LoginServerState) -> Result<()> {
    let pkt: &sP_CL2LS_REQ_SAVE_CHAR_TUTOR = client.get_packet(P_CL2LS_REQ_SAVE_CHAR_TUTOR);
    let pc_uid = pkt.iPC_UID;
    if let Some(player) = state.players.get_mut(&pc_uid) {
        if pkt.iTutorialFlag == 1 {
            player.set_tutorial_flag();
            return Ok(());
        }
    }

    Err(FFError::build(Severity::Warning, format!("TODO")))
}

pub fn char_select(
    client_key: usize,
    clients: &mut HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> Result<()> {
    let client: &mut FFClient = clients.get_mut(&client_key).unwrap();
    if let ClientType::GameClient { serial_key, .. } = client.get_client_type() {
        let pkt: &sP_CL2LS_REQ_CHAR_SELECT = client.get_packet(P_CL2LS_REQ_CHAR_SELECT);
        let pc_uid: i64 = pkt.iPC_UID;
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
            uiSvrTime: get_time(),
            player: state.players.remove(&pc_uid).unwrap(),
        };

        let shard_server = clients
            .values_mut()
            .find(|c| matches!(c.get_client_type(), ClientType::ShardServer(_)));

        match shard_server {
            Some(shard) => {
                let _ = shard.send_packet(P_LS2FE_REQ_UPDATE_LOGIN_INFO, &login_info);
                Ok(())
            }
            None => {
                // no shards available
                let resp = sP_LS2CL_REP_CHAR_SELECT_FAIL { iErrorCode: 1 };
                let client: &mut FFClient = clients.get_mut(&client_key).unwrap();
                client.send_packet(P_LS2CL_REP_CHAR_SELECT_FAIL, &resp)
            }
        }
    } else {
        Err(FFError::build(
            Severity::Warning,
            format!(
                "Client is not a game client ({:?})",
                client.get_client_type()
            ),
        ))
    }
}
