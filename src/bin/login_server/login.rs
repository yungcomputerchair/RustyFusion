use std::{collections::HashMap, time::SystemTime};

use rand::random;

use rusty_fusion::{
    config::config_get,
    database::db_get,
    defines::*,
    entity::{Combatant, Entity, Player},
    enums::{ItemLocation, ItemType},
    error::{catch_fail, log, log_if_failed, FFError, FFResult, Severity},
    item::Item,
    net::{
        crypto,
        packet::{PacketID::*, *},
        ClientType, FFClient,
    },
    state::LoginServerState,
    unused, util,
};

pub fn login(
    client: &mut FFClient,
    state: &mut LoginServerState,
    time: SystemTime,
) -> FFResult<()> {
    let pkt: sP_CL2LS_REQ_LOGIN = *client.get_packet(P_CL2LS_REQ_LOGIN)?;
    let mut error_code = 4; // "Login error"
    catch_fail(
        (|| {
            let username = if pkt.szID[0] != 0 {
                util::parse_utf16(&pkt.szID)?
            } else {
                util::parse_utf8(&pkt.szCookie_TEGid)?
            }
            .trim()
            .to_lowercase();

            let password = if pkt.szPassword[0] != 0 {
                util::parse_utf16(&pkt.szPassword)?
            } else {
                util::parse_utf8(&pkt.szCookie_authid)?
            }
            .trim()
            .to_owned();

            let mut db = db_get();
            let account = match db.find_account(&username)? {
                Some(account) => account,
                None => {
                    if config_get().login.auto_create_accounts.get() {
                        // automatically create the account with the supplied credentials
                        let password_hashed = util::hash_password(&password)?;
                        let new_acc = db.create_account(&username, &password_hashed)?;
                        log(
                            Severity::Info,
                            &format!(
                                "Created account {} with ID {} and level {}",
                                username, new_acc.id, new_acc.account_level
                            ),
                        );
                        new_acc
                    } else {
                        error_code = 1; // "Sorry, the ID you have entered does not exist. Please try again."
                        return Err(FFError::build(
                            Severity::Warning,
                            format!("Couldn't find account {}", username),
                        ));
                    }
                }
            };

            // check password
            if !util::check_password(&password, &account.password_hashed)? {
                error_code = 2; // "Sorry, the ID and Password you have entered do not match. Please try again."
                return Err(FFError::build(
                    Severity::Warning,
                    format!("Incorrect password for account {}", username),
                ));
            }

            // check if banned
            if account.banned_until > time {
                let ban_duration =
                    util::format_duration(account.banned_until.duration_since(time).unwrap());
                log(
                    Severity::Info,
                    &format!(
                        "Banned account {} tried to log in (banned for {})",
                        account.username, ban_duration
                    ),
                );
                let ban_message = format!(
                    "You are banned for {}.\nReason: {}",
                    ban_duration, account.ban_reason
                );
                let resp = sP_FE2CL_GM_REP_PC_ANNOUNCE {
                    iAnnounceType: unused!(),
                    iDuringTime: i32::MAX,
                    szAnnounceMsg: util::encode_utf16(&ban_message),
                };
                client.send_packet(P_FE2CL_GM_REP_PC_ANNOUNCE, &resp)?;
                return Ok(());
            }

            let last_player_slot = account.selected_slot;
            let mut players = db.load_players(account.id)?;
            for player in &mut players {
                // even if the player has a temporary name,
                // we want to show the real name in character selection
                player.flags.name_check_flag = true;
            }

            /*
             * Check if this account is already logged in, meaning:
             * a) the account has a session here in the login server, or
             * b) one of the account's players is tracked in a shard server
             *
             * Disabled in debug mode for convenience!
             */
            #[cfg(not(debug_assertions))]
            if state.is_session_active(account.id) {
                client.client_type = ClientType::UnauthedClient {
                    username: username.clone(),
                    dup_pc_uid: None,
                };
            } else if let Some(dup_player) = players
                .iter()
                .find(|p| state.get_player_shard(p.get_uid()).is_some())
            {
                client.client_type = ClientType::UnauthedClient {
                    username: username.clone(),
                    dup_pc_uid: Some(dup_player.get_uid()),
                };
            }

            if matches!(client.client_type, ClientType::UnauthedClient { .. }) {
                error_code = 3; // "ID already in use. Disconnect existing connection?"
                return Err(FFError::build(
                    Severity::Debug,
                    format!("Account {} already logged in", username),
                ));
            }

            let resp = sP_LS2CL_REP_LOGIN_SUCC {
                iCharCount: players.len() as i8,
                iSlotNum: last_player_slot as i8,
                iTempForPacking4: unused!(),
                uiSvrTime: util::get_timestamp_ms(time),
                szID: pkt.szID,
                iPaymentFlag: 1,  // all accounts have a subscription
                iOpenBetaFlag: 0, // and we're not in open beta
            };
            let e_base: u64 = resp.uiSvrTime;
            let e_iv1: i32 = (resp.iCharCount + 1) as i32;
            let e_iv2: i32 = (resp.iSlotNum + 1) as i32;
            let fe_base: u64 = u64::from_le_bytes(crypto::DEFAULT_KEY.try_into().unwrap());
            let fe_iv1: i32 = pkt.iClientVerC;
            let fe_iv2: i32 = 1;

            client.send_packet(P_LS2CL_REP_LOGIN_SUCC, &resp)?;

            client.e_key = crypto::gen_key(e_base, e_iv1, e_iv2);
            client.fe_key = crypto::gen_key(fe_base, fe_iv1, fe_iv2);

            let serial_key: i64 = random();
            client.client_type = ClientType::GameClient {
                account_id: account.id,
                serial_key,
                pc_id: None,
            };
            state.start_session(account, players.clone().iter().cloned());

            players.iter().try_for_each(|player| {
                let pos = player.get_position();
                let pkt = sP_LS2CL_REP_CHAR_INFO {
                    iSlot: player.get_slot_num() as i8,
                    iLevel: player.get_level(),
                    sPC_Style: player.get_style(),
                    sPC_Style2: player.get_style_2(),
                    iX: pos.x,
                    iY: pos.y,
                    iZ: pos.z,
                    aEquip: player.get_equipped().map(Option::<Item>::into),
                };
                client.send_packet(P_LS2CL_REP_CHAR_INFO, &pkt)
            })
        })(),
        || {
            let resp = sP_LS2CL_REP_LOGIN_FAIL {
                iErrorCode: error_code,
                szID: pkt.szID,
            };
            client.send_packet(P_LS2CL_REP_LOGIN_FAIL, &resp)
        },
    )
}

pub fn pc_exit_duplicate(
    new_key: usize,
    clients: &mut HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let client = clients.get_mut(&new_key).unwrap();
    let client_type = client.client_type.clone();
    let ClientType::UnauthedClient {
        username,
        dup_pc_uid,
    } = client_type
    else {
        return Err(FFError::build(
            Severity::Warning,
            "Client is not an unauthed client".to_string(),
        ));
    };

    if let Some(dup_pc_uid) = dup_pc_uid {
        // find the shard server that the duplicate player is on
        let shard_id = state.get_player_shard(dup_pc_uid).ok_or(FFError::build(
            Severity::Warning,
            format!("Couldn't find shard server for player {}", dup_pc_uid),
        ))?;
        let shard = clients
            .values_mut()
            .find(|c| matches!(c.client_type, ClientType::ShardServer(sid) if sid == shard_id))
            .unwrap();
        let pkt = sP_LS2FE_REQ_PC_EXIT_DUPLICATE {
            iPC_UID: dup_pc_uid,
        };
        log_if_failed(shard.send_packet(P_LS2FE_REQ_PC_EXIT_DUPLICATE, &pkt));
        Ok(())
    } else {
        // kick login server session
        for client in clients.values_mut() {
            if matches!(client.client_type, ClientType::GameClient { account_id, .. } if state.get_username(account_id)? == username)
            {
                let pkt = sP_LS2CL_REP_PC_EXIT_DUPLICATE {
                    iErrorCode: unused!(),
                };
                log_if_failed(client.send_packet(P_LS2CL_REP_PC_EXIT_DUPLICATE, &pkt));
                client.disconnect();
                return Ok(());
            }
        }
        Err(FFError::build(
            Severity::Warning,
            format!(
                "Couldn't find client with login session for account {}",
                username
            ),
        ))
    }
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
    let acc_id = client.get_account_id()?;
    let pkt: &sP_CL2LS_REQ_SAVE_CHAR_NAME = client.get_packet(P_CL2LS_REQ_SAVE_CHAR_NAME)?;

    let pc_uid = util::get_uid();
    let slot_num = pkt.iSlotNum as usize;
    if !(1..=4).contains(&slot_num) {
        return Err(FFError::build(
            Severity::Warning,
            format!("Bad slot number {}", slot_num),
        ));
    }

    let mut player = Player::new(pc_uid, slot_num);
    let first_name = util::parse_utf16(&pkt.szFirstName)?;
    let last_name = util::parse_utf16(&pkt.szLastName)?;
    player.first_name = first_name;
    player.last_name = last_name;
    player.flags.name_check_flag = true; // TODO check name + config

    let mut db = db_get();
    db.init_player(acc_id, &player)?;
    db.update_selected_player(acc_id, slot_num as i32)?;

    let style = &player.get_style();
    let resp = sP_LS2CL_REP_SAVE_CHAR_NAME_SUCC {
        iPC_UID: pc_uid,
        iSlotNum: pkt.iSlotNum,
        iGender: style.iGender,
        szFirstName: style.szFirstName,
        szLastName: style.szLastName,
    };
    client.send_packet(P_LS2CL_REP_SAVE_CHAR_NAME_SUCC, &resp)?;
    state.get_players_mut(acc_id)?.insert(pc_uid, player);

    Ok(())
}

pub fn char_create(client: &mut FFClient, state: &mut LoginServerState) -> FFResult<()> {
    let acc_id = client.get_account_id()?;
    let pkt: &sP_CL2LS_REQ_CHAR_CREATE = client.get_packet(P_CL2LS_REQ_CHAR_CREATE)?;

    let pc_uid = pkt.PCStyle.iPC_UID;
    if let Some(player) = state.get_players_mut(acc_id)?.get_mut(&pc_uid) {
        player.style = Some(pkt.PCStyle.try_into()?);
        let mut db = db_get();
        db.update_player_appearance(player)?;

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
        db.save_player(player, None)?;

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

pub fn char_delete(client: &mut FFClient, state: &mut LoginServerState) -> FFResult<()> {
    let acc_id = client.get_account_id()?;
    let pkt: &sP_CL2LS_REQ_CHAR_DELETE = client.get_packet(P_CL2LS_REQ_CHAR_DELETE)?;
    let pc_uid = pkt.iPC_UID;
    let player = state
        .get_players_mut(acc_id)?
        .remove(&pc_uid)
        .ok_or(FFError::build(
            Severity::Warning,
            format!("Couldn't get player {}", pc_uid),
        ))?;
    let mut db = db_get();
    db.delete_player(pc_uid)?;
    let resp = sP_LS2CL_REP_CHAR_DELETE_SUCC {
        iSlotNum: player.get_slot_num() as i8,
    };
    client.send_packet(P_LS2CL_REP_CHAR_DELETE_SUCC, &resp)
}

pub fn save_char_tutor(client: &mut FFClient, state: &mut LoginServerState) -> FFResult<()> {
    let acc_id = client.get_account_id()?;
    let pkt: &sP_CL2LS_REQ_SAVE_CHAR_TUTOR = client.get_packet(P_CL2LS_REQ_SAVE_CHAR_TUTOR)?;
    let pc_uid = pkt.iPC_UID;
    let player = state
        .get_players_mut(acc_id)?
        .get_mut(&pc_uid)
        .ok_or(FFError::build(
            Severity::Warning,
            format!("Couldn't get player {}", pc_uid),
        ))?;
    if pkt.iTutorialFlag == 1 {
        player.set_tutorial_done();
        let mut db = db_get();
        db.save_player(player, None)
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
) -> FFResult<()> {
    let client = clients.get_mut(&client_key).unwrap();
    if let ClientType::GameClient { account_id, .. } = client.client_type {
        let pkt: &sP_CL2LS_REQ_CHAR_SELECT = client.get_packet(P_CL2LS_REQ_CHAR_SELECT)?;
        let pc_uid = pkt.iPC_UID;
        let players = state.get_players_mut(account_id)?;
        let player = players.get(&pc_uid).ok_or(FFError::build(
            Severity::Warning,
            format!("Couldn't get player {}", pc_uid),
        ))?;
        let slot_num = player.get_slot_num();

        if !player.flags.tutorial_flag {
            return Err(FFError::build(
                Severity::Warning,
                format!("Player {} hasn't completed the tutorial", pc_uid),
            ));
        }

        log_if_failed(state.set_selected_player_id(account_id, pc_uid));

        let mut db = db_get();
        db.update_selected_player(account_id, slot_num as i32)?;

        let pkt = sP_LS2CL_REP_CHAR_SELECT_SUCC { UNUSED: unused!() };
        client.send_packet(P_LS2CL_REP_CHAR_SELECT_SUCC, &pkt)
    } else {
        Err(FFError::build(
            Severity::Warning,
            format!("Client is not a game client ({:?})", client.client_type),
        ))
    }
}

pub fn shard_list_info(client: &mut FFClient, state: &mut LoginServerState) -> FFResult<()> {
    // client is hardcoded to shard 1 for this at the time of writing
    let shard_id = 1; // pkt.iShardNum some day?
    let mut statuses = [0; MAX_NUM_CHANNELS + 1];
    statuses[0] = unused!();
    statuses[1..].copy_from_slice(&state.get_shard_channel_statuses(shard_id).map(|s| s as u8));
    let resp = sP_LS2CL_REP_SHARD_LIST_INFO_SUCC {
        aShardConnectFlag: statuses,
    };
    client.send_packet(P_LS2CL_REP_SHARD_LIST_INFO_SUCC, &resp)
}

pub fn shard_select(
    client_key: usize,
    clients: &mut HashMap<usize, FFClient>,
    state: &mut LoginServerState,
    time: SystemTime,
) -> FFResult<()> {
    let client = clients.get_mut(&client_key).unwrap();
    let pkt: sP_CL2LS_REQ_SHARD_SELECT = *client.get_packet(P_CL2LS_REQ_SHARD_SELECT)?;
    let req_shard_id = pkt.ShardNum as i32;
    if let ClientType::GameClient {
        account_id,
        serial_key,
        ..
    } = client.client_type
    {
        let mut error_code = -1;
        catch_fail(
            (|| {
                let client = clients.get_mut(&client_key).unwrap();
                let fe_key = client.get_fe_key_uint();
                let pc_uid = match state.get_selected_player_id(account_id)? {
                    Some(pc_uid) => pc_uid,
                    None => {
                        error_code = 2; // "Selected character error"
                        return Err(FFError::build(
                            Severity::Warning,
                            format!("No selected player for account {}", account_id),
                        ));
                    }
                };

                let shard_id = if req_shard_id == 0 {
                    // pick the shard with the lowest population
                    match state.get_lowest_pop_shard_id() {
                        Some(shard_id) => shard_id,
                        None => {
                            error_code = 1; // "Shard connection error"
                            return Err(FFError::build(
                                Severity::Warning,
                                "No shard servers available".to_string(),
                            ));
                        }
                    }
                } else {
                    req_shard_id
                };

                let shard_server = match clients.values_mut().find(
                    |c| matches!(c.client_type, ClientType::ShardServer(sid) if sid == shard_id),
                ) {
                    Some(shard) => shard,
                    None => {
                        error_code = 0; // "Shard number error"
                        return Err(FFError::build(
                            Severity::Warning,
                            format!("Couldn't find shard server with ID {}", shard_id),
                        ));
                    }
                };

                let login_info = sP_LS2FE_REQ_UPDATE_LOGIN_INFO {
                    iAccountID: account_id,
                    iEnterSerialKey: serial_key,
                    iPC_UID: pc_uid,
                    uiFEKey: fe_key,
                    uiSvrTime: util::get_timestamp_ms(time),
                };
                if shard_server
                    .send_packet(P_LS2FE_REQ_UPDATE_LOGIN_INFO, &login_info)
                    .is_err()
                {
                    error_code = 1; // "Shard connection error"
                    return Err(FFError::build(
                        Severity::Warning,
                        format!("Couldn't send login info to shard server {}", req_shard_id),
                    ));
                }

                Ok(())
            })(),
            || {
                let client = clients.get_mut(&client_key).unwrap();
                let resp = sP_LS2CL_REP_SHARD_SELECT_FAIL {
                    iErrorCode: error_code,
                };
                client.send_packet(P_LS2CL_REP_SHARD_SELECT_FAIL, &resp)
            },
        )
    } else {
        Err(FFError::build(
            Severity::Warning,
            format!("Client is not a game client ({:?})", client.client_type),
        ))
    }
}
