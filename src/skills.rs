use crate::{
    entity::{Combatant, EntityID},
    error::*,
    net::{
        packet::{PacketID::*, *},
        ClientMap, PacketBuffer,
    },
    state::ShardServerState,
};

pub fn do_basic_attack(
    attacker_id: EntityID,
    target_ids: &[EntityID],
    damage: i32,
    state: &mut ShardServerState,
    clients: &mut ClientMap,
) -> FFResult<()> {
    let cb = state.get_combatant(attacker_id)?;
    let mut attacker_client = cb.get_client(clients);

    let mut pc_attack_results = Vec::new();
    let mut npc_attack_results = Vec::new();
    for target_id in target_ids {
        let target = match state.get_combatant_mut(*target_id) {
            Ok(target) => target,
            Err(e) => {
                log_error(&e);
                continue;
            }
        };
        if target.is_dead() {
            log(
                Severity::Warning,
                &format!(
                    "{:?} tried to attack dead target {:?}",
                    attacker_id, target_id
                ),
            );
            continue;
        }
        let result = handle_basic_attack(attacker_id, target, damage);
        match target_id {
            EntityID::Player(_) => pc_attack_results.push(result),
            EntityID::NPC(_) => npc_attack_results.push(result),
            _ => unreachable!(),
        }
    }

    let pc_attack_count = pc_attack_results.len();
    let npc_attack_count = npc_attack_results.len();
    if pc_attack_count == 0 && npc_attack_count == 0 {
        return Ok(());
    }

    // PC targets
    let pc_payload = if pc_attack_count > 0 {
        let mut payload = PacketBuffer::default();
        match attacker_id {
            EntityID::Player(pc_id) => {
                // response packet
                let player = state.get_player(pc_id).unwrap();
                let resp = sP_FE2CL_PC_ATTACK_CHARs_SUCC {
                    iBatteryW: player.get_weapon_boosts() as i32,
                    iTargetCnt: pc_attack_count as i32,
                };
                if let Some(client) = &mut attacker_client {
                    client.queue_packet(P_FE2CL_PC_ATTACK_CHARs_SUCC, &resp);
                }

                // broadcast packet
                let pkt = sP_FE2CL_PC_ATTACK_CHARs {
                    iPC_ID: pc_id,
                    iTargetCnt: pc_attack_count as i32,
                };
                payload.queue_packet(P_FE2CL_PC_ATTACK_CHARs, &pkt);
            }
            EntityID::NPC(npc_id) => {
                let pkt = sP_FE2CL_NPC_ATTACK_PCs {
                    iNPC_ID: npc_id,
                    iPCCnt: pc_attack_count as i32,
                };
                payload.queue_packet(P_FE2CL_NPC_ATTACK_PCs, &pkt);
            }
            _ => unreachable!(),
        }

        for result in &pc_attack_results {
            if let Some(client) = &mut attacker_client {
                client.queue_struct(result);
            }
            payload.queue_struct(result);
        }

        if let Some(client) = &mut attacker_client {
            log_if_failed(client.flush());
        }
        Some(payload)
    } else {
        None
    };

    // NPC targets
    let npc_payload = if npc_attack_count > 0 {
        let mut payload = PacketBuffer::default();
        match attacker_id {
            EntityID::Player(pc_id) => {
                // response packet
                let player = state.get_player(pc_id).unwrap();
                let resp = sP_FE2CL_PC_ATTACK_NPCs_SUCC {
                    iBatteryW: player.get_weapon_boosts() as i32,
                    iNPCCnt: npc_attack_count as i32,
                };
                if let Some(client) = &mut attacker_client {
                    client.queue_packet(P_FE2CL_PC_ATTACK_NPCs_SUCC, &resp);
                }

                // broadcast packet
                let pkt = sP_FE2CL_PC_ATTACK_NPCs {
                    iPC_ID: pc_id,
                    iNPCCnt: npc_attack_count as i32,
                };
                payload.queue_packet(P_FE2CL_PC_ATTACK_NPCs, &pkt);
            }
            EntityID::NPC(npc_id) => {
                let pkt = sP_FE2CL_NPC_ATTACK_CHARs {
                    iNPC_ID: npc_id,
                    iTargetCnt: npc_attack_count as i32,
                };
                payload.queue_packet(P_FE2CL_NPC_ATTACK_CHARs, &pkt);
            }
            _ => unreachable!(),
        }

        for result in &npc_attack_results {
            if let Some(client) = &mut attacker_client {
                client.queue_struct(result);
            }
            payload.queue_struct(result);
        }

        if let Some(client) = &mut attacker_client {
            log_if_failed(client.flush());
        }
        Some(payload)
    } else {
        None
    };

    state.entity_map.for_each_around(attacker_id, clients, |c| {
        if let Some(pc_payload) = pc_payload.as_ref() {
            c.send_payload(pc_payload.clone())?;
        }
        if let Some(npc_payload) = npc_payload.as_ref() {
            c.send_payload(npc_payload.clone())?;
        }
        Ok(())
    });

    Ok(())
}

fn handle_basic_attack(from: EntityID, to: &mut dyn Combatant, damage: i32) -> sAttackResult {
    let dealt = to.take_damage(damage, from);
    sAttackResult {
        eCT: to.get_char_type() as i32,
        iID: match to.get_id() {
            EntityID::Player(id) => id,
            EntityID::NPC(id) => id,
            _ => unreachable!(),
        },
        bProtected: unused!(),
        iDamage: dealt,
        iHP: to.get_hp(),
        iHitFlag: placeholder!(1),
    }
}
