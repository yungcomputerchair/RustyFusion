use rand::Rng;

use crate::{
    defines::*,
    entity::{Combatant, EntityID},
    enums::CombatStyle,
    error::*,
    net::packet::{PacketID::*, *},
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
) -> FFResult<()> {
    const CRIT_CHANCE: f32 = 0.05;

    if let EntityID::Player(pc_id) = attacker_id {
        let player = state.get_player_mut(pc_id)?;
        if let Some(eid) = target_ids.first() {
            // last_attacked_by is used by scripts as an indicator
            // of who the player is in combat with
            player.target = Some(*eid);
        }
    }

    let attacker = state.get_combatant(attacker_id)?;
    let mut attacker_client = attacker.get_client();

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
                log_error(e);
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

    let pc_count = pc_attack_results.len();
    let npc_count = npc_attack_results.len();
    if pc_count == 0 && npc_count == 0 {
        return Ok(());
    }

    let battery_w = if let EntityID::Player(pc_id) = attacker_id {
        Some(state.get_player(pc_id).unwrap().get_weapon_boosts() as i32)
    } else {
        None
    };

    // PC targets
    let pc_bcast = if pc_count > 0 {
        // attacker response (PC attackers only)
        if let (EntityID::Player(_), Some(bw), Some(client)) =
            (attacker_id, battery_w, attacker_client.as_mut())
        {
            let mut resp = PacketBuilder::new(P_FE2CL_PC_ATTACK_CHARs_SUCC).with(
                &sP_FE2CL_PC_ATTACK_CHARs_SUCC {
                    iBatteryW: bw,
                    iTargetCnt: pc_count as i32,
                },
            );

            for r in &pc_attack_results {
                resp.push(r);
            }

            if let Some(resp) = log_if_failed(resp.build()) {
                client.send_payload(resp)
            }
        }

        // broadcast
        let mut bcast = match attacker_id {
            EntityID::Player(pc_id) => {
                PacketBuilder::new(P_FE2CL_PC_ATTACK_CHARs).with(&sP_FE2CL_PC_ATTACK_CHARs {
                    iPC_ID: pc_id,
                    iTargetCnt: pc_count as i32,
                })
            }
            EntityID::NPC(npc_id) => {
                PacketBuilder::new(P_FE2CL_NPC_ATTACK_PCs).with(&sP_FE2CL_NPC_ATTACK_PCs {
                    iNPC_ID: npc_id,
                    iPCCnt: pc_count as i32,
                })
            }
            _ => unreachable!(),
        };

        for r in &pc_attack_results {
            bcast.push(r);
        }

        Some(bcast.build()?)
    } else {
        None
    };

    // NPC targets
    let npc_bcast = if npc_count > 0 {
        // attacker response (PC attackers only)
        if let (EntityID::Player(_), Some(bw), Some(client)) =
            (attacker_id, battery_w, attacker_client.as_mut())
        {
            let mut resp = PacketBuilder::new(P_FE2CL_PC_ATTACK_NPCs_SUCC).with(
                &sP_FE2CL_PC_ATTACK_NPCs_SUCC {
                    iBatteryW: bw,
                    iNPCCnt: npc_count as i32,
                },
            );

            for r in &npc_attack_results {
                resp.push(r);
            }

            if let Some(resp) = log_if_failed(resp.build()) {
                client.send_payload(resp);
            }
        }

        // broadcast
        let mut bcast = match attacker_id {
            EntityID::Player(pc_id) => {
                PacketBuilder::new(P_FE2CL_PC_ATTACK_NPCs).with(&sP_FE2CL_PC_ATTACK_NPCs {
                    iPC_ID: pc_id,
                    iNPCCnt: npc_count as i32,
                })
            }
            EntityID::NPC(npc_id) => {
                PacketBuilder::new(P_FE2CL_NPC_ATTACK_CHARs).with(&sP_FE2CL_NPC_ATTACK_CHARs {
                    iNPC_ID: npc_id,
                    iTargetCnt: npc_count as i32,
                })
            }
            _ => unreachable!(),
        };

        for r in &npc_attack_results {
            bcast.push(r);
        }

        Some(bcast.build()?)
    } else {
        None
    };

    state.entity_map.for_each_around(attacker_id, |c| {
        if let Some(pkt) = &pc_bcast {
            c.send_payload(pkt.clone());
        }
        if let Some(pkt) = &npc_bcast {
            c.send_payload(pkt.clone());
        }
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
