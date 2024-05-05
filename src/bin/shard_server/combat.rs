use std::collections::HashMap;

use rand::{thread_rng, Rng};
use rusty_fusion::{
    defines::RANGE_GROUP_PARTICIPATE,
    entity::{Combatant, Entity, EntityID, Player},
    enums::{ItemLocation, ItemType},
    error::{log, log_error, log_if_failed, FFError, FFResult, Severity},
    net::{
        packet::{PacketID::*, *},
        ClientMap,
    },
    placeholder, skills,
    state::ShardServerState,
    tabledata::tdata_get,
    unused,
};

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[repr(packed(4))]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct sTargetNpcId {
    pub iNPC_ID: i32,
}
impl FFPacket for sTargetNpcId {}

pub fn pc_attack_npcs(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    const MAX_TARGETS: usize = 4;

    let mut rng = thread_rng();

    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let pkt: sP_CL2FE_REQ_PC_ATTACK_NPCs = *client.get_packet(P_CL2FE_REQ_PC_ATTACK_NPCs)?;
    let target_count = pkt.iNPCCnt as usize;
    if target_count == 0 {
        return Ok(());
    }
    if target_count > MAX_TARGETS {
        return Err(FFError::build(
            Severity::Warning,
            format!(
                "Player {} tried to attack {} NPCs (max {})",
                pc_id, pkt.iNPCCnt, MAX_TARGETS
            ),
        ));
    }

    let mut defeated_types = HashMap::new();
    let mut target_ids = Vec::with_capacity(4);
    for _ in 0..target_count {
        let npc_id = client.get_struct::<sTargetNpcId>()?.iNPC_ID;
        let target_id = EntityID::NPC(npc_id);
        target_ids.push(target_id);
    }
    let attacker_id = EntityID::Player(pc_id);
    let damage = placeholder!(50);
    skills::do_basic_attack(attacker_id, &target_ids, damage, state, clients)?;

    // kills
    for target_id in &target_ids {
        if let EntityID::NPC(npc_id) = target_id {
            let npc = state.get_npc(*npc_id).unwrap();
            if npc.is_dead() {
                *defeated_types.entry(npc.ty).or_insert(0) += 1;
            }
        }
    }

    // rewards
    let player = state.get_player_mut(pc_id)?;
    helpers::give_defeat_rewards(&defeated_types, player, clients, &mut rng);
    if let Some(group_id) = player.group_id {
        let position = player.get_position();
        let group = state.groups.get(&group_id).unwrap().clone();
        for eid in group.get_member_ids() {
            if let EntityID::Player(member_pc_id) = *eid {
                if pc_id == member_pc_id {
                    continue;
                }
                let member = state.get_player_mut(member_pc_id)?;
                if member.get_position().distance_to(&position) < RANGE_GROUP_PARTICIPATE {
                    helpers::give_defeat_rewards(&defeated_types, member, clients, &mut rng);
                }
            }
        }
    }

    Ok(())
}

mod helpers {
    use rusty_fusion::entity::Entity;

    use super::*;

    pub fn give_defeat_rewards(
        defeated_types: &HashMap<i32, usize>,
        player: &mut Player,
        clients: &mut ClientMap,
        rng: &mut impl Rng,
    ) {
        let active_task_id = player.mission_journal.get_active_task_id().unwrap_or(0);
        for (&ty, &count) in defeated_types {
            for _ in 0..count {
                let client = player.get_client(clients).unwrap();
                let mut item_rewards = Vec::new();
                let (enemy_in_tasks, count_updated) =
                    player.mission_journal.mark_enemy_defeated(ty);

                // if this kill reduced the remaining enemy count in any tasks, notify the client
                if count_updated {
                    let kill_pkt = sP_FE2CL_REP_PC_KILL_QUEST_NPCs_SUCC { iNPCID: ty };
                    log_if_failed(
                        client.send_packet(P_FE2CL_REP_PC_KILL_QUEST_NPCs_SUCC, &kill_pkt),
                    );
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
                    .get_mob_reward(ty)
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
                    m_iBatteryN: player.set_nano_potions(player.get_nano_potions() + gained_potions)
                        as i32,
                    m_iBatteryW: player
                        .set_weapon_boosts(player.get_weapon_boosts() + gained_boosts)
                        as i32,
                    iItemCnt: item_rewards.len() as i8,
                    iFatigue: 100,
                    iFatigue_Level: 1,
                    iNPC_TypeID: ty,
                    iTaskID: active_task_id,
                };
                let client = player.get_client(clients).unwrap();
                client.queue_packet(P_FE2CL_REP_REWARD_ITEM, &reward_pkt);
                for item in &item_rewards {
                    client.queue_struct(item);
                }
                log_if_failed(client.flush());
            }
        }
    }
}
