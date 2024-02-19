use crate::{
    defines::*,
    enums::*,
    error::{FFError, FFResult, Severity},
    net::packet::*,
    tabledata::tdata_get,
};

#[derive(Debug, Clone)]
pub struct Nano {
    id: i16,
    pub selected_skill: Option<usize>,
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

    pub fn get_stats(&self) -> FFResult<&NanoStats> {
        tdata_get().get_nano_stats(self.id)
    }
}
impl TryFrom<sNano> for Option<Nano> {
    type Error = FFError;
    fn try_from(value: sNano) -> FFResult<Self> {
        if value.iID == 0 {
            return Ok(None);
        }

        let skill = if value.iSkillID == 0 {
            None
        } else {
            let stats = tdata_get().get_nano_stats(value.iID)?;
            Some(
                stats
                    .skills
                    .iter()
                    .position(|&skill| skill == value.iSkillID)
                    .ok_or(FFError::build(
                        Severity::Warning,
                        format!("Skill id {} invalid for nano {}", value.iSkillID, value.iID),
                    ))?,
            )
        };

        let nano = Nano {
            id: value.iID,
            selected_skill: skill,
            stamina: value.iStamina,
        };
        Ok(Some(nano))
    }
}
impl From<Option<Nano>> for sNano {
    fn from(value: Option<Nano>) -> Self {
        match value {
            Some(nano) => Self {
                iID: nano.id,
                iSkillID: match nano.selected_skill {
                    Some(skill_idx) => {
                        let stats = nano.get_stats().unwrap();
                        stats.skills[skill_idx]
                    }
                    None => 0,
                },
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
impl Nano {
    pub fn tune(&mut self, skill_idx: Option<usize>) {
        self.selected_skill = skill_idx;
    }
}

#[derive(Debug)]
pub struct NanoStats {
    pub style: NanoStyle,
    pub skills: [i16; SIZEOF_NANO_SKILLS],
}

#[derive(Debug)]
pub struct NanoTuning {
    pub fusion_matter_cost: u32,
    pub req_item_id: i16,
    pub req_item_quantity: u16,
    pub skill_id: i16,
}
