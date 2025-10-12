use std::time::SystemTime;

use rusty_fusion::{
    defines::EQUIP_SLOT_HAND,
    entity::{Combatant, Entity, EntityID, Projectile, ProjectileKind},
    enums::{CharType, WeaponTargetMode},
    error::*,
    net::{
        packet::{PacketID::*, *},
        ClientMap, FFClient,
    },
    skills,
    state::ShardServerState,
    unused, Position,
};

const BATTERY_BASE_COST: u32 = 6;

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[repr(packed(4))]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct sTargetNpcId {
    pub iNPC_ID: i32,
}
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[repr(packed(4))]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct sTargetAnyId {
    pub iEntity_ID: i32,
    pub eCT: i32,
}

impl FFPacket for sTargetNpcId {}
impl FFPacket for sTargetAnyId {}

fn get_targets(
    client: &mut FFClient,
    state: &mut ShardServerState,
    target_count: usize,
    target_type: Option<CharType>,
    max_targets: Option<usize>,
) -> FFResult<(Vec<EntityID>, u32)> {
    let mut target_ids = Vec::with_capacity(max_targets.unwrap_or(3));
    let mut weapon_boosts_needed = 0;

    for i in 0..target_count {
        // TODO stricter anti-cheat.
        // validate target count, range, attack cooldown, etc against weapon stats
        if max_targets.is_some_and(|max| i >= max) {
            log(
                Severity::Warning,
                &format!(
                    "Tried to attack {} entities (max {})",
                    target_count,
                    max_targets.unwrap()
                ),
            );
            return Ok((target_ids, weapon_boosts_needed));
        }

        match target_type {
            Some(CharType::Player) => todo!(),
            None | Some(CharType::All) => {
                let Ok(trailer) = client.get_struct::<sTargetAnyId>() else {
                    break;
                };
                let entity_id = trailer.iEntity_ID;
                let Ok(char_type) = CharType::try_from(trailer.eCT) else {
                    continue;
                };

                let entity_id = match char_type {
                    CharType::Unknown => continue,
                    CharType::Player => EntityID::Player(entity_id),
                    CharType::NPC | CharType::Mob => EntityID::NPC(entity_id),
                    CharType::All => continue,
                };

                let Ok(combatant) = state.get_combatant(entity_id) else {
                    continue;
                };
                weapon_boosts_needed += BATTERY_BASE_COST + combatant.get_level() as u32;
                target_ids.push(combatant.get_id());
            }
            Some(_) => {
                let Ok(trailer) = client.get_struct::<sTargetNpcId>() else {
                    break;
                };
                let npc_id = trailer.iNPC_ID;
                let npc = match state.get_npc(npc_id) {
                    Ok(npc) => npc,
                    Err(e) => {
                        log_error(&e);
                        continue;
                    }
                };
                weapon_boosts_needed += BATTERY_BASE_COST + npc.get_level() as u32;
                target_ids.push(npc.get_id());
            }
        }
    }

    Ok((target_ids, weapon_boosts_needed))
}

pub fn pc_attack_npcs(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    const MAX_TARGETS: usize = 3;

    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let pkt: sP_CL2FE_REQ_PC_ATTACK_NPCs = *client.get_packet(P_CL2FE_REQ_PC_ATTACK_NPCs)?;
    let target_count = pkt.iNPCCnt as usize;
    if target_count == 0 {
        return Ok(());
    }

    let (target_ids, weapon_boosts_needed) = get_targets(
        client,
        state,
        target_count,
        Some(CharType::Mob),
        Some(MAX_TARGETS),
    )?;

    let player = state.get_player_mut(pc_id)?;
    let charged = player.consume_weapon_boosts(weapon_boosts_needed);

    // attack handler
    skills::do_basic_attack(
        player.get_id(),
        &target_ids,
        charged,
        (None, None),
        (None, None),
        state,
        clients,
    )?;

    Ok(())
}

pub fn pc_fire_rocket(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_ROCKET_STYLE_FIRE = *clients
        .get_self()
        .get_packet(P_CL2FE_REQ_PC_ROCKET_STYLE_FIRE)?;

    let new_pkt = sP_CL2FE_REQ_PC_GRENADE_STYLE_FIRE {
        iSkillID: pkt.iSkillID,
        iToX: pkt.iToX,
        iToY: pkt.iToY,
        iToZ: pkt.iToZ,
    };

    pc_fire_projectile(clients, state, new_pkt, false)
}

pub fn pc_fire_grenade(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_GRENADE_STYLE_FIRE = *clients
        .get_self()
        .get_packet(P_CL2FE_REQ_PC_GRENADE_STYLE_FIRE)?;

    pc_fire_projectile(clients, state, pkt, true)
}

fn pc_fire_projectile(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
    pkt: sP_CL2FE_REQ_PC_GRENADE_STYLE_FIRE,
    is_grenade: bool,
) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let player = state.get_player_mut(pc_id)?;

    let action_message = if is_grenade {
        "throw grenade"
    } else {
        "fire rocket"
    };
    let expected_target_mode = if is_grenade {
        WeaponTargetMode::Grenade
    } else {
        WeaponTargetMode::Rocket
    };

    let Some(weapon) = player.get_equipped()[EQUIP_SLOT_HAND as usize] else {
        return Err(FFError::build(
            Severity::Warning,
            format!("Tried to {} but no weapon in hand", action_message),
        ));
    };

    let weapon_data = weapon.get_stats()?;

    if weapon_data.target_mode != Some(expected_target_mode) {
        return Err(FFError::build(
            Severity::Warning,
            format!("Tried to {} but holding wrong weapon type", action_message),
        ));
    }

    let weapon_boosts_needed = BATTERY_BASE_COST + weapon_data.required_level as u32;
    let charged = player.consume_weapon_boosts(weapon_boosts_needed);

    let projectile = Projectile {
        projectile_kind: if is_grenade {
            ProjectileKind::Grenade
        } else {
            ProjectileKind::Rocket(weapon.id)
        },
        single_power: player.get_single_power(),
        multi_power: player.get_multi_power(),
        charged,
        end_time: SystemTime::now() + weapon_data.projectile_time.unwrap(),
        start_pos: Position {
            z: player.get_position().z + 100,
            ..player.get_position()
        },
        // todo: validate
        end_pos: Position {
            x: pkt.iToX,
            y: pkt.iToY,
            z: pkt.iToZ + 100,
        },
    };

    let Some(bullet_id) = player.add_projectile(projectile.clone()) else {
        return Err(FFError::build(
            Severity::Warning,
            format!(
                "Attempted to {} but player has too many projectiles",
                action_message
            ),
        ));
    };

    let resp = sP_FE2CL_REP_PC_GRENADE_STYLE_FIRE_SUCC {
        iSkillID: unused!(),
        iToX: projectile.end_pos.x,
        iToY: projectile.end_pos.y,
        iToZ: projectile.end_pos.z,
        iBulletID: bullet_id,
        Bullet: projectile.clone().into(),
        iBatteryW: player.get_weapon_boosts() as i32,
        bNanoDeactive: unused!(),
        iNanoID: unused!(),
        iNanoStamina: unused!(),
    };

    let bcast = sP_FE2CL_PC_GRENADE_STYLE_FIRE {
        iPC_ID: pc_id,
        iToX: pkt.iToX,
        iToY: pkt.iToY,
        iToZ: pkt.iToZ,
        iBulletID: bullet_id,
        Bullet: projectile.into(),
        bNanoDeactive: unused!(),
    };

    state
        .entity_map
        .for_each_around(EntityID::Player(pc_id), clients, |c| {
            c.send_packet(P_FE2CL_PC_GRENADE_STYLE_FIRE, &bcast)
        });
    clients
        .get_self()
        .send_packet(P_FE2CL_REP_PC_GRENADE_STYLE_FIRE_SUCC, &resp)
}

pub fn pc_projectile_hit(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    const EXPLOSION_RADIUS: u32 = 300; // TODO pull from tdata
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let player = state.get_player_mut(pc_id)?;

    // Rocket and grenade hit are indentical
    let pkt: sP_CL2FE_REQ_PC_ROCKET_STYLE_HIT = *client.get_packet_unchecked()?;

    let Some(projectile) = player.remove_projectile(pkt.iBulletID) else {
        return Ok(());
    };

    let (target_ids, _weapon_boosts_needed) =
        get_targets(client, state, pkt.iTargetCnt as usize, None, Some(100))?;
    let player = state.get_player(pc_id)?;

    // TODO: validate
    let hit_position = Position::new(pkt.iX, pkt.iY, pkt.iZ);
    let target_ids = state.entity_map.filter_ids_in_proximity(
        hit_position,
        player.instance_id,
        &target_ids,
        EXPLOSION_RADIUS,
    );

    skills::do_basic_attack(
        EntityID::Player(pc_id),
        &target_ids,
        projectile.charged,
        (Some(projectile.single_power), Some(projectile.multi_power)),
        (Some(pkt.iBulletID), Some(projectile.clone())),
        state,
        clients,
    )?;

    Ok(())
}
