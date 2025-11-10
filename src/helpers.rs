use rand::{rngs::ThreadRng, Rng};
use uuid::Uuid;

use crate::{
    entity::{Combatant, Entity, EntityID, Player},
    enums::*,
    error::*,
    net::{
        packet::{PacketID::*, *},
        ClientMap, FFClient,
    },
    state::ShardServerState,
    tabledata::tdata_get,
    util,
};

pub fn broadcast_state(
    pc_id: i32,
    player_sbf: i8,
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) {
    let bcast = sP_FE2CL_PC_STATE_CHANGE {
        iPC_ID: pc_id,
        iState: player_sbf,
    };
    state
        .entity_map
        .for_each_around(EntityID::Player(pc_id), clients, |client| {
            client.send_packet(P_FE2CL_PC_STATE_CHANGE, &bcast)
        });
}

pub fn broadcast_monkey(
    pc_id: i32,
    ride_type: RideType,
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) {
    let player = state.get_player(pc_id).unwrap();

    // monkey activate packet
    let pkt_monkey = sP_FE2CL_PC_RIDING {
        iPC_ID: pc_id,
        eRT: ride_type as i32,
    };

    // nano stash packets
    let pkt_nano = sP_FE2CL_REP_NANO_ACTIVE_SUCC {
        iActiveNanoSlotNum: -1,
        eCSTB___Add: 0,
    };
    let pkt_nano_bcast = sP_FE2CL_NANO_ACTIVE {
        iPC_ID: pc_id,
        Nano: None.into(),
        iConditionBitFlag: player.get_condition_bit_flag(),
        eCSTB___Add: 0,
    };

    let client = player.get_client(clients).unwrap();
    log_if_failed(client.send_packet(P_FE2CL_REP_PC_RIDING_SUCC, &pkt_monkey));
    log_if_failed(client.send_packet(P_FE2CL_REP_NANO_ACTIVE_SUCC, &pkt_nano));
    state
        .entity_map
        .for_each_around(EntityID::Player(pc_id), clients, |c| {
            c.send_packet(P_FE2CL_PC_RIDING, &pkt_monkey)?;
            c.send_packet(P_FE2CL_NANO_ACTIVE, &pkt_nano_bcast)
        });
}

pub fn remove_group_member(
    leaver_id: EntityID,
    group_id: Uuid,
    state: &mut ShardServerState,
    clients: &mut ClientMap,
) -> FFResult<()> {
    let mut group = state.groups.get(&group_id).unwrap().clone();
    group.remove_member(leaver_id)?;

    if group.should_disband() {
        // we can just tell all players that they've left the group
        // (except the leaver; that is the caller's job)
        let leaver_pkt = sP_FE2CL_PC_GROUP_LEAVE_SUCC { UNUSED: unused!() };
        for eid in group.get_member_ids() {
            let entity = state.entity_map.get_entity_raw(*eid).unwrap();
            if let Some(client) = entity.get_client(clients) {
                log_if_failed(client.send_packet(P_FE2CL_PC_GROUP_LEAVE_SUCC, &leaver_pkt));
            }
            match eid {
                EntityID::Player(pc_id) => {
                    state.get_player_mut(*pc_id).unwrap().group_id = None;
                }
                EntityID::NPC(npc_id) => {
                    state.get_npc_mut(*npc_id).unwrap().group_id = None;
                }
                _ => unreachable!(),
            }
        }

        log(Severity::Debug, &format!("Disbanded group {}", group_id));
        state.groups.remove(&group_id);
        return Ok(());
    }

    // notify clients of the group member removal
    let (pc_group_data, npc_group_data) = group.get_member_data(state);
    match leaver_id {
        EntityID::Player(leaver_pc_id) => {
            let update_pkt = sP_FE2CL_PC_GROUP_LEAVE {
                iID_LeaveMember: leaver_pc_id,
                iMemberPCCnt: pc_group_data.len() as i32,
                iMemberNPCCnt: npc_group_data.len() as i32,
            };
            for eid in group.get_member_ids() {
                let entity = state.entity_map.get_entity_raw(*eid).unwrap();
                if let Some(client) = entity.get_client(clients) {
                    client.queue_packet(P_FE2CL_PC_GROUP_LEAVE, &update_pkt);
                    for pc_data in &pc_group_data {
                        client.queue_struct(pc_data);
                    }
                    for npc_data in &npc_group_data {
                        client.queue_struct(npc_data);
                    }
                    log_if_failed(client.flush());
                }
            }
        }
        EntityID::NPC(leaver_npc_id) => {
            let update_pkt = sP_FE2CL_REP_NPC_GROUP_KICK_SUCC {
                iPC_ID: unused!(),
                iNPC_ID: leaver_npc_id,
                iMemberPCCnt: pc_group_data.len() as i32,
                iMemberNPCCnt: npc_group_data.len() as i32,
            };
            for eid in group.get_member_ids() {
                let entity = state.entity_map.get_entity_raw(*eid).unwrap();
                if let Some(client) = entity.get_client(clients) {
                    client.queue_packet(P_FE2CL_REP_NPC_GROUP_KICK_SUCC, &update_pkt);
                    for pc_data in &pc_group_data {
                        client.queue_struct(pc_data);
                    }
                    for npc_data in &npc_group_data {
                        client.queue_struct(npc_data);
                    }
                    log_if_failed(client.flush());
                }
            }
        }
        _ => unreachable!(),
    }

    // save group state
    state.groups.insert(group_id, group);
    Ok(())
}

pub fn send_system_message(client: &mut FFClient, msg: &str) -> FFResult<()> {
    let resp = sP_FE2CL_PC_MOTD_LOGIN {
        iType: unused!(),
        szSystemMsg: util::encode_utf16(msg)?,
    };
    client.send_packet(P_FE2CL_PC_MOTD_LOGIN, &resp)
}

pub fn give_defeat_rewards(
    player: &mut Player,
    defeated_type: i32,
    clients: &mut ClientMap,
    rng: &mut ThreadRng,
) {
    let active_task_id = player.mission_journal.get_active_task_id().unwrap_or(0);
    let client = player.get_client(clients).unwrap();
    let mut item_rewards = Vec::new();
    let (enemy_in_tasks, count_updated) = player.mission_journal.mark_enemy_defeated(defeated_type);

    // if this kill reduced the remaining enemy count in any tasks, notify the client
    if count_updated {
        let kill_pkt = sP_FE2CL_REP_PC_KILL_QUEST_NPCs_SUCC {
            iNPCID: defeated_type,
        };
        log_if_failed(client.send_packet(P_FE2CL_REP_PC_KILL_QUEST_NPCs_SUCC, &kill_pkt));
    }

    // go through each task that has this enemy as a target and drop quest items
    for task_id in &enemy_in_tasks {
        let task_def = tdata_get().get_task_definition(*task_id).unwrap();
        let mut chosen_qitem = None;
        for qitem in &task_def.dropped_qitems {
            let qitem_id = *qitem.0;
            let max_qitem_count = *task_def.obj_qitems.get(&qitem_id).unwrap_or(&0);
            let qitem_count = player.get_quest_item_count(qitem_id);
            if qitem_count < max_qitem_count {
                match qitem_count {
                    0 => {
                        if player.get_free_slots(ItemLocation::QInven) > 0 {
                            chosen_qitem = Some(qitem);
                            break;
                        }
                    }
                    _ => {
                        chosen_qitem = Some(qitem);
                        break;
                    }
                }
            }
        }

        if let Some((&qitem_id, &drop_chance)) = chosen_qitem {
            let roll: f32 = rng.gen();
            log(
                Severity::Debug,
                &format!(
                    "Rolled {} against {} ({:?}) for qitem {}",
                    roll,
                    drop_chance,
                    roll < drop_chance,
                    qitem_id
                ),
            );
            if roll < drop_chance {
                let new_qitem_count = player.get_quest_item_count(qitem_id) + 1;
                let qitem_slot = player
                    .set_quest_item_count(qitem_id, new_qitem_count)
                    .unwrap();
                let qitem_drop = sItemReward {
                    sItem: sItemBase {
                        iType: ItemType::Quest as i16,
                        iID: qitem_id,
                        iOpt: new_qitem_count as i32,
                        iTimeLimit: unused!(),
                    },
                    eIL: ItemLocation::QInven as i32,
                    iSlotNum: qitem_slot as i32,
                };
                if active_task_id == *task_id {
                    // active task rewards should show up first
                    item_rewards.insert(0, qitem_drop);
                } else {
                    item_rewards.push(qitem_drop);
                }

                // fresh drop so a repair won't be needed.
                // to prevent one from happening, we signal that it's already done
                player.mission_journal.repair_task(*task_id).unwrap();
            }
        }
    }

    let mut gained_taros = 0;
    let mut gained_fm = 0;
    let mut gained_potions = 0;
    let mut gained_boosts = 0;
    match tdata_get()
        .get_mob_reward(defeated_type)
        .map(|r| r.with_rates(&player.reward_data))
    {
        Ok(reward) => {
            gained_taros = reward.taros;
            gained_fm = reward.fusion_matter;
            gained_potions = reward.nano_potions;
            gained_boosts = reward.weapon_boosts;
            for item in reward.items {
                if let Ok(slot) = player.find_free_slot(ItemLocation::Inven) {
                    player
                        .set_item(ItemLocation::Inven, slot, Some(item))
                        .unwrap();
                    let item_reward = sItemReward {
                        sItem: Some(item).into(),
                        eIL: ItemLocation::Inven as i32,
                        iSlotNum: slot as i32,
                    };
                    item_rewards.push(item_reward);
                }
            }
        }
        Err(e) => log_error(&e),
    }

    let reward_pkt = sP_FE2CL_REP_REWARD_ITEM {
        m_iCandy: player.set_taros(player.get_taros() + gained_taros) as i32,
        m_iFusionMatter: player
            .set_fusion_matter(player.get_fusion_matter() + gained_fm, Some(clients))
            as i32,
        m_iBatteryN: player.set_nano_potions(player.get_nano_potions() + gained_potions) as i32,
        m_iBatteryW: player.set_weapon_boosts(player.get_weapon_boosts() + gained_boosts) as i32,
        iItemCnt: item_rewards.len() as i8,
        iFatigue: 100,
        iFatigue_Level: 1,
        iNPC_TypeID: defeated_type,
        iTaskID: active_task_id,
    };
    let client = player.get_client(clients).unwrap();
    client.queue_packet(P_FE2CL_REP_REWARD_ITEM, &reward_pkt);
    for item in &item_rewards {
        client.queue_struct(item);
    }
    log_if_failed(client.flush());
}
