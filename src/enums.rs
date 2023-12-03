#![allow(non_camel_case_types)]

use crate::error::{FFError, FFResult, Severity};
use num_traits::FromPrimitive;

#[repr(i32)]
#[derive(PartialEq, FromPrimitive, Clone, Copy, Debug)]
pub enum ItemLocation {
    Equip,
    Inven,
    QInven,
    Bank,
}
impl TryFrom<i32> for ItemLocation {
    type Error = FFError;
    fn try_from(value: i32) -> FFResult<Self> {
        Self::from_i32(value).ok_or(FFError::build(
            Severity::Warning,
            format!("Invalid ItemLocation value {}", value),
        ))
    }
}

#[repr(i16)]
#[derive(PartialEq, FromPrimitive, Clone, Copy, Debug)]
pub enum ItemType {
    Hand = 0,
    UpperBody = 1,
    LowerBody = 2,
    Foot = 3,
    Head = 4,
    Face = 5,
    Back = 6,
    General = 7,
    Quest = 8,
    Chest = 9,
    Vehicle = 10,
    GMKey = 11,
    FMatter = 12,
    Hair = 13,
    SkinFace = 14,
    Nano = 19,
    NanoTune = 24,
    Skill = 27,
    Npc = 30,
    SkillBuffEffect = 138,
}
impl TryFrom<i16> for ItemType {
    type Error = FFError;
    fn try_from(value: i16) -> FFResult<Self> {
        Self::from_i16(value).ok_or(FFError::build(
            Severity::Warning,
            format!("Invalid ItemType value {}", value),
        ))
    }
}
