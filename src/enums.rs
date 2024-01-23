#![allow(non_camel_case_types)]

use num_enum::TryFromPrimitive;

use crate::{defines::*, error::FFError};

#[repr(i32)]
#[derive(PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
#[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
pub enum NanoStyle {
    Adaptium = NANO_STYLE_CRYSTAL as i32,
    Blastons = NANO_STYLE_ENERGY as i32,
    Cosmix = NANO_STYLE_FLUID as i32,
}

#[repr(i16)]
#[derive(PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
#[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
pub enum PlayerGuide {
    Edd = 1,
    Dexter = 2,
    Mojo = 3,
    Ben = 4,
    Computress = 5,
}

#[repr(i8)]
#[derive(PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
#[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
pub enum PlayerShardStatus {
    Entered = 0,
    Exited = 1,
}

#[repr(u8)]
#[derive(PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
#[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
pub enum ShardChannelStatus {
    Closed = 0,
    Empty = 1,
    Normal = 2,
    Busy = 3,
}

#[repr(i8)]
#[derive(PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
#[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
pub enum AreaType {
    Local = 0,
    Channel = 1,
    Shard = 2,
    Global = 3,
}

#[repr(i32)]
#[derive(PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
#[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
pub enum TargetSearchBy {
    PlayerID = 0,
    PlayerName = 1,
    PlayerUID = 2,
}

#[repr(i32)]
#[derive(PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
#[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
pub enum RewardType {
    Taros = 0,
    FusionMatter = 1,
}

/* Enums ripped from the client */

#[repr(i32)]
#[derive(PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
#[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
pub enum ItemLocation {
    Equip = 0,  /*eIL_Equip*/
    Inven = 1,  /*eIL_Inven*/
    QInven = 2, /*eIL_QInven*/
    Bank = 3,   /*eIL_Bank*/
                /*eIL__End*/
}
impl ItemLocation {
    pub fn end() -> i32 {
        4
    }
}

#[repr(i16)]
#[derive(PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
#[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
pub enum ItemType {
    Hand = 0,              /*eItemType_Hand*/
    UpperBody = 1,         /*eItemType_UpperBody*/
    LowerBody = 2,         /*eItemType_LowerBody*/
    Foot = 3,              /*eItemType_Foot*/
    Head = 4,              /*eItemType_Head*/
    Face = 5,              /*eItemType_Face*/
    Back = 6,              /*eItemType_Back*/
    General = 7,           /*eItemType_General*/
    Quest = 8,             /*eItemType_Quest*/
    Chest = 9,             /*eItemType_Chest*/
    Vehicle = 10,          /*eItemType_Vehicle*/
    GMKey = 11,            /*eItemType_GMKey*/
    FMatter = 12,          /*eItemType_FMatter*/
    Hair = 13,             /*eItemType_Hair*/
    SkinFace = 14,         /*eItemType_SkinFace*/
    Nano = 19,             /*eItemType_Nano*/
    NanoTune = 24,         /*eItemType_NanoTune*/
    Skill = 27,            /*eItemType_Skill*/
    Npc = 30,              /*eItemType_Npc*/
    SkillBuffEffect = 138, /*eItemType_SkillBuffEffect*/
}

#[repr(i32)]
#[derive(PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
#[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
pub enum TransportationType {
    /*eTT_None*/
    Warp = 1,   /*eTT_Warp*/
    Wyvern = 2, /*eTT_Wyvern*/
    Bus = 3,    /*eTT_Bus*/
                /*eTT__End*/
}

#[repr(i32)]
#[derive(PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
#[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
pub enum TeleportType {
    XYZ = 0,             /*eCN_GM_TeleportMapType__XYZ*/
    MapXYZ = 1,          /*eCN_GM_TeleportMapType__MapXYZ*/
    MyLocation = 2,      /*eCN_GM_TeleportMapType__MyLocation*/
    SomeoneLocation = 3, /*eCN_GM_TeleportMapType__SomeoneLocation*/
    Unstick = 4,         /*eCN_GM_TeleportMapType__Unstick*/
}
