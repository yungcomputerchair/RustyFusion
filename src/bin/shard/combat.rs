use std::time::SystemTime;

use rusty_fusion::{
    defines::EQUIP_SLOT_HAND,
    entity::{Combatant, Entity, EntityID, Projectile, ProjectileKind},
    error::*,
    net::{
        packet::{PacketID::*, *},
        ClientMap, FFClient,
    },
    skills,
    state::ShardServerState,
    unused, Position,
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

fn get_targets(
    client: &mut FFClient,
    state: &mut ShardServerState,
    target_count: usize,
    max_targets: Option<usize>,
) -> FFResult<(Vec<EntityID>, u32)> {
    const BATTERY_BASE_COST: u32 = 6;

    let mut target_ids = Vec::with_capacity(max_targets.unwrap_or(3));
    let mut weapon_boosts_needed = 0;

    for i in 0..target_count {
        // TODO stricter anti-cheat.
        // validate target count, range, attack cooldown, etc against weapon stats
        if max_targets.is_some_and(|max| i >= max) {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "Tried to attack {} entities (max {})",
                    target_count,
                    max_targets.unwrap()
                ),
            ));
        }
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

    let (target_ids, weapon_boosts_needed) =
        get_targets(client, state, target_count, Some(MAX_TARGETS))?;

    // consume weapon boosts
    let player = state.get_player_mut(pc_id)?;
    let weapon_boosts = player.get_weapon_boosts();
    let charged = if weapon_boosts >= weapon_boosts_needed {
        player.set_weapon_boosts(weapon_boosts - weapon_boosts_needed);
        true
    } else {
        player.set_weapon_boosts(0);
        false
    };

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

pub fn pc_grenade_fire(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let player = state.get_player_mut(pc_id)?;

    let pkt: sP_CL2FE_REQ_PC_GRENADE_STYLE_FIRE =
        *client.get_packet(P_CL2FE_REQ_PC_GRENADE_STYLE_FIRE)?;

    let Some(weapon) = player.get_equipped()[EQUIP_SLOT_HAND as usize] else {
        return Err(FFError::build(
            Severity::Warning,
            "Tried to throw grenade but no weapon in hand".to_string(),
        ));
    };

    let weapon_data = weapon.get_stats()?;

    if weapon_data.target_mode != Some(6) {
        return Err(FFError::build(
            Severity::Warning,
            "Tried to throw grenade but holding wrong weapon type".to_string(),
        ));
    }

    let projectile = Projectile {
        projectile_kind: ProjectileKind::Grenade,
        single_power: player.get_single_power(),
        multi_power: player.get_multi_power(),
        charged: false,
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
            "Attempted to throw grenade but player has too many projectiles".to_string(),
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

pub fn pc_rocket_fire(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let player = state.get_player_mut(pc_id)?;

    let pkt: sP_CL2FE_REQ_PC_ROCKET_STYLE_FIRE =
        *client.get_packet(P_CL2FE_REQ_PC_ROCKET_STYLE_FIRE)?;

    let Some(weapon) = player.get_equipped()[EQUIP_SLOT_HAND as usize] else {
        return Err(FFError::build(
            Severity::Warning,
            "Tried to fire rocket but no weapon in hand".to_string(),
        ));
    };

    let weapon_data = weapon.get_stats()?;

    if weapon_data.target_mode != Some(5) {
        return Err(FFError::build(
            Severity::Warning,
            "Tried to fire rocket but holding wrong weapon type".to_string(),
        ));
    }

    let projectile = Projectile {
        projectile_kind: ProjectileKind::Rocket(weapon.id),
        single_power: player.get_single_power(),
        multi_power: player.get_multi_power(),
        charged: false,
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
            "Attempted to fire rocket but player has too many projectiles".to_string(),
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
    const EXPLOSION_RADIUS: u32 = 250; // todo pull from tdata
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let player = state.get_player_mut(pc_id)?;

    // Rocket and grenade hit are indentical
    let pkt: sP_CL2FE_REQ_PC_ROCKET_STYLE_HIT = *client.get_packet_unchecked()?;

    let Some(projectile) = player.remove_projectile(pkt.iBulletID) else {
        return Ok(());
    };

    let (target_ids, _weapon_boosts_needed) =
        get_targets(client, state, pkt.iTargetCnt as usize, None)?;
    let player = state.get_player(pc_id)?;

    // todo: validate
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
