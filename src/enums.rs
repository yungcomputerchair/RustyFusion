#![allow(non_camel_case_types)]

use crate::{
    defines::*,
    error::{FFError, FFResult, Severity},
};
use num_traits::FromPrimitive;

#[repr(i32)]
#[derive(PartialEq, Eq, Hash, FromPrimitive, Clone, Copy, Debug)]
pub enum NanoStyle {
    Adaptium = NANO_STYLE_CRYSTAL as i32,
    Blastons = NANO_STYLE_ENERGY as i32,
    Cosmix = NANO_STYLE_FLUID as i32,
}
impl TryFrom<i32> for NanoStyle {
    type Error = FFError;
    fn try_from(value: i32) -> FFResult<Self> {
        Self::from_i32(value).ok_or(FFError::build(
            Severity::Warning,
            format!("Invalid NanoStyle value {}", value),
        ))
    }
}

/* Enums ripped from the client */

#[repr(i32)]
#[derive(PartialEq, Eq, Hash, FromPrimitive, Clone, Copy, Debug)]
pub enum ItemLocation {
    /*eIL_Equip*/ Equip = 0,
    /*eIL_Inven*/ Inven = 1,
    /*eIL_QInven*/ QInven = 2,
    /*eIL_Bank*/ Bank = 3,
    /*eIL__End*/
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
#[derive(PartialEq, Eq, Hash, FromPrimitive, Clone, Copy, Debug)]
pub enum ItemType {
    /*eItemType_Hand*/ Hand = 0,
    /*eItemType_UpperBody*/ UpperBody = 1,
    /*eItemType_LowerBody*/ LowerBody = 2,
    /*eItemType_Foot*/ Foot = 3,
    /*eItemType_Head*/ Head = 4,
    /*eItemType_Face*/ Face = 5,
    /*eItemType_Back*/ Back = 6,
    /*eItemType_General*/ General = 7,
    /*eItemType_Quest*/ Quest = 8,
    /*eItemType_Chest*/ Chest = 9,
    /*eItemType_Vehicle*/ Vehicle = 10,
    /*eItemType_GMKey*/ GMKey = 11,
    /*eItemType_FMatter*/ FMatter = 12,
    /*eItemType_Hair*/ Hair = 13,
    /*eItemType_SkinFace*/ SkinFace = 14,
    /*eItemType_Nano*/ Nano = 19,
    /*eItemType_NanoTune*/ NanoTune = 24,
    /*eItemType_Skill*/ Skill = 27,
    /*eItemType_Npc*/ Npc = 30,
    /*eItemType_SkillBuffEffect*/ SkillBuffEffect = 138,
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

#[repr(i32)]
#[derive(PartialEq, Eq, Hash, FromPrimitive, Clone, Copy, Debug)]
pub enum TransportationType {
    /*eTT_None*/
    /*eTT_Warp*/ Warp = 1,
    /*eTT_Wyvern*/ Wyvern = 2,
    /*eTT_Bus*/ Bus = 3,
    /*eTT__End*/
}
impl TryFrom<i32> for TransportationType {
    type Error = FFError;
    fn try_from(value: i32) -> FFResult<Self> {
        Self::from_i32(value).ok_or(FFError::build(
            Severity::Warning,
            format!("Invalid TransportationType value {}", value),
        ))
    }
}
