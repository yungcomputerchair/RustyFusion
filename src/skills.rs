use rand::Rng;

use crate::{
    defines::*,
    entity::{Combatant, EntityID},
    enums::CombatStyle,
    error::*,
    net::{
        packet::{PacketID::*, *},
        ClientMap, PacketBuffer,
    },
    state::ShardServerState,
};

struct BasicAttack {
    power: i32,
    crit_chance: Option<f32>,
    attack_style: Option<CombatStyle>,
    charged: bool,
}

pub fn do_basic_attack(
    attacker_id: EntityID,
    target_ids: &[EntityID],
    charged: bool,
    state: &mut ShardServerState,
    clients: &mut ClientMap,
) -> FFResult<()> {
    const CRIT_CHANCE: f32 = 0.05;

    let attacker = state.get_combatant(attacker_id)?;
    let mut attacker_client = attacker.get_client(clients);

    let power = if target_ids.len() == 1 {
        attacker.get_single_power()
    } else {
        attacker.get_multi_power()
    };
    let basic_attack = BasicAttack {
        power,
        crit_chance: Some(CRIT_CHANCE),
        attack_style: attacker.get_style(),
        charged,
    };

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
        let result = handle_basic_attack(attacker_id, target, &basic_attack);
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

fn calculate_damage(
    attack: &BasicAttack,
    defense: i32,
    defense_style: Option<CombatStyle>,
    defense_level: i16,
) -> (i32, bool) {
    // this formula is taken basically 1:1 from OpenFusion
    let mut rng = rand::thread_rng();
    let BasicAttack {
        power: attack,
        crit_chance,
        attack_style,
        charged,
    } = attack;

    // base damage + variability
    if attack + defense == 0 {
        // divide-by-0 check
        return (0, false);
    }
    let mut damage = attack * attack / (attack + defense);
    damage = std::cmp::max(
        10 + attack / 10,
        damage - (defense - attack / 6) * defense_level as i32 / 100,
    );
    damage = (damage as f32 * (rng.gen_range(0.8..1.2))) as i32;

    // rock-paper-scissors
    let rps = do_rps(attack_style, &defense_style);
    match rps {
        RpsResult::Win => {
            damage = damage * 5 / 4;
        }
        RpsResult::Lose => {
            damage = damage * 4 / 5;
        }
        RpsResult::Draw => {}
    };

    // boost
    if *charged {
        damage = damage * 5 / 4;
    }

    // crit
    let crit = match crit_chance {
        Some(crit_chance) => rng.gen::<f32>() < *crit_chance,
        None => false,
    };
    if crit {
        damage *= 2;
    }

    (damage, crit)
}

fn handle_basic_attack(
    from: EntityID,
    to: &mut dyn Combatant,
    attack: &BasicAttack,
) -> sAttackResult {
    let defense = to.get_defense();
    let defense_style = to.get_style();
    let defense_level = to.get_level();
    let (damage, crit) = calculate_damage(attack, defense, defense_style, defense_level);
    let dealt = to.take_damage(damage, from);

    let mut hit_flag = HF_BIT_NORMAL as i8;
    if crit {
        hit_flag |= HF_BIT_CRITICAL as i8;
    }

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
        iHitFlag: hit_flag,
    }
}

enum RpsResult {
    Win,
    Lose,
    Draw,
}
fn do_rps(us: &Option<CombatStyle>, them: &Option<CombatStyle>) -> RpsResult {
    if us.is_none() || them.is_none() {
        return RpsResult::Draw;
    }

    let us = us.as_ref().unwrap();
    let them = them.as_ref().unwrap();
    match us {
        CombatStyle::Adaptium => match them {
            CombatStyle::Blastons => RpsResult::Win,
            CombatStyle::Cosmix => RpsResult::Lose,
            _ => RpsResult::Draw,
        },

        CombatStyle::Blastons => match them {
            CombatStyle::Cosmix => RpsResult::Win,
            CombatStyle::Adaptium => RpsResult::Lose,
            _ => RpsResult::Draw,
        },

        CombatStyle::Cosmix => match them {
            CombatStyle::Adaptium => RpsResult::Win,
            CombatStyle::Blastons => RpsResult::Lose,
            _ => RpsResult::Draw,
        },
    }
}
