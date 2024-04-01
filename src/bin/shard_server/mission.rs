use rusty_fusion::{
    defines::{RANGE_INTERACT, RANGE_TRIGGER},
    entity::{Combatant, EntityID},
    enums::{ItemLocation, ItemType, TaskType},
    error::*,
    item::Item,
    mission::Task,
    net::{
        packet::{PacketID::*, *},
        ClientMap, FFClient,
    },
    state::ShardServerState,
    tabledata::tdata_get,
    unused,
};

pub fn task_start(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_TASK_START = *client.get_packet(P_CL2FE_REQ_PC_TASK_START)?;
    catch_fail(
        (|| {
            let pc_id = client.get_player_id()?;
            let player = state.get_player(pc_id)?;
            let task_def = tdata_get().get_task_definition(pkt.iTaskNum)?;

            // check giver NPC type + proximity
            if let Some(giver_npc_type) = task_def.prereq_npc_type {
                let req_npc_id = pkt.iNPC_ID;
                let req_npc = state.get_npc(req_npc_id)?;
                if req_npc.ty != giver_npc_type {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Tried to start task {} from NPC type {}, should be {}",
                            pkt.iTaskNum, req_npc.ty, giver_npc_type
                        ),
                    ));
                }
                state.entity_map.validate_proximity(
                    &[EntityID::Player(pc_id), EntityID::NPC(req_npc_id)],
                    RANGE_INTERACT,
                )?;
            }

            // check min level
            if let Some(min_level) = task_def.prereq_level {
                let player_level = player.get_level();
                if player_level < min_level {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Tried to start task {} with level {} < {}",
                            pkt.iTaskNum, player_level, min_level
                        ),
                    ));
                }
            }

            // check guide
            if let Some(guide) = task_def.prereq_guide {
                if player.get_guide() != guide {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Tried to start task {} with guide {:?} != {:?}",
                            pkt.iTaskNum,
                            player.get_guide(),
                            guide
                        ),
                    ));
                }
            }

            // check nanos
            for nano_id in &task_def.prereq_nano_ids {
                if player.get_nano(*nano_id).is_none() {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Tried to start task {} without nano {}",
                            pkt.iTaskNum, nano_id
                        ),
                    ));
                }
            }

            // check if already completed
            if player
                .mission_journal
                .is_mission_completed(task_def.mission_id)
                .unwrap()
            {
                return Err(FFError::build(
                    Severity::Warning,
                    format!(
                        "Tried to start task for already completed mission {}",
                        task_def.mission_id
                    ),
                ));
            }

            // check prereq missions
            for prereq_mission_id in &task_def.prereq_completed_mission_ids {
                if !player
                    .mission_journal
                    .is_mission_completed(*prereq_mission_id)?
                {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Tried to start task {} without completing prereq mission {}",
                            pkt.iTaskNum, prereq_mission_id
                        ),
                    ));
                }
            }

            // check map number
            if let Some(map_num) = task_def.prereq_map_num {
                if player.get_mapnum() != map_num {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Tried to start task {} in mapnum {} != {}",
                            pkt.iTaskNum,
                            player.get_mapnum(),
                            map_num
                        ),
                    ));
                }
            }

            // check previous task for completion or failure
            if !player
                .mission_journal
                .check_completed_previous_task(task_def)
                && !player.mission_journal.check_failed_previous_task(task_def)
            {
                return Err(FFError::build(
                    Severity::Warning,
                    format!(
                        "Tried to start task {} without completing previous task",
                        pkt.iTaskNum
                    ),
                ));
            }

            // check escort npc
            if let Some(escort_npc_type) = task_def.obj_escort_npc_type {
                let escort_npc_id = pkt.iEscortNPC_ID;
                let escort_npc = state.get_npc(escort_npc_id)?;
                if escort_npc.instance_id != player.instance_id {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Tried to start task {} with escort NPC instance {} != player instance {}",
                            pkt.iTaskNum, escort_npc.instance_id, player.instance_id
                        ),
                    ));
                }
                if escort_npc.ty != escort_npc_type {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Tried to start task {} with escort NPC type {}, should be {}",
                            pkt.iTaskNum, escort_npc.ty, escort_npc_type
                        ),
                    ));
                }
                if escort_npc.path.is_none() {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!("Escort NPC {} has no path", escort_npc_id),
                    ));
                }
                state.entity_map.validate_proximity(
                    &[EntityID::Player(pc_id), EntityID::NPC(escort_npc_id)],
                    RANGE_TRIGGER,
                )?;
            }

            // all clear, start the task
            let mut task: Task = task_def.into();
            let mission_def = task.get_mission_def();

            // start escort path
            if task_def.obj_escort_npc_type.is_some() {
                let escort_npc_id = pkt.iEscortNPC_ID;
                let escort_npc = state.get_npc_mut(pkt.iEscortNPC_ID).unwrap();
                escort_npc.path.as_mut().unwrap().start();
                task.escort_npc_id = Some(escort_npc_id);
            }

            let player = state.get_player_mut(pc_id)?;
            if player.mission_journal.start_task(task)? {
                log(
                    Severity::Debug,
                    &format!(
                        "Player {} started mission: {}",
                        player.get_uid(),
                        mission_def.mission_name
                    ),
                );
            }

            let resp = sP_FE2CL_REP_PC_TASK_START_SUCC {
                iTaskNum: pkt.iTaskNum,
                iRemainTime: task_def
                    .obj_time_limit
                    .map(|d| d.as_secs() as i32)
                    .unwrap_or(unused!()),
            };
            client.send_packet(P_FE2CL_REP_PC_TASK_START_SUCC, &resp)
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_TASK_START_FAIL {
                iTaskNum: pkt.iTaskNum,
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_PC_TASK_START_FAIL, &resp)
        },
    )
}

pub fn task_stop(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_TASK_STOP = *client.get_packet(P_CL2FE_REQ_PC_TASK_STOP)?;
    let pc_id = client.get_player_id()?;
    let player = state.get_player_mut(pc_id)?;
    let task = player.mission_journal.remove_task(pkt.iTaskNum)?;

    for item_id in &task.get_task_def().del_qitems {
        let qitem_slot = player.set_quest_item_count(*item_id, 0);
        // client doesn't automatically delete qitems clientside
        let pkt = sP_FE2CL_REP_PC_ITEM_DELETE_SUCC {
            eIL: ItemLocation::QInven as i32,
            iSlotNum: qitem_slot as i32,
        };
        log_if_failed(client.send_packet(P_FE2CL_REP_PC_ITEM_DELETE_SUCC, &pkt));
    }

    let resp = sP_FE2CL_REP_PC_TASK_STOP_SUCC {
        iTaskNum: pkt.iTaskNum,
    };
    client.send_packet(P_FE2CL_REP_PC_TASK_STOP_SUCC, &resp)
}

pub fn task_end(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_TASK_END = *clients.get_self().get_packet(P_CL2FE_REQ_PC_TASK_END)?;
    let mut error_code = None; // true failures are handled in player tick
    catch_fail(
        (|| {
            let pc_id = clients.get_self().get_player_id()?;
            let player = state.get_player(pc_id)?;
            let running_tasks = player.mission_journal.get_current_tasks();
            let task = running_tasks
                .iter()
                .find(|t| t.get_task_id() == pkt.iTaskNum)
                .ok_or(FFError::build(
                    Severity::Warning,
                    format!("Tried to end task {} that is not active", pkt.iTaskNum),
                ))?;

            if task.failed {
                // ignore, task will get cleaned up by next start request
                return Ok(());
            }

            let task_def = task.get_task_def();

            // check target npc type + proximity
            if let Some(target_npc_type) = task_def.obj_npc_type {
                let target_npc_id = pkt.iNPC_ID;
                let target_npc = state.get_npc(target_npc_id)?;
                if target_npc.ty != target_npc_type {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Tried to end task {} with objective NPC type {}, should be {}",
                            pkt.iTaskNum, target_npc.ty, target_npc_type
                        ),
                    ));
                }
                state.entity_map.validate_proximity(
                    &[EntityID::Player(pc_id), EntityID::NPC(target_npc_id)],
                    match task_def.task_type {
                        TaskType::Talk => RANGE_INTERACT,
                        TaskType::GotoLocation => RANGE_TRIGGER,
                        _ => RANGE_INTERACT,
                    },
                )?;
            }

            // check qitems
            for (qitem_id, qitem_count) in &task_def.obj_qitems {
                let has_count = player.get_quest_item_count(*qitem_id);
                if has_count < *qitem_count {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Tried to end task {} with qitem {} count {} (need {})",
                            pkt.iTaskNum, qitem_id, has_count, qitem_count,
                        ),
                    ));
                }
            }

            // check enemies
            let remaining_counts = &task.remaining_enemy_defeats;
            for enemy_id in task_def.obj_enemies.keys() {
                let remaining_count = *remaining_counts.get(enemy_id).unwrap();
                if remaining_count > 0 {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Tried to end task {} with enemy {} x {} remaining",
                            pkt.iTaskNum, enemy_id, remaining_count
                        ),
                    ));
                }
            }

            // check escort path
            if let Some(escort_npc_id) = task.escort_npc_id {
                let escort_npc = state.get_npc(escort_npc_id)?;
                if !escort_npc.path.as_ref().unwrap().is_done() {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Tried to end task {} with escort NPC {} not at destination",
                            pkt.iTaskNum, escort_npc_id
                        ),
                    ));
                }
            }

            // check time limit
            if let Some(time_limit) = task.fail_time {
                if time_limit < std::time::SystemTime::now() {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!("Tried to end task {} after time limit", pkt.iTaskNum),
                    ));
                }
            }

            // check for inventory space for rewards
            if let Some(reward_id) = task_def.succ_reward {
                let reward = tdata_get().get_mission_reward(reward_id)?;
                let inv_space = player.get_free_slots(ItemLocation::Inven);
                if reward.items.len() as usize > inv_space {
                    error_code = Some(codes::TaskEndErr::InventoryFull);
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Tried to end task {} with {} items but only {} inventory space",
                            pkt.iTaskNum,
                            reward.items.len(),
                            inv_space
                        ),
                    ));
                }
            }

            // all clear, mark the task completed. it'll be overwritten by the next task
            let player = state.get_player_mut(pc_id).unwrap();
            player.mission_journal.complete_task(pkt.iTaskNum)?;

            // success qitem changes
            if !task_def.succ_qitems.is_empty() {
                let qitem_pkt = sP_FE2CL_REP_REWARD_ITEM {
                    m_iCandy: player.get_taros() as i32,
                    m_iFusionMatter: player.get_fusion_matter() as i32,
                    m_iBatteryN: player.get_nano_potions() as i32,
                    m_iBatteryW: player.get_weapon_boosts() as i32,
                    iItemCnt: task_def.succ_qitems.len() as i8,
                    iFatigue: 100,
                    iFatigue_Level: 1,
                    iNPC_TypeID: 0,
                    iTaskID: task_def.task_id,
                };
                clients
                    .get_self()
                    .queue_packet(P_FE2CL_REP_REWARD_ITEM, &qitem_pkt);
                for (qitem_id, qitem_count_mod) in &task_def.succ_qitems {
                    let curr_count = player.get_quest_item_count(*qitem_id) as isize;
                    let new_count = (curr_count + *qitem_count_mod) as usize;
                    let qitem_slot = player.set_quest_item_count(*qitem_id, new_count);
                    let qitem_reward = sItemReward {
                        sItem: sItemBase {
                            iType: ItemType::Quest as i16,
                            iID: *qitem_id,
                            iOpt: new_count as i32,
                            iTimeLimit: unused!(),
                        },
                        eIL: ItemLocation::QInven as i32,
                        iSlotNum: qitem_slot as i32,
                    };
                    clients.get_self().queue_struct(&qitem_reward);
                }
                log_if_failed(clients.get_self().flush());
            }

            if let Some(reward_id) = task_def.succ_reward {
                match tdata_get().get_mission_reward(reward_id) {
                    Err(e) => log_error(&e),
                    Ok(reward) => {
                        let taros_new = player.get_taros() + reward.taros;
                        let fm_new = player.get_fusion_matter() + reward.fusion_matter;
                        let reward_pkt = sP_FE2CL_REP_REWARD_ITEM {
                            m_iCandy: player.set_taros(taros_new) as i32,
                            m_iFusionMatter: player.set_fusion_matter(fm_new, Some(clients)) as i32,
                            m_iBatteryN: player.get_nano_potions() as i32,
                            m_iBatteryW: player.get_weapon_boosts() as i32,
                            iItemCnt: reward.items.len() as i8,
                            iFatigue: 100,
                            iFatigue_Level: 1,
                            iNPC_TypeID: unused!(),
                            iTaskID: task_def.task_id,
                        };
                        clients
                            .get_self()
                            .queue_packet(P_FE2CL_REP_REWARD_ITEM, &reward_pkt);
                        for item in &reward.items {
                            let slot_num = player.find_free_slot(ItemLocation::Inven).unwrap();
                            let item_reward = Item::new(item.0, item.1);
                            player
                                .set_item(ItemLocation::Inven, slot_num, Some(item_reward))
                                .unwrap();
                            let item_reward = sItemReward {
                                sItem: Some(item_reward).into(),
                                eIL: ItemLocation::Inven as i32,
                                iSlotNum: slot_num as i32,
                            };
                            clients.get_self().queue_struct(&item_reward);
                        }
                        log_if_failed(clients.get_self().flush());
                    }
                }
            }

            if task_def.succ_task_id.is_none() {
                // Final task, mission complete
                player
                    .mission_journal
                    .remove_task(task_def.task_id)
                    .unwrap();
                player
                    .mission_journal
                    .set_mission_completed(task_def.mission_id)
                    .unwrap();
                if let Some(nano_id) = task_def.succ_nano_id {
                    let player_stats = tdata_get().get_player_stats(player.get_level()).unwrap();
                    match player.unlock_nano(nano_id).cloned() {
                        Ok(nano) => {
                            player.set_fusion_matter(
                                player.get_fusion_matter() - player_stats.req_fm_nano_create,
                                None,
                            );
                            let new_level = std::cmp::max(player.get_level(), nano_id);
                            player.set_level(new_level);

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
                            state.entity_map.for_each_around(
                                EntityID::Player(pc_id),
                                clients,
                                |c| c.send_packet(P_FE2CL_REP_PC_CHANGE_LEVEL, &bcast),
                            );
                        }
                        Err(e) => log_error(&e),
                    }
                }
            }

            let resp = sP_FE2CL_REP_PC_TASK_END_SUCC {
                iTaskNum: pkt.iTaskNum,
            };
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_TASK_END_SUCC, &resp)
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_TASK_END_FAIL {
                iTaskNum: pkt.iTaskNum,
                iErrorCode: error_code.unwrap() as i32,
            };
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_TASK_END_FAIL, &resp)
        },
    )
}

pub fn set_current_mission_id(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_SET_CURRENT_MISSION_ID =
        *client.get_packet(P_CL2FE_REQ_PC_SET_CURRENT_MISSION_ID)?;
    catch_fail(
        (|| {
            let pc_id = client.get_player_id()?;
            let player = state.get_player_mut(pc_id)?;
            let active_mission_slot = player
                .mission_journal
                .set_active_mission_id(pkt.iCurrentMissionID)?;
            log(
                Severity::Debug,
                &format!(
                    "Player {} set active mission slot to {}, mission ID {}",
                    player.get_uid(),
                    active_mission_slot,
                    pkt.iCurrentMissionID
                ),
            );

            let resp = sP_FE2CL_REP_PC_SET_CURRENT_MISSION_ID {
                iCurrentMissionID: pkt.iCurrentMissionID,
            };
            client.send_packet(P_FE2CL_REP_PC_SET_CURRENT_MISSION_ID, &resp)
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_SET_CURRENT_MISSION_ID {
                iCurrentMissionID: 0,
            };
            client.send_packet(P_FE2CL_REP_PC_SET_CURRENT_MISSION_ID, &resp)
        },
    )
}
