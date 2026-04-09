use std::{collections::HashMap, sync::LazyLock, time::SystemTime};

use ffmonitor::NameRequestEvent;
use rand::random;

use regex::Regex;
use rusty_fusion::{
    config::config_get,
    database::{db_get, Database as _},
    defines::*,
    entity::{Combatant, Entity, Player},
    enums::{ItemLocation, ItemType, LoginType, PlayerNameStatus},
    error::{codes::LoginError, log, log_if_failed, CatchFail as _, FFError, FFResult, Severity},
    item::Item,
    monitor::{monitor_queue, MonitorEvent},
    net::{
        crypto,
        packet::{PacketID::*, *},
        ClientMap, ClientType, FFClient,
    },
    state::LoginServerState,
    unused, util,
};

static USERNAME_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[a-zA-Z0-9_-]{4,32}").unwrap());

static PASSWORD_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[a-zA-Z0-9!@#$%^&*()_+]{8,32}").unwrap());

pub async fn login(
    pkt: Packet,
    client: &FFClient,
    state: &mut LoginServerState,
    time: SystemTime,
) -> FFResult<()> {
    const PLAINTEXT_PASSWORD_NOT_ALLOWED_MSG: &str = "Password login disabled
This server has disabled logging in with plaintext passwords.
Please contact an admin for assistance.";
    const BAD_USERNAME_REGEX_MSG: &str = "Invalid username
Username must be 4-32 characters long and contain only letters, numbers, underscores, or hyphens.";
    const BAD_PASSWORD_REGEX_MSG: &str =
        "Invalid password
Password must be 8-32 characters long and contain only letters, numbers, or special characters !@#$%^&*()_+.";

    let pkt: sP_CL2LS_REQ_LOGIN = *pkt.get(P_CL2LS_REQ_LOGIN)?;
    let mut error_code = LoginError::LoginError;
    (async {
        let login_type = LoginType::try_from(pkt.iLoginType).map_err(|_| {
            FFError::build(
                Severity::Warning,
                format!("Bad login type {}", pkt.iLoginType),
            )
        })?;

        let allow_plaintext_passwords = if cfg!(debug_assertions) {
            // plaintext passwords are disabled by default for security,
            // but dev environments are very unlikely to have an OFAPI
            // instance set up. So just allow plaintext passwords always
            // if we are in a debug build.
            true
        } else {
            config_get().login.allow_plaintext_passwords.get()
        };

        if login_type == LoginType::Password && !allow_plaintext_passwords {
            let announce = sP_FE2CL_GM_REP_PC_ANNOUNCE {
                iAnnounceType: unused!(),
                iDuringTime: 10,
                szAnnounceMsg: util::encode_utf16(PLAINTEXT_PASSWORD_NOT_ALLOWED_MSG).unwrap(),
            };
            client.send_packet(P_FE2CL_GM_REP_PC_ANNOUNCE, &announce);
            return Ok(());
        }

        let username = match login_type {
            LoginType::Password => util::parse_utf16(&pkt.szID)?,
            LoginType::Cookie => util::parse_utf8(&pkt.szCookie_TEGid)?,
        }
        .trim()
        .to_lowercase();

        if !USERNAME_REGEX.is_match(&username) {
            let announce = sP_FE2CL_GM_REP_PC_ANNOUNCE {
                iAnnounceType: unused!(),
                iDuringTime: 10,
                szAnnounceMsg: util::encode_utf16(BAD_USERNAME_REGEX_MSG).unwrap(),
            };
            client.send_packet(P_FE2CL_GM_REP_PC_ANNOUNCE, &announce);
            return Ok(());
        }

        let token = match login_type {
            LoginType::Password => {
                let password = util::parse_utf16(&pkt.szPassword)?;
                if !PASSWORD_REGEX.is_match(&password) {
                    let announce = sP_FE2CL_GM_REP_PC_ANNOUNCE {
                        iAnnounceType: unused!(),
                        iDuringTime: 10,
                        szAnnounceMsg: util::encode_utf16(BAD_PASSWORD_REGEX_MSG).unwrap(),
                    };
                    client.send_packet(P_FE2CL_GM_REP_PC_ANNOUNCE, &announce);
                    return Err(FFError::build(
                        Severity::Warning,
                        "Password did not match regex".to_string(),
                    ));
                }
                password
            }
            LoginType::Cookie => util::parse_utf8(&pkt.szCookie_authid)?,
        }
        .trim()
        .to_owned();

        let lookup_username = username.clone();
        let db = db_get();
        let account = match db.find_account_from_username(&lookup_username).await? {
            Some(account) => account,
            None => {
                if config_get().login.auto_create_accounts.get()
                    && login_type == LoginType::Password
                {
                    // automatically create the account with the supplied credentials
                    let new_username = username.clone();
                    let password_hashed = util::hash_password(&token)?;
                    let new_acc = db.create_account(&new_username, &password_hashed).await?;
                    log(
                        Severity::Info,
                        &format!(
                            "Created account {} with ID {} and level {}",
                            username, new_acc.id, new_acc.account_level
                        ),
                    );
                    new_acc
                } else {
                    error_code = LoginError::UsernameNotFound;
                    return Err(FFError::build(
                        Severity::Warning,
                        format!("Couldn't find account {}", username),
                    ));
                }
            }
        };

        // check password
        match login_type {
            LoginType::Password => {
                if !util::check_password(&token, &account.password_hashed)? {
                    error_code = LoginError::IncorrectPassword;
                    return Err(FFError::build(
                        Severity::Debug,
                        format!("Incorrect password for account {}", username),
                    ));
                }
            }
            LoginType::Cookie => {
                if account.cookie.is_none()
                    || account.cookie.as_ref().unwrap().expires < time
                    || !crypto::timing_safe_strcmp(
                        account.cookie.as_ref().unwrap().token.as_str(),
                        &token,
                    )
                {
                    error_code = LoginError::IncorrectPassword;
                    return Err(FFError::build(
                        Severity::Debug,
                        format!("Invalid or expired cookie for account {}", username),
                    ));
                }
            }
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
                szAnnounceMsg: util::encode_utf16(&ban_message)?,
            };
            client.send_packet(P_FE2CL_GM_REP_PC_ANNOUNCE, &resp);
            return Ok(());
        }

        let last_player_slot = account.selected_slot;
        let acc_id = account.id;
        let players = db.load_players(acc_id).await?;

        /*
         * Check if this account is already logged in, meaning:
         * a) the account has a session here in the login server, or
         * b) one of the account's players is tracked in a shard server
         *
         * Disabled in debug mode for convenience!
         */
        #[cfg(not(debug_assertions))]
        if state.is_session_active(account.id) {
            client.get_client_type() = ClientType::UnauthedClient {
                username: username.clone(),
                dup_pc_uid: None,
            };
        } else if let Some(dup_player) = players
            .iter()
            .find(|p| state.get_player_shard(p.get_uid()).is_some())
        {
            client.get_client_type() = ClientType::UnauthedClient {
                username: username.clone(),
                dup_pc_uid: Some(dup_player.get_uid()),
            };
        }

        if matches!(client.get_client_type(), ClientType::UnauthedClient { .. }) {
            error_code = LoginError::AlreadyLoggedIn;
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
        let fe_base: u64 = crypto::DEFAULT_KEY;
        let fe_iv1: i32 = pkt.iClientVerC;
        let fe_iv2: i32 = 1;

        let e_key = crypto::gen_key(e_base, e_iv1, e_iv2);
        let fe_key = crypto::gen_key(fe_base, fe_iv1, fe_iv2);

        client.send_packet(P_LS2CL_REP_LOGIN_SUCC, &resp);
        client.update_encryption(Some(e_key), Some(fe_key), None);

        let serial_key: i64 = random();
        client.set_client_type(ClientType::GameClient {
            account_id: account.id,
            serial_key,
            pc_id: None,
        });

        state.start_session(account, players.iter().cloned(), fe_key);

        players.iter().for_each(|player| {
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
            client.send_packet(P_LS2CL_REP_CHAR_INFO, &pkt);
        });
        Ok(())
    })
    .await
    .catch_fail(|| {
        let resp = sP_LS2CL_REP_LOGIN_FAIL {
            iErrorCode: error_code as i32,
            szID: pkt.szID,
        };
        client.send_packet(P_LS2CL_REP_LOGIN_FAIL, &resp)
    })
}

pub fn pc_exit_duplicate(
    new_key: usize,
    clients: &HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let client = clients.get(&new_key).unwrap();
    let client_type = client.get_client_type().clone();
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

        let clients = ClientMap::new(new_key, clients);
        let shard = clients.get_shard_server(shard_id).unwrap();
        let pkt = sP_LS2FE_REQ_PC_EXIT_DUPLICATE {
            iPC_UID: dup_pc_uid,
        };

        shard.send_packet(P_LS2FE_REQ_PC_EXIT_DUPLICATE, &pkt);
        Ok(())
    } else {
        // kick login server session
        for client in clients.values() {
            if matches!(client.get_client_type(), ClientType::GameClient { account_id, .. } if state.get_username(account_id)? == username)
            {
                let pkt = sP_LS2CL_REP_PC_EXIT_DUPLICATE {
                    iErrorCode: unused!(),
                };
                client.send_packet(P_LS2CL_REP_PC_EXIT_DUPLICATE, &pkt);
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

pub fn check_char_name(pkt: Packet, client: &FFClient) -> FFResult<()> {
    // TODO failure
    let pkt: &sP_CL2LS_REQ_CHECK_CHAR_NAME = pkt.get(P_CL2LS_REQ_CHECK_CHAR_NAME)?;
    let resp = sP_LS2CL_REP_CHECK_CHAR_NAME_SUCC {
        szFirstName: pkt.szFirstName,
        szLastName: pkt.szLastName,
    };
    client.send_packet(P_LS2CL_REP_CHECK_CHAR_NAME_SUCC, &resp);
    Ok(())
}

pub async fn save_char_name(
    pkt: Packet,
    client: &FFClient,
    state: &mut LoginServerState,
) -> FFResult<()> {
    // TODO failure
    let acc_id = client.get_account_id()?;
    let pkt: &sP_CL2LS_REQ_SAVE_CHAR_NAME = pkt.get(P_CL2LS_REQ_SAVE_CHAR_NAME)?;

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

    let name_check = if pkt.iFNCode != 0 {
        // name wheel name; TODO validate
        PlayerNameStatus::Approved
    } else if config_get().login.auto_approve_custom_names.get() {
        PlayerNameStatus::Approved
    } else {
        monitor_queue(MonitorEvent::NameRequest(NameRequestEvent {
            player_uid: pc_uid as u64,
            requested_name: format!("{} {}", first_name, last_name),
        }));
        PlayerNameStatus::Pending
    };

    player.first_name = first_name;
    player.last_name = last_name;
    player.flags.name_check = name_check;

    let player_saved = player.clone();
    let db = db_get();
    db.init_player(acc_id, &player_saved).await?;
    db.update_selected_player(acc_id, slot_num as i32).await?;

    let style = &player.get_style();
    let resp = sP_LS2CL_REP_SAVE_CHAR_NAME_SUCC {
        iPC_UID: pc_uid,
        iSlotNum: pkt.iSlotNum,
        iGender: style.iGender,
        szFirstName: style.szFirstName,
        szLastName: style.szLastName,
    };
    client.send_packet(P_LS2CL_REP_SAVE_CHAR_NAME_SUCC, &resp);
    state.get_players_mut(acc_id)?.insert(pc_uid, player);

    Ok(())
}

pub async fn char_create(
    pkt: Packet,
    client: &FFClient,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let acc_id = client.get_account_id()?;
    let pkt: &sP_CL2LS_REQ_CHAR_CREATE = pkt.get(P_CL2LS_REQ_CHAR_CREATE)?;

    let pc_uid = pkt.PCStyle.iPC_UID;
    if let Some(player) = state.get_players_mut(acc_id)?.get_mut(&pc_uid) {
        player.style = Some(pkt.PCStyle.try_into()?);
        let player_saved = player.clone();
        let db = db_get();
        db.update_player_appearance(&player_saved).await?;

        player
            .set_item(
                ItemLocation::Equip,
                EQUIP_SLOT_UPPERBODY as usize,
                Some(Item::new(ItemType::UpperBody, pkt.sOn_Item.iEquipUBID)),
            )
            .unwrap();
        player
            .set_item(
                ItemLocation::Equip,
                EQUIP_SLOT_LOWERBODY as usize,
                Some(Item::new(ItemType::LowerBody, pkt.sOn_Item.iEquipLBID)),
            )
            .unwrap();
        player
            .set_item(
                ItemLocation::Equip,
                EQUIP_SLOT_FOOT as usize,
                Some(Item::new(ItemType::Foot, pkt.sOn_Item.iEquipFootID)),
            )
            .unwrap();

        let player_saved = player.clone();
        db.save_player(&player_saved).await?;

        let resp = sP_LS2CL_REP_CHAR_CREATE_SUCC {
            iLevel: player.get_level(),
            sPC_Style: player.get_style(),
            sPC_Style2: player.get_style_2(),
            sOn_Item: pkt.sOn_Item,
        };

        client.send_packet(P_LS2CL_REP_CHAR_CREATE_SUCC, &resp);
        Ok(())
    } else {
        Err(FFError::build(
            Severity::Warning,
            format!("Couldn't get player {}", pc_uid),
        ))
    }
}

pub async fn char_delete(
    pkt: Packet,
    client: &FFClient,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let acc_id = client.get_account_id()?;
    let pkt: &sP_CL2LS_REQ_CHAR_DELETE = pkt.get(P_CL2LS_REQ_CHAR_DELETE)?;
    let pc_uid = pkt.iPC_UID;
    let player = state
        .get_players_mut(acc_id)?
        .remove(&pc_uid)
        .ok_or(FFError::build(
            Severity::Warning,
            format!("Couldn't get player {}", pc_uid),
        ))?;

    let db = db_get();
    db.delete_player(pc_uid).await?;
    let resp = sP_LS2CL_REP_CHAR_DELETE_SUCC {
        iSlotNum: player.get_slot_num() as i8,
    };

    client.send_packet(P_LS2CL_REP_CHAR_DELETE_SUCC, &resp);
    Ok(())
}

pub async fn save_char_tutor(
    pkt: Packet,
    client: &FFClient,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let acc_id = client.get_account_id()?;
    let pkt: &sP_CL2LS_REQ_SAVE_CHAR_TUTOR = pkt.get(P_CL2LS_REQ_SAVE_CHAR_TUTOR)?;
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
        let player_saved = player.clone();
        let db = db_get();
        db.save_player(&player_saved).await
    } else {
        Err(FFError::build(
            Severity::Warning,
            format!("Bad iTutorialFlag value {}", pkt.iTutorialFlag),
        ))
    }
}

pub async fn char_select(
    pkt: Packet,
    client_key: usize,
    clients: &HashMap<usize, FFClient>,
    state: &mut LoginServerState,
) -> FFResult<()> {
    let client = clients.get(&client_key).unwrap();
    if let ClientType::GameClient { account_id, .. } = client.get_client_type() {
        let pkt: &sP_CL2LS_REQ_CHAR_SELECT = pkt.get(P_CL2LS_REQ_CHAR_SELECT)?;
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
        let db = db_get();
        db.update_selected_player(account_id, slot_num as i32)
            .await?;

        let pkt = sP_LS2CL_REP_CHAR_SELECT_SUCC { UNUSED: unused!() };
        client.send_packet(P_LS2CL_REP_CHAR_SELECT_SUCC, &pkt);
        Ok(())
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

pub fn shard_list_info(client: &FFClient, state: &mut LoginServerState) -> FFResult<()> {
    // client is hardcoded to shard 1 for this at the time of writing
    let shard_id = 1; // pkt.iShardNum some day?
    let mut statuses = [0; MAX_NUM_CHANNELS + 1];
    statuses[0] = unused!();
    statuses[1..].copy_from_slice(&state.get_shard_channel_statuses(shard_id).map(|s| s as u8));
    let resp = sP_LS2CL_REP_SHARD_LIST_INFO_SUCC {
        aShardConnectFlag: statuses,
    };
    client.send_packet(P_LS2CL_REP_SHARD_LIST_INFO_SUCC, &resp);
    Ok(())
}

pub fn shard_select(
    pkt: Packet,
    client_key: usize,
    clients: &HashMap<usize, FFClient>,
    state: &mut LoginServerState,
    time: SystemTime,
) -> FFResult<()> {
    let client = clients.get(&client_key).unwrap();
    let pkt: sP_CL2LS_REQ_SHARD_SELECT = *pkt.get(P_CL2LS_REQ_SHARD_SELECT)?;
    let req_shard_id = pkt.ShardNum as i32;
    if let ClientType::GameClient { account_id, .. } = client.get_client_type() {
        let mut error_code = 1; // "Shard connection error"
        (|| -> FFResult<()> {
            let pc_uid = match state.get_selected_player_id(account_id)? {
                Some(uid) => uid,
                None => {
                    error_code = 2; // "Selected character error"
                    return Err(FFError::build(
                        Severity::Warning,
                        format!("No selected player for account {}", account_id),
                    ));
                }
            };

            let channel_num = state.get_pending_channel_request(pc_uid);

            let shard_id = if req_shard_id == 0 {
                None
            } else {
                Some(req_shard_id)
            };

            state.request_shard_connection(account_id, shard_id, channel_num)?;
            state.process_shard_connection_requests(clients, time);
            Ok(())
        })()
        .catch_fail(|| {
            let client = clients.get(&client_key).unwrap();
            let resp = sP_LS2CL_REP_SHARD_SELECT_FAIL {
                iErrorCode: error_code,
            };
            client.send_packet(P_LS2CL_REP_SHARD_SELECT_FAIL, &resp)
        })
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
