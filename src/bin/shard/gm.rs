use std::{
    cmp::max,
    time::{Duration, SystemTime},
};

use rusty_fusion::{
    chunk::{EntityMap, InstanceID, TickMode},
    defines::*,
    entity::{Combatant, Egg, Entity, EntityID, PlayerSearchQuery, NPC},
    enums::*,
    error::*,
    item::Item,
    net::{
        packet::{PacketID::*, *},
        ClientMap, FFClient,
    },
    placeholder,
    state::ShardServerState,
    tabledata::tdata_get,
    unused, util, Position,
};

pub fn gm_pc_set_value(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    helpers::validate_perms(client, state, CN_ACCOUNT_LEVEL__DEVELOPER as i16)?;

    let pkt: sP_CL2FE_GM_REQ_PC_SET_VALUE = *client.get_packet(P_CL2FE_GM_REQ_PC_SET_VALUE)?;
    let pc_id = pkt.iPC_ID;
    let value = pkt.iSetValue;
    let value_type = pkt.iSetValueType;
    let player = state.get_player_mut(pc_id)?;

    let value = match value_type as u32 {
        CN_GM_SET_VALUE_TYPE__HP => player.set_hp(value),
        CN_GM_SET_VALUE_TYPE__WEAPON_BATTERY => player.set_weapon_boosts(value as u32) as i32,
        CN_GM_SET_VALUE_TYPE__NANO_BATTERY => player.set_nano_potions(value as u32) as i32,
        CN_GM_SET_VALUE_TYPE__FUSION_MATTER => {
            player.set_fusion_matter(value as u32, Some(clients)) as i32
        }
        CN_GM_SET_VALUE_TYPE__CANDY => player.set_taros(value as u32) as i32,
        CN_GM_SET_VALUE_TYPE__SPEED => placeholder!(value),
        CN_GM_SET_VALUE_TYPE__JUMP => placeholder!(value),
        _ => {
            return Err(FFError::build(
                Severity::Warning,
                format!("Bad value type: {}", value_type),
            ));
        }
    };

    let resp = sP_FE2CL_GM_REP_PC_SET_VALUE {
        iPC_ID: pkt.iPC_ID,
        iSetValue: value,
        iSetValueType: value_type,
    };
    clients
        .get_self()
        .send_packet(P_FE2CL_GM_REP_PC_SET_VALUE, &resp)
}

pub fn gm_pc_give_item(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    catch_fail(
        (|| {
            let pc_id = helpers::validate_perms(client, state, CN_ACCOUNT_LEVEL__DEVELOPER as i16)?;
            let pkt: &sP_CL2FE_REQ_PC_GIVE_ITEM = client.get_packet(P_CL2FE_REQ_PC_GIVE_ITEM)?;
            let player = state.get_player_mut(pc_id)?;

            let mut item: Option<Item> = pkt.Item.try_into()?;
            let time = pkt.iTimeLeft as u32;
            if time > 0 && item.is_some() {
                let duration = Duration::from_secs(time as u64);
                let expiry_time = SystemTime::now() + duration;
                item.as_mut().unwrap().set_expiry_time(expiry_time);
            }

            let location = pkt.eIL.try_into()?;
            let slot_number = match location {
                ItemLocation::QInven => {
                    let qitem_id = pkt.Item.iID;
                    let qitem_count = pkt.Item.iOpt as usize;
                    player.set_quest_item_count(qitem_id, qitem_count)?
                }
                other => {
                    let req_slot_number = pkt.iSlotNum as usize;
                    player.set_item(other, req_slot_number, item)?;
                    req_slot_number
                }
            };

            let resp = sP_FE2CL_REP_PC_GIVE_ITEM_SUCC {
                eIL: pkt.eIL,
                iSlotNum: slot_number as i32,
                Item: item.into(),
            };
            client.send_packet(P_FE2CL_REP_PC_GIVE_ITEM_SUCC, &resp)
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_GIVE_ITEM_FAIL {
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_PC_GIVE_ITEM_FAIL, &resp)
        },
    )
}

pub fn gm_pc_give_nano(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_GIVE_NANO =
        *clients.get_self().get_packet(P_CL2FE_REQ_PC_GIVE_NANO)?;
    catch_fail(
        (|| {
            let client = clients.get_self();
            let pc_id = helpers::validate_perms(client, state, CN_ACCOUNT_LEVEL__DEVELOPER as i16)?;
            let nano_id = pkt.iNanoID;
            let player = state.get_player_mut(pc_id)?;
            let new_level = max(player.get_level(), nano_id);
            player.set_level(new_level)?;
            let nano = player.unlock_nano(nano_id)?.clone();

            let resp = sP_FE2CL_REP_PC_NANO_CREATE_SUCC {
                iPC_FusionMatter: player.get_fusion_matter() as i32,
                iQuestItemSlotNum: -1,
                QuestItem: None.into(),
                Nano: Some(nano).into(),
                iPC_Level: new_level,
            };

            log_if_failed(
                clients
                    .get_self()
                    .send_packet(P_FE2CL_REP_PC_NANO_CREATE_SUCC, &resp),
            );

            let bcast = sP_FE2CL_REP_PC_CHANGE_LEVEL {
                iPC_ID: pc_id,
                iPC_Level: new_level,
            };
            state
                .entity_map
                .for_each_around(EntityID::Player(pc_id), clients, |c| {
                    c.send_packet(P_FE2CL_REP_PC_CHANGE_LEVEL, &bcast)
                });
            Ok(())
        })(),
        || {
            let client = clients.get_self();
            let resp = sP_FE2CL_REP_PC_NANO_CREATE_FAIL {
                iPC_ID: client.get_player_id()?,
                iErrorCode: unused!(),
            };
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_NANO_CREATE_FAIL, &resp)
        },
    )
}

pub fn gm_pc_goto(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = helpers::validate_perms(client, state, CN_ACCOUNT_LEVEL__DEVELOPER as i16)?;
    let pkt: &sP_CL2FE_REQ_PC_GOTO = client.get_packet(P_CL2FE_REQ_PC_GOTO)?;
    let new_pos = Position {
        x: pkt.iToX,
        y: pkt.iToY,
        z: pkt.iToZ,
    };
    let player = state.get_player_mut(pc_id)?;
    player.set_position(new_pos);
    player.instance_id = InstanceID::default();
    let taros = player.get_taros();

    state
        .entity_map
        .update(EntityID::Player(pc_id), None, Some(clients));

    // sP_FE2CL_REP_PC_GOTO_SUCC doesn't reset the clientside instance state,
    // but we need that to happen so we use the NPC warp packet instead
    let resp = sP_FE2CL_REP_PC_WARP_USE_NPC_SUCC {
        iX: new_pos.x,
        iY: new_pos.y,
        iZ: new_pos.z,
        eIL: ItemLocation::end(),
        iItemSlotNum: unused!(),
        Item: unused!(),
        iCandy: taros as i32,
    };
    clients
        .get_self()
        .send_packet(P_FE2CL_REP_PC_WARP_USE_NPC_SUCC, &resp)
}

pub fn gm_pc_special_state_switch(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = helpers::validate_perms(client, state, CN_ACCOUNT_LEVEL__GM as i16)?;
    let pkt: &sP_CL2FE_GM_REQ_PC_SPECIAL_STATE_SWITCH =
        client.get_packet(P_CL2FE_GM_REQ_PC_SPECIAL_STATE_SWITCH)?;

    let player = state.get_player_mut(pc_id)?;

    match pkt.iSpecialStateFlag as u32 {
        CN_SPECIAL_STATE_FLAG__PRINT_GM => {
            player.show_gm_marker = !player.show_gm_marker;
        }
        CN_SPECIAL_STATE_FLAG__INVISIBLE => {
            player.invisible = !player.invisible;
        }
        CN_SPECIAL_STATE_FLAG__INVULNERABLE => {
            player.invulnerable = !player.invulnerable;
        }
        _ => {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "P_CL2FE_GM_REQ_PC_SPECIAL_STATE_SWITCH: invalid special state flag: {}",
                    pkt.iSpecialStateFlag
                ),
            ));
        }
    }

    let special_state_flags = player.get_special_state_bit_flag();

    let resp = sP_FE2CL_REP_PC_SPECIAL_STATE_SWITCH_SUCC {
        iPC_ID: pkt.iPC_ID,
        iReqSpecialStateFlag: pkt.iSpecialStateFlag,
        iSpecialState: special_state_flags,
    };
    state
        .entity_map
        .for_each_around(EntityID::Player(pkt.iPC_ID), clients, |c| {
            c.send_packet(P_FE2CL_PC_SPECIAL_STATE_CHANGE, &resp)
        });
    clients
        .get_self()
        .send_packet(P_FE2CL_REP_PC_SPECIAL_STATE_SWITCH_SUCC, &resp)
}

pub fn gm_pc_motd_register(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    helpers::validate_perms(client, state, CN_ACCOUNT_LEVEL__CS as i16)?;
    let pkt: &sP_CL2FE_GM_REQ_PC_MOTD_REGISTER =
        client.get_packet(P_CL2FE_GM_REQ_PC_MOTD_REGISTER)?;
    let pkt = sP_FE2LS_MOTD_REGISTER {
        szMessage: pkt.szSystemMsg,
    };
    if let Some(login_server) = clients.get_login_server() {
        login_server.send_packet(P_FE2LS_MOTD_REGISTER, &pkt)
    } else {
        Ok(())
    }
}

pub fn gm_pc_announce(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = helpers::validate_perms(client, state, CN_ACCOUNT_LEVEL__CS as i16)?;
    let pkt: &sP_CL2FE_GM_REQ_PC_ANNOUNCE = client.get_packet(P_CL2FE_GM_REQ_PC_ANNOUNCE)?;
    let area_type: AreaType = pkt.iAreaType.try_into()?;
    let pkt = sP_FE2CL_ANNOUNCE_MSG {
        iAnnounceType: pkt.iAnnounceType,
        iDuringTime: pkt.iDuringTime,
        szAnnounceMsg: pkt.szAnnounceMsg,
    };
    let player = state.get_player(pc_id).unwrap();
    match area_type {
        AreaType::Local => {
            state
                .entity_map
                .for_each_around(EntityID::Player(pc_id), clients, |c| {
                    c.send_packet(P_FE2CL_ANNOUNCE_MSG, &pkt)
                });
        }
        AreaType::Channel => state
            .entity_map
            .find_players(|p| p.instance_id.channel_num == player.instance_id.channel_num)
            .iter()
            .for_each(|pc_id| {
                let player = state.get_player(*pc_id).unwrap();
                let client = player.get_client(clients).unwrap();
                log_if_failed(client.send_packet(P_FE2CL_ANNOUNCE_MSG, &pkt));
            }),
        AreaType::Shard => state
            .entity_map
            .find_players(|_| true)
            .iter()
            .for_each(|pc_id| {
                let player = state.get_player(*pc_id).unwrap();
                let client = player.get_client(clients).unwrap();
                log_if_failed(client.send_packet(P_FE2CL_ANNOUNCE_MSG, &pkt));
            }),
        AreaType::Global => {
            if let Some(login_server) = clients.get_login_server() {
                log_if_failed(login_server.send_packet(P_FE2LS_ANNOUNCE_MSG, &pkt));
            }
        }
    }
    Ok(())
}

pub fn gm_pc_location(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let gm_pc_id = helpers::validate_perms(client, state, CN_ACCOUNT_LEVEL__CS as i16)?;
    let pkt: sP_CL2FE_GM_REQ_PC_LOCATION = *client.get_packet(P_CL2FE_GM_REQ_PC_LOCATION)?;
    let search_mode: TargetSearchBy = pkt.eTargetSearchBy.try_into()?;
    let search_query = match search_mode {
        TargetSearchBy::PlayerID => PlayerSearchQuery::ByID(pkt.iTargetPC_ID),
        TargetSearchBy::PlayerName => PlayerSearchQuery::ByName(
            util::parse_utf16(&pkt.szTargetPC_FirstName)?,
            util::parse_utf16(&pkt.szTargetPC_LastName)?,
        ),
        TargetSearchBy::PlayerUID => PlayerSearchQuery::ByUID(pkt.iTargetPC_UID),
    };
    if let Some(pc_id) = search_query.execute(state) {
        let player = state.get_player(pc_id)?;
        let pos = player.get_position();
        let resp = sP_FE2CL_GM_REP_PC_LOCATION {
            iTargetPC_UID: player.get_uid(),
            iTargetPC_ID: pc_id,
            iShardID: state.shard_id,
            iMapType: if player.instance_id.instance_num.is_some() {
                1 // instance
            } else {
                0 // non-instance
            },
            iMapID: player.instance_id.instance_num.unwrap_or(0) as i32,
            iMapNum: player.instance_id.map_num as i32,
            iX: pos.x,
            iY: pos.y,
            iZ: pos.z,
            szTargetPC_FirstName: util::encode_utf16(&player.first_name),
            szTargetPC_LastName: util::encode_utf16(&player.last_name),
        };
        clients
            .get_self()
            .send_packet(P_FE2CL_GM_REP_PC_LOCATION, &resp)
    } else if search_mode != TargetSearchBy::PlayerID && clients.get_login_server().is_some() {
        // for name or UID search, we can ask the login server,
        // which will ask all the other shards
        let pkt = sP_FE2LS_REQ_PC_LOCATION {
            iPC_ID: gm_pc_id,
            sReq: pkt,
        };
        let login_server = clients.get_login_server().unwrap();
        log_if_failed(login_server.send_packet(P_FE2LS_REQ_PC_LOCATION, &pkt));
        Ok(())
    } else {
        Err(helpers::send_search_fail(clients.get_self(), search_query))
    }
}

pub fn gm_target_pc_special_state_onoff(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let client = clients.get_self();
    helpers::validate_perms(client, state, CN_ACCOUNT_LEVEL__CS as i16)?;
    let pkt: sP_CL2FE_GM_REQ_TARGET_PC_SPECIAL_STATE_ONOFF =
        *client.get_packet(P_CL2FE_GM_REQ_TARGET_PC_SPECIAL_STATE_ONOFF)?;

    let search_mode: TargetSearchBy = pkt.eTargetSearchBy.try_into()?;
    let search_query = match search_mode {
        TargetSearchBy::PlayerID => PlayerSearchQuery::ByID(pkt.iTargetPC_ID),
        TargetSearchBy::PlayerName => PlayerSearchQuery::ByName(
            util::parse_utf16(&pkt.szTargetPC_FirstName)?,
            util::parse_utf16(&pkt.szTargetPC_LastName)?,
        ),
        TargetSearchBy::PlayerUID => PlayerSearchQuery::ByUID(pkt.iTargetPC_UID),
    };
    let pc_id = search_query
        .execute(state)
        .ok_or_else(|| helpers::send_search_fail(client, search_query))?;
    let player = state.get_player_mut(pc_id)?;

    let new_flag = pkt.iONOFF != 0;
    match pkt.iSpecialStateFlag as u32 {
        // this packet is only used for /mute
        CN_SPECIAL_STATE_FLAG__MUTE_FREECHAT => {
            player.freechat_muted = new_flag;
        }
        _ => {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "P_CL2FE_GM_REQ_TARGET_PC_SPECIAL_STATE_ONOFF: invalid special state flag: {}",
                    pkt.iSpecialStateFlag
                ),
            ));
        }
    }

    let special_state_flags = player.get_special_state_bit_flag();

    let resp = sP_FE2CL_REP_PC_SPECIAL_STATE_SWITCH_SUCC {
        iPC_ID: pc_id,
        iReqSpecialStateFlag: pkt.iSpecialStateFlag,
        iSpecialState: special_state_flags,
    };
    state
        .entity_map
        .for_each_around(EntityID::Player(pc_id), clients, |c| {
            c.send_packet(P_FE2CL_PC_SPECIAL_STATE_CHANGE, &resp)
        });
    clients
        .get_self()
        .send_packet(P_FE2CL_REP_PC_SPECIAL_STATE_SWITCH_SUCC, &resp)
}

pub fn gm_target_pc_teleport(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let client = clients.get_self();
    let gm_pc_id = helpers::validate_perms(client, state, CN_ACCOUNT_LEVEL__CS as i16)?;
    let pkt: sP_CL2FE_GM_REQ_TARGET_PC_TELEPORT =
        *client.get_packet(P_CL2FE_GM_REQ_TARGET_PC_TELEPORT)?;

    // the "target PC" is the player being teleported
    let search_mode: TargetSearchBy = pkt.eTargetPCSearchBy.try_into()?;
    let search_query = match search_mode {
        TargetSearchBy::PlayerID => PlayerSearchQuery::ByID(pkt.iTargetPC_ID),
        TargetSearchBy::PlayerName => PlayerSearchQuery::ByName(
            util::parse_utf16(&pkt.szTargetPC_FirstName)?,
            util::parse_utf16(&pkt.szTargetPC_LastName)?,
        ),
        TargetSearchBy::PlayerUID => PlayerSearchQuery::ByUID(pkt.iTargetPC_UID),
    };
    let target_pc_id = search_query
        .execute(state)
        .ok_or_else(|| helpers::send_search_fail(client, search_query))?;
    let target_player = state.get_player(target_pc_id).unwrap();
    let teleport_type: TeleportType = pkt.eTeleportType.try_into()?;
    let (dest_pos, dest_inst_id) = match teleport_type {
        TeleportType::XYZ => (
            Position {
                x: pkt.iToX,
                y: pkt.iToY,
                z: pkt.iToZ,
            },
            target_player.instance_id,
        ),
        TeleportType::MapXYZ => (
            Position {
                x: pkt.iToX,
                y: pkt.iToY,
                z: pkt.iToZ,
            },
            InstanceID {
                // player needs to be in the same map as the instance they want to teleport to
                instance_num: if pkt.iToMap == 0 {
                    None
                } else {
                    Some(pkt.iToMap as u32)
                },
                ..target_player.instance_id
            },
        ),
        TeleportType::MyLocation => {
            let my_player = state.get_player(gm_pc_id).unwrap();
            (my_player.get_position(), my_player.instance_id)
        }
        TeleportType::SomeoneLocation => {
            // the "goal PC" is the player being teleported to
            let search_mode: TargetSearchBy = pkt.eGoalPCSearchBy.try_into()?;
            let search_query = match search_mode {
                TargetSearchBy::PlayerID => PlayerSearchQuery::ByID(pkt.iGoalPC_ID),
                TargetSearchBy::PlayerName => PlayerSearchQuery::ByName(
                    util::parse_utf16(&pkt.szGoalPC_FirstName)?,
                    util::parse_utf16(&pkt.szGoalPC_LastName)?,
                ),
                TargetSearchBy::PlayerUID => PlayerSearchQuery::ByUID(pkt.iGoalPC_UID),
            };
            let goal_pc_id = search_query
                .execute(state)
                .ok_or_else(|| helpers::send_search_fail(client, search_query))?;
            let goal_player = state.get_player(goal_pc_id).unwrap();
            (goal_player.get_position(), goal_player.instance_id)
        }
        TeleportType::Unstick => (
            target_player.get_position().get_unstuck(),
            target_player.instance_id,
        ),
    };

    let player = state.get_player_mut(target_pc_id).unwrap();
    player.set_pre_warp();
    player.set_position(dest_pos);
    player.instance_id = dest_inst_id;

    let resp = sP_FE2CL_REP_PC_WARP_USE_NPC_SUCC {
        iX: dest_pos.x,
        iY: dest_pos.y,
        iZ: dest_pos.z,
        eIL: ItemLocation::end(),
        iItemSlotNum: unused!(),
        Item: unused!(),
        iCandy: player.get_taros() as i32,
    };
    let client = player.get_client(clients).unwrap();
    log_if_failed(client.send_packet(P_FE2CL_REP_PC_WARP_USE_NPC_SUCC, &resp));

    // see transport::helpers::do_warp to see why we use None for the chunk here
    state
        .entity_map
        .update(EntityID::Player(target_pc_id), None, Some(clients));
    Ok(())
}

pub fn gm_kick_player(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    helpers::validate_perms(client, state, CN_ACCOUNT_LEVEL__CS as i16)?;
    let pkt: sP_CL2FE_GM_REQ_KICK_PLAYER = *client.get_packet(P_CL2FE_GM_REQ_KICK_PLAYER)?;
    let search_mode: TargetSearchBy = pkt.eTargetSearchBy.try_into()?;
    let search_query = match search_mode {
        TargetSearchBy::PlayerID => PlayerSearchQuery::ByID(pkt.iTargetPC_ID),
        TargetSearchBy::PlayerName => PlayerSearchQuery::ByName(
            util::parse_utf16(&pkt.szTargetPC_FirstName)?,
            util::parse_utf16(&pkt.szTargetPC_LastName)?,
        ),
        TargetSearchBy::PlayerUID => PlayerSearchQuery::ByUID(pkt.iTargetPC_UID),
    };
    let pc_id = search_query
        .execute(state)
        .ok_or_else(|| helpers::send_search_fail(clients.get_self(), search_query))?;
    let client = state
        .get_player(pc_id)
        .unwrap()
        .get_client(clients)
        .unwrap();
    let pkt = sP_FE2CL_REP_PC_EXIT_SUCC {
        iID: pc_id,
        iExitCode: EXIT_CODE_REQ_BY_GM as i32,
    };
    log_if_failed(client.send_packet(P_FE2CL_REP_PC_EXIT_SUCC, &pkt));
    client.disconnect();
    Ok(())
}

pub fn gm_reward_rate(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pc_id = helpers::validate_perms(client, state, CN_ACCOUNT_LEVEL__DEVELOPER as i16)?;
    let pkt: &sP_CL2FE_GM_REQ_REWARD_RATE = client.get_packet(P_CL2FE_GM_REQ_REWARD_RATE)?;
    let player = state.get_player_mut(pc_id)?;

    if pkt.iGetSet != 0 {
        let reward_type: RewardType = pkt.iRewardType.try_into()?;
        let rate_percent = pkt.iSetRateValue as f32;
        let category: RewardCategory = (pkt.iRewardRateIndex as usize).try_into()?;
        player
            .reward_data
            .set_reward_rate(reward_type, category, rate_percent);
    }

    let resp = sP_FE2CL_GM_REP_REWARD_RATE_SUCC {
        afRewardRate_Taros: player.reward_data.get_rates_as_array(RewardType::Taros),
        afRewardRate_FusionMatter: player
            .reward_data
            .get_rates_as_array(RewardType::FusionMatter),
    };
    client.send_packet(P_FE2CL_GM_REP_REWARD_RATE_SUCC, &resp)
}

pub fn gm_pc_task_complete(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pc_id = helpers::validate_perms(client, state, CN_ACCOUNT_LEVEL__DEVELOPER as i16)?;
    let pkt: &sP_CL2FE_REQ_PC_TASK_COMPLETE = client.get_packet(P_CL2FE_REQ_PC_TASK_COMPLETE)?;
    let player = state.get_player_mut(pc_id)?;
    let task_id = pkt.iTaskNum;
    player.mission_journal.complete_task(task_id)?;
    let resp = sP_FE2CL_REP_PC_TASK_END_SUCC { iTaskNum: task_id };
    client.send_packet(P_FE2CL_REP_PC_TASK_END_SUCC, &resp)
}

pub fn gm_pc_mission_complete(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pc_id = helpers::validate_perms(client, state, CN_ACCOUNT_LEVEL__DEVELOPER as i16)?;
    let pkt: &sP_CL2FE_REQ_PC_MISSION_COMPLETE =
        client.get_packet(P_CL2FE_REQ_PC_MISSION_COMPLETE)?;
    let player = state.get_player_mut(pc_id)?;
    let mission_id = pkt.iMissionNum;
    player.mission_journal.set_mission_completed(mission_id)?;
    let resp = sP_FE2CL_REP_PC_MISSION_COMPLETE_SUCC {
        iMissionNum: mission_id,
    };
    client.send_packet(P_FE2CL_REP_PC_MISSION_COMPLETE_SUCC, &resp)
}

pub fn gm_shiny_summon(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = helpers::validate_perms(client, state, CN_ACCOUNT_LEVEL__GM as i16)?;
    let pkt: sP_CL2FE_REQ_SHINY_SUMMON = *client.get_packet(P_CL2FE_REQ_SHINY_SUMMON)?;
    let player = state.get_player(pc_id)?;

    let egg_type = pkt.iShinyType;
    tdata_get().get_egg_stats(egg_type)?;
    let egg_pos = Position {
        x: pkt.iX,
        y: pkt.iY,
        z: pkt.iZ,
    };
    let egg_instance_id = player.instance_id;

    let entity_map = &mut state.entity_map;
    let egg_id = entity_map.gen_next_egg_id();
    let egg = Egg::new(egg_id, egg_type, egg_pos, egg_instance_id, true);
    let chunk_coords = egg.get_chunk_coords();
    let eid = entity_map.track(Box::new(egg), TickMode::Always);
    entity_map.update(eid, Some(chunk_coords), Some(clients));
    Ok(())
}

pub fn gm_npc_summon(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = helpers::validate_perms(client, state, CN_ACCOUNT_LEVEL__GM as i16)?;
    let pkt: sP_CL2FE_REQ_NPC_SUMMON = *client.get_packet(P_CL2FE_REQ_NPC_SUMMON)?;
    let player = state.get_player(pc_id)?;

    let npc_type = pkt.iNPCType;
    tdata_get().get_npc_stats(npc_type)?;

    let spawn_pos = player.get_position();
    let spawn_angle = player.get_rotation();
    let spawn_instance_id = player.instance_id;

    let entity_map = &mut state.entity_map;
    let count = pkt.iNPCCnt as usize;
    for _ in 0..count {
        let npc_id = entity_map.gen_next_npc_id();
        let npc = NPC::new(npc_id, npc_type, spawn_pos, spawn_angle, spawn_instance_id).unwrap();
        helpers::spawn_temp_npc(clients, entity_map, npc);
    }

    Ok(())
}

pub fn gm_npc_group_summon(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = helpers::validate_perms(client, state, CN_ACCOUNT_LEVEL__GM as i16)?;
    let pkt: sP_CL2FE_REQ_NPC_GROUP_SUMMON = *client.get_packet(P_CL2FE_REQ_NPC_GROUP_SUMMON)?;
    let player = state.get_player(pc_id)?;

    let spawn_pos = player.get_position();
    let spawn_angle = player.get_rotation();
    let spawn_instance_id = player.instance_id;

    let group_id = pkt.iNPCGroupType;
    let entity_map = &mut state.entity_map;
    let npcs = tdata_get().make_group_npcs(entity_map, unused!(), group_id);
    for mut npc in npcs {
        npc.set_position(spawn_pos);
        npc.set_rotation(spawn_angle);
        npc.instance_id = spawn_instance_id;
        helpers::spawn_temp_npc(clients, entity_map, npc);
    }

    Ok(())
}

pub fn gm_npc_unsummon(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    helpers::validate_perms(client, state, CN_ACCOUNT_LEVEL__GM as i16)?;
    let pkt: sP_CL2FE_REQ_NPC_UNSUMMON = *client.get_packet(P_CL2FE_REQ_NPC_UNSUMMON)?;
    let npc_id = pkt.iNPC_ID;
    let npc = state.get_npc(npc_id)?;
    if !npc.summoned {
        return Ok(());
    }

    helpers::remove_temp_npc(clients, state, npc_id);
    Ok(())
}

mod helpers {
    use rusty_fusion::ai::AI;

    use super::*;

    pub fn validate_perms(
        client: &mut FFClient,
        state: &ShardServerState,
        req_perms: i16,
    ) -> FFResult<i32> {
        let user_pc_id = client.get_player_id()?;
        let player = state.get_player(user_pc_id)?;
        if player.perms > req_perms {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "{} tried to use cheats without sufficient perms: {}",
                    player, player.perms
                ),
            ));
        }
        Ok(user_pc_id)
    }

    pub fn send_search_fail(client: &mut FFClient, query: PlayerSearchQuery) -> FFError {
        let err_msg = format!("Player not found: {:?}", query);
        let pkt = sP_FE2CL_ANNOUNCE_MSG {
            iAnnounceType: unused!(),
            iDuringTime: MSG_BOX_DURATION_DEFAULT,
            szAnnounceMsg: util::encode_utf16(&err_msg),
        };
        log_if_failed(client.send_packet(P_FE2CL_ANNOUNCE_MSG, &pkt));
        FFError::build(Severity::Warning, err_msg)
    }

    pub fn spawn_temp_npc(clients: &mut ClientMap, entity_map: &mut EntityMap, mut npc: NPC) {
        npc.summoned = true;
        let (ai, tick_mode) = AI::make_for_npc(&npc, true);
        npc.ai = ai;
        let chunk_coords = npc.get_chunk_coords();
        let eid = entity_map.track(Box::new(npc), tick_mode);
        entity_map.update(eid, Some(chunk_coords), Some(clients));
    }

    pub fn remove_temp_npc(clients: &mut ClientMap, state: &mut ShardServerState, npc_id: i32) {
        let entity_map = &mut state.entity_map;
        let eid = EntityID::NPC(npc_id);
        entity_map.update(eid, None, Some(clients));
        let mut npc = entity_map.untrack(eid);
        npc.cleanup(clients, state)
    }
}
