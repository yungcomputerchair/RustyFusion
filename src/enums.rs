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

/* Enums ripped from the client */

#[repr(i32)]
#[derive(PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
#[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
pub enum ItemLocation {
    /*eIL_Equip*/ Equip = 0,
    /*eIL_Inven*/ Inven = 1,
    /*eIL_QInven*/ QInven = 2,
    /*eIL_Bank*/ Bank = 3,
    /*eIL__End*/
}

#[repr(i16)]
#[derive(PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
#[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
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

#[repr(i32)]
#[derive(PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
#[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
pub enum TransportationType {
    /*eTT_None*/
    /*eTT_Warp*/ Warp = 1,
    /*eTT_Wyvern*/ Wyvern = 2,
    /*eTT_Bus*/ Bus = 3,
    /*eTT__End*/
}
