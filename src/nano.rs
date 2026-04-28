use std::time::{Duration, Instant};

use crate::{
    defines::*,
    enums::*,
    error::{log_if_failed, FFResult},
    net::packet::*,
    skills::Skill,
    tabledata::tdata_get,
    util::clamp,
};

#[derive(Debug, Clone)]
pub struct Nano {
    id: i16,
    pub selected_skill: Option<i16>,
    stamina: i16,
    last_regen: Option<Instant>,
    last_wear: Option<Instant>,
}
impl Nano {
    pub fn new(id: i16) -> Self {
        Self {
            id,
            selected_skill: None,
            stamina: NANO_STAMINA_MAX,
            last_regen: None,
            last_wear: None,
        }
    }

    pub fn get_id(&self) -> i16 {
        self.id
    }

    pub fn get_stamina(&self) -> i16 {
        self.stamina
    }

    pub fn set_stamina(&mut self, stamina: i16) {
        self.stamina = clamp(stamina, 0, NANO_STAMINA_MAX);
    }

    pub fn tune(&mut self, skill: Option<i16>) {
        self.selected_skill = skill;
    }

    pub fn get_stats(&self) -> FFResult<&NanoStats> {
        tdata_get().get_nano_stats(self.id)
    }

    pub fn get_skill(&self) -> Option<&'static Skill> {
        self.selected_skill
            .and_then(|id| log_if_failed(tdata_get().get_skill(id)))
    }

    pub fn tick_regen(&mut self) -> bool {
        const STAMINA_REGEN_INTERVAL: Duration = Duration::from_secs(2);
        const STAMINA_REGEN_HEAL: i16 = 1;

        if self.stamina == NANO_STAMINA_MAX {
            return false;
        }

        let now = Instant::now();
        if self
            .last_regen
            .is_some_and(|last| now - last < STAMINA_REGEN_INTERVAL)
        {
            return false;
        }

        let new_stamina = self.stamina + STAMINA_REGEN_HEAL;
        self.set_stamina(new_stamina);
        self.last_regen = Some(now);
        true
    }

    pub fn tick_wear(&mut self, level: usize) -> bool {
        const STAMINA_WEAR_INTERVAL: Duration = Duration::from_secs(2);
        const STAMINA_WEAR_BASE_RATE: f32 = 1.0;
        const STAMINA_WEAR_PASSIVE_FACTOR: f32 = 0.2;

        let now = Instant::now();
        if self
            .last_wear
            .is_some_and(|last| now - last < STAMINA_WEAR_INTERVAL)
        {
            return false;
        }

        let mut wear_rate = STAMINA_WEAR_BASE_RATE;
        if let Some(skill) = log_if_none!(self.get_skill()) {
            if skill.passive {
                let skill_wear_rate = skill.costs[level];
                wear_rate += STAMINA_WEAR_PASSIVE_FACTOR * skill_wear_rate as f32;
            }
        }

        let cost = wear_rate as i16;
        let new_stamina = self.stamina - cost;
        self.set_stamina(new_stamina);
        self.last_wear = Some(now);
        true
    }
}
impl From<sNano> for Option<Nano> {
    fn from(value: sNano) -> Self {
        if value.iID == 0 {
            return None;
        }

        let skill = match value.iSkillID {
            0 => None,
            id => Some(id),
        };

        let nano = Nano {
            id: value.iID,
            selected_skill: skill,
            stamina: value.iStamina,
            last_regen: None,
            last_wear: None,
        };
        Some(nano)
    }
}
impl From<Option<&Nano>> for sNano {
    fn from(value: Option<&Nano>) -> Self {
        match value {
            Some(nano) => Self {
                iID: nano.id,
                iSkillID: nano.selected_skill.unwrap_or(0),
                iStamina: nano.stamina,
            },
            None => sNano {
                iID: 0,
                iSkillID: 0,
                iStamina: 0,
            },
        }
    }
}

#[derive(Debug)]
pub struct NanoStats {
    pub style: CombatStyle,
    pub skills: [i16; SIZEOF_NANO_SKILLS],
}

#[derive(Debug)]
pub struct NanoTuning {
    pub fusion_matter_cost: u32,
    pub req_item_id: i16,
    pub req_item_quantity: u16,
    pub skill_id: i16,
}
