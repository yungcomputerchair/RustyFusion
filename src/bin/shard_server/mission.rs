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
            if let Some(giver_npc_type) = task_def.giver_npc_type {
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

            // check items
            for (item_id, count) in &task_def.prereq_items {
                if player.get_quest_item_count(*item_id) < *count {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Tried to start task {} without quest item {} x {}",
                            pkt.iTaskNum, item_id, count
                        ),
                    ));
                }
            }

            // check required running task ID
            if let Some(running_task_id) = task_def.prereq_running_task_id {
                let running_task_ids = player.mission_journal.get_current_task_ids();
                if !running_task_ids.contains(&running_task_id) {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Tried to start task {} without current task {}",
                            pkt.iTaskNum, running_task_id
                        ),
                    ));
                }
            }

            // check completed missions
            if player
                .mission_journal
                .is_mission_completed(task_def.mission_id)
            {
                // TODO check repeatability
                return Err(FFError::build(
                    Severity::Warning,
                    format!(
                        "Tried to start task for already completed mission {}",
                        task_def.mission_id
                    ),
                ));
            }

            // all clear, start the task
            let task: Task = task_def.into();

            let player = state.get_player_mut(pc_id)?;
            player.mission_journal.start_task(task)?;

            let resp = sP_FE2CL_REP_PC_TASK_START_SUCC {
                iTaskNum: pkt.iTaskNum,
                iRemainTime: task_def
                    .time_limit
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
