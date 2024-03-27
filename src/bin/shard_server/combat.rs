use std::collections::HashMap;

use rusty_fusion::{
    entity::{Combatant, EntityID},
    error::{log, log_if_failed, FFResult, Severity},
    net::{
        packet::{PacketID::*, *},
        ClientMap,
    },
    placeholder,
    state::ShardServerState,
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
    for (ty, count) in defeated_types {
        if player.mission_journal.mark_enemy_defeated(ty, count) {
            let kill_pkt = sP_FE2CL_REP_PC_KILL_QUEST_NPCs_SUCC { iNPCID: ty };
            log_if_failed(client.send_packet(P_FE2CL_REP_PC_KILL_QUEST_NPCs_SUCC, &kill_pkt));
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
