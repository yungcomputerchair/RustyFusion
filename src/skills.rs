use std::{
    collections::{hash_map::Entry, HashMap},
    time::{Duration, Instant},
};

use rand::Rng;

use crate::{
    defines::*,
    entity::{Combatant, EntityID},
    enums::{BuffID, BuffType, CombatStyle, SkillShape, SkillType, TimeBuffUpdate},
    error::*,
    net::packet::{PacketID::*, *},
    state::ShardServerState,
};

impl Default for sTimeBuff {
    fn default() -> Self {
        Self {
            iTimeLimit: 0,
            iTimeDuration: 0,
            iTimeRepeat: 0,
            iValue: 0,
            iConfirmNum: 0,
        }
    }
}

#[derive(Debug)]
pub struct Skill {
    pub skill_type: SkillType,
    pub skill_shape: SkillShape,
    pub passive: bool,
    pub range: u32,
    pub values_a: [i32; SKILL_LEVEL_MAX + 1],
    pub values_b: [Option<i32>; SKILL_LEVEL_MAX + 1],
    pub values_c: [Option<i32>; SKILL_LEVEL_MAX + 1],
    pub costs: [i32; SKILL_LEVEL_MAX + 1],
    pub durations: [Option<Duration>; SKILL_LEVEL_MAX + 1],
}
impl Skill {
    pub fn get_buff_id(&self) -> Option<BuffID> {
        let buff_id = match self.skill_type {
            SkillType::Run => BuffID::UpMoveSpeed,
            SkillType::Jump => BuffID::UpJumpHeight,
            SkillType::Stealth => BuffID::UpStealth,
            SkillType::Phoenix => BuffID::Phoenix,
            SkillType::ProtectBattery => BuffID::ProtectBattery,
            SkillType::ProtectInfection => BuffID::ProtectInfection,
            SkillType::Snare => BuffID::DnMoveSpeed,
            SkillType::Sleep => BuffID::Sleep,
            SkillType::MiniMapEnemy => BuffID::MiniMapEnemy,
            SkillType::MiniMapTreasure => BuffID::MiniMapTreasure,
            SkillType::RewardBlob => BuffID::RewardBlob,
            SkillType::RewardCash => BuffID::RewardCash,
            SkillType::InfectionDamage => BuffID::Infection,
            SkillType::Freedom => BuffID::Freedom,
            SkillType::BoundingBall => BuffID::BoundingBall,
            SkillType::Invulnerable => BuffID::Invulnerable,
            SkillType::BuffHeal => BuffID::Heal,
            SkillType::NanoStimPak => BuffID::StimPakSlot1,
            _ => return None,
        };

        Some(buff_id)
    }

    pub fn make_buff_instance(&self, ty: BuffType, level: usize) -> FFResult<BuffInstance> {
        if level > SKILL_LEVEL_MAX {
            return Err(FFError::build(
                Severity::Warning,
                format!("Skill level {} is above max of {}", level, SKILL_LEVEL_MAX),
            ));
        }

        if self.get_buff_id().is_none() {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "Skill type {:?} does not have an associated buff",
                    self.skill_type
                ),
            ));
        }

        let value = self.values_a[level];
        let sub_value = self.values_b[level];
        let special_value = self.values_c[level];
        let duration = if self.passive {
            None
        } else {
            self.durations[level]
        };

        let buff = BuffInstance::new(ty, value, sub_value, special_value, duration);
        Ok(buff)
    }
}

#[derive(Debug)]
pub enum BuffUpdate {
    Added(BuffID, BuffType, sTimeBuff),
    Changed(BuffID, BuffType, sTimeBuff),
    Removed(BuffID),
}
impl From<BuffUpdate> for sP_FE2CL_PC_BUFF_UPDATE {
    fn from(update: BuffUpdate) -> Self {
        match update {
            BuffUpdate::Added(buff_id, source, time_buff) => Self {
                eTBU: TimeBuffUpdate::Add as i32,
                eTBT: source as i32,
                eCSTB: buff_id as i32,
                TimeBuff: time_buff,
                iConditionBitFlag: 0, // set by caller based on active buffs
            },
            BuffUpdate::Changed(buff_id, source, time_buff) => Self {
                eTBU: TimeBuffUpdate::Change as i32,
                eTBT: source as i32,
                eCSTB: buff_id as i32,
                TimeBuff: time_buff,
                iConditionBitFlag: 0, // set by caller based on active buffs
            },
            BuffUpdate::Removed(buff_id) => Self {
                eTBU: TimeBuffUpdate::Del as i32,
                eTBT: unused!(),
                eCSTB: buff_id as i32,
                TimeBuff: unused!(),
                iConditionBitFlag: 0, // set by caller based on active buffs
            },
        }
    }
}

#[derive(Debug)]
pub enum BuffEffect {
    HealEntity {
        target: EntityID,
        amount: i32,
    },
    DamageEntity {
        target: EntityID,
        source: Option<EntityID>,
        damage: i32,
    },
}

#[derive(Debug, Clone)]
pub struct BuffInstance {
    ty: BuffType,
    value: i32,
    _sub_value: Option<i32>,
    _special_value: Option<i32>,
    onset: Instant,
    expires: Option<Instant>,
    source: Option<EntityID>,
}
impl BuffInstance {
    pub fn new(
        ty: BuffType,
        value: i32,
        sub_value: Option<i32>,
        special_value: Option<i32>,
        duration: Option<Duration>,
    ) -> Self {
        let expires = duration.map(|d| Instant::now() + d);
        Self {
            ty,
            value,
            _sub_value: sub_value,
            _special_value: special_value,
            expires,
            onset: Instant::now(),
            source: None,
        }
    }

    fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires {
            Instant::now() >= expires
        } else {
            false
        }
    }

    fn tick(&mut self, _target: EntityID, _effects: &mut Vec<BuffEffect>) {
        // TODO individual buff tick
    }

    pub fn set_source(&mut self, source: EntityID) {
        self.source = Some(source);
    }
}

#[derive(Debug, Clone)]
struct BuffStack {
    buffs: Vec<BuffInstance>,
    applied: bool,
    changed: bool,
    remove: bool,
}
impl BuffStack {
    fn new(first_stack: BuffInstance) -> Self {
        Self {
            buffs: vec![first_stack],
            applied: false,
            changed: false,
            remove: false,
        }
    }

    fn add_stack(&mut self, buff: BuffInstance) {
        self.buffs.push(buff);
        self.changed = true;
    }

    fn remove_stacks(&mut self, buff_type: Option<BuffType>) {
        if let Some(buff_type) = buff_type {
            self.buffs.retain(|b| b.ty != buff_type);
        } else {
            self.buffs.clear();
        }
        self.changed = true;
    }

    fn has_stack(&self, buff_type: BuffType) -> bool {
        self.buffs.iter().any(|b| b.ty == buff_type)
    }

    fn tick(
        &mut self,
        buff_id: BuffID,
        target: EntityID,
        updates: &mut Vec<BuffUpdate>,
        effects: &mut Vec<BuffEffect>,
    ) {
        if !self.buffs.is_empty() {
            if !self.applied {
                self.on_stack_apply(buff_id, target);
                self.applied = true;
                self.changed = false;
                updates.push(BuffUpdate::Added(
                    buff_id,
                    self.get_dominant_type(),
                    (&*self).into(),
                ));
            }

            if self.changed {
                self.on_stack_change(buff_id, target);
                self.changed = false;
                updates.push(BuffUpdate::Changed(
                    buff_id,
                    self.get_dominant_type(),
                    (&*self).into(),
                ));
            }

            self.on_stack_tick(buff_id, target);
            self.buffs.iter_mut().for_each(|b| b.tick(target, effects));
            self.buffs.retain(|buff| !buff.is_expired());
        }

        if self.buffs.is_empty() {
            self.on_stack_remove(buff_id, target);
            self.remove = true;
            updates.push(BuffUpdate::Removed(buff_id));
        }
    }

    fn get_max_value(&self) -> i32 {
        self.buffs.iter().map(|b| b.value).max().unwrap_or(0)
    }

    fn get_dominant_type(&self) -> BuffType {
        self.buffs
            .iter()
            .max_by_key(|b| b.value)
            .map(|b| b.ty)
            .unwrap_or(BuffType::Nano)
    }

    fn get_expires(&self) -> Option<Instant> {
        // if any instance doesn't expire, then the whole buff doesn't expire.
        // otherwise, the buff expires when the last instance expires.
        if self.buffs.iter().any(|b| b.expires.is_none()) {
            None
        } else {
            self.buffs.iter().map(|b| b.expires.unwrap()).max()
        }
    }

    fn get_onset(&self) -> Instant {
        self.buffs
            .iter()
            .map(|b| b.onset)
            .min()
            .unwrap_or_else(Instant::now)
    }

    fn get_duration(&self) -> Option<Duration> {
        let onset = self.get_onset();
        self.get_expires()
            .map(|expires| expires.duration_since(onset))
    }

    fn on_stack_apply(&mut self, buff_id: BuffID, target: EntityID) {
        // do stuff
        log(
            Severity::Debug,
            &format!("Buff {:?} applied to {:?}", buff_id, target),
        );
    }

    fn on_stack_change(&mut self, buff_id: BuffID, target: EntityID) {
        // do stuff
        log(
            Severity::Debug,
            &format!("Buff {:?} changed on {:?}", buff_id, target),
        );
    }

    fn on_stack_remove(&mut self, buff_id: BuffID, target: EntityID) {
        // do stuff
        log(
            Severity::Debug,
            &format!("Buff {:?} removed from {:?}", buff_id, target),
        );
    }

    fn on_stack_tick(&mut self, buff_id: BuffID, target: EntityID) -> bool {
        // do stuff
        log(
            Severity::Debug,
            &format!("Buff {:?} ticked on {:?}", buff_id, target),
        );
        false
    }
}
impl From<&BuffStack> for sTimeBuff {
    fn from(stack: &BuffStack) -> Self {
        let now = Instant::now();
        Self {
            iTimeLimit: match stack.get_expires() {
                Some(expires) => expires.saturating_duration_since(now).as_millis() as u64,
                None => 0,
            },
            iTimeDuration: stack.get_duration().map_or(0, |d| d.as_millis() as u64),
            iTimeRepeat: unused!(),
            iValue: stack.get_max_value(),
            iConfirmNum: unused!(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct BuffContainer {
    buff_stacks: HashMap<BuffID, BuffStack>,
}
impl BuffContainer {
    pub fn add_buff(
        &mut self,
        buff_id: BuffID,
        mut buff: BuffInstance,
        source: Option<EntityID>,
    ) -> bool {
        if let Some(source) = source {
            buff.set_source(source);
        }

        match self.buff_stacks.entry(buff_id) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().add_stack(buff);
                false
            }
            Entry::Vacant(entry) => {
                entry.insert(BuffStack::new(buff));
                true
            }
        }
    }

    pub fn remove_buff(&mut self, buff_id: BuffID, buff_type: Option<BuffType>) -> bool {
        if let Some(stack) = self.buff_stacks.get_mut(&buff_id) {
            stack.remove_stacks(buff_type);
            true
        } else {
            false
        }
    }

    pub fn tick(&mut self, target: EntityID, effects: &mut Vec<BuffEffect>) -> Vec<BuffUpdate> {
        let mut updates = Vec::new();
        for (buff_id, buff_stack) in self.buff_stacks.iter_mut() {
            buff_stack.tick(*buff_id, target, &mut updates, effects);
        }

        self.buff_stacks.retain(|_, buff_stack| !buff_stack.remove);
        updates
    }

    pub fn has_buff(&self, buff_id: BuffID, buff_type: Option<BuffType>) -> bool {
        match buff_type {
            Some(buff_type) => self.buff_stacks.values().any(|s| s.has_stack(buff_type)),
            None => self.buff_stacks.contains_key(&buff_id),
        }
    }

    pub fn get_buff_value(&self, buff_id: BuffID) -> Option<i32> {
        self.buff_stacks
            .get(&buff_id)
            .map(|stack| stack.get_max_value())
    }

    pub fn get_bit_flags(&self) -> i32 {
        let mut flags = 0;
        for (buff_id, buff_stack) in &self.buff_stacks {
            if buff_stack.applied {
                flags |= 1 << (*buff_id as i32 - 1);
            }
        }
        flags
    }
}

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

    state.for_each_around(attacker_id, |c| {
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
    let dealt = to.take_damage(damage, Some(from));

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
