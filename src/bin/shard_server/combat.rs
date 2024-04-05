use std::collections::HashMap;

use rand::{thread_rng, Rng};
use rusty_fusion::{
    entity::{Combatant, EntityID},
    enums::{ItemLocation, ItemType},
    error::{log, log_error, log_if_failed, FFResult, Severity},
    net::{
        packet::{PacketID::*, *},
        ClientMap,
    },
    placeholder,
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
    let mut rng = thread_rng();

    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let pkt: sP_CL2FE_REQ_PC_ATTACK_NPCs = *client.get_packet(P_CL2FE_REQ_PC_ATTACK_NPCs)?;
    let attack_count = if pkt.iNPCCnt > 4 {
        log(
            Severity::Warning,
            &format!(
                "Player {} tried to attack {} NPCs at once",
                pc_id, pkt.iNPCCnt
            ),
        );
        4
    } else {
        pkt.iNPCCnt as usize
    };

    let mut attack_results = Vec::with_capacity(attack_count);
    let mut defeated_types = HashMap::new();
    for _ in 0..pkt.iNPCCnt {
        let target_id = client.get_struct::<sTargetNpcId>()?.iNPC_ID;
        let Ok(target) = state.get_npc_mut(target_id) else {
            log(
                Severity::Warning,
                &format!("Attacked NPC {} not found", target_id),
            );
            continue;
        };
        // TODO proper implementation. This is stubbed to just kill the NPC for mission testing
        let result = sAttackResult {
            eCT: placeholder!(4),
            iID: target_id,
            bProtected: placeholder!(0),
            iDamage: placeholder!(target.get_hp()),
            iHP: placeholder!(0),
            iHitFlag: placeholder!(1),
        };
        attack_results.push(result);
        defeated_types
            .entry(target.ty)
            .and_modify(|count| *count += 1)
            .or_insert(1_usize);
    }

    let player = state.get_player_mut(pc_id)?;
    let resp = sP_FE2CL_PC_ATTACK_NPCs_SUCC {
        iBatteryW: player.get_weapon_boosts() as i32,
        iNPCCnt: attack_results.len() as i32,
    };
    client.queue_packet(P_FE2CL_PC_ATTACK_NPCs_SUCC, &resp);
    for result in &attack_results {
        client.queue_struct(result);
    }
    log_if_failed(client.flush());

    // kills
    let active_task_id = player.mission_journal.get_active_task_id().unwrap_or(0);
    for (ty, count) in defeated_types {
        for _ in 0..count {
            let mut item_rewards = Vec::new();
            let (enemy_in_tasks, count_updated) = player.mission_journal.mark_enemy_defeated(ty);

            // if this kill reduced the remaining enemy count in any tasks, notify the client
            if count_updated {
                let kill_pkt = sP_FE2CL_REP_PC_KILL_QUEST_NPCs_SUCC { iNPCID: ty };
                log_if_failed(
                    clients
                        .get_self()
                        .send_packet(P_FE2CL_REP_PC_KILL_QUEST_NPCs_SUCC, &kill_pkt),
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
                    }
                }
            }

            let mut gained_taros = placeholder!(0);
            let mut gained_fm = placeholder!(0);
            let mut gained_potions = placeholder!(0);
            let mut gained_boosts = placeholder!(0);
            match tdata_get().get_mob_reward(ty) {
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
                m_iBatteryW: player.set_weapon_boosts(player.get_weapon_boosts() + gained_boosts)
                    as i32,
                iItemCnt: item_rewards.len() as i8,
                iFatigue: 100,
                iFatigue_Level: 1,
                iNPC_TypeID: ty,
                iTaskID: active_task_id,
            };
            let client = clients.get_self();
            client.queue_packet(P_FE2CL_REP_REWARD_ITEM, &reward_pkt);
            for item in &item_rewards {
                client.queue_struct(item);
            }
            log_if_failed(client.flush());
        }
    }

    let bcast = sP_FE2CL_PC_ATTACK_NPCs {
        iPC_ID: pc_id,
        iNPCCnt: attack_results.len() as i32,
    };
    state
        .entity_map
        .for_each_around(EntityID::Player(pc_id), clients, |c| {
            c.queue_packet(P_FE2CL_PC_ATTACK_NPCs, &bcast);
            for result in &attack_results {
                c.queue_struct(result);
            }
            c.flush()
        });

    Ok(())
}
