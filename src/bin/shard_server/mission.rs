use rusty_fusion::{
    defines::RANGE_INTERACT,
    entity::{Combatant, EntityID},
    error::*,
    mission::Task,
    net::{
        packet::{PacketID::*, *},
        FFClient,
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

            // check completed missions
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

            // all clear, start the task
            let task: Task = task_def.into();
            let mission_def = task.get_mission_def();

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
        player.set_quest_item_count(*item_id, 0);
    }

    let resp = sP_FE2CL_REP_PC_TASK_STOP_SUCC {
        iTaskNum: pkt.iTaskNum,
    };
    client.send_packet(P_FE2CL_REP_PC_TASK_STOP_SUCC, &resp)
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
