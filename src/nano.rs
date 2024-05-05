use crate::{defines::*, enums::*, error::FFResult, net::packet::*, tabledata::tdata_get};

#[derive(Debug, Clone)]
pub struct Nano {
    id: i16,
    pub selected_skill: Option<i16>,
    pub stamina: i16,
}
impl Nano {
    pub fn new(id: i16) -> Self {
        Self {
            id,
            selected_skill: None,
            stamina: NANO_STAMINA_MAX,
        }
    }

    pub fn get_id(&self) -> i16 {
        self.id
    }

    pub fn tune(&mut self, skill: Option<i16>) {
        self.selected_skill = skill;
    }

    pub fn get_stats(&self) -> FFResult<&NanoStats> {
        tdata_get().get_nano_stats(self.id)
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
        };
        Some(nano)
    }
}
impl From<Option<Nano>> for sNano {
    fn from(value: Option<Nano>) -> Self {
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
