#![allow(non_camel_case_types)]

use num_enum::TryFromPrimitive;
use serde_repr::{Deserialize_repr, Serialize_repr};

use crate::{defines::*, error::FFError};

macro_rules! ffenum {
    ($name:ident, $ty:ty, { $($variant:ident = $val:expr,)* }) => {
        #[repr($ty)]
        #[derive(Serialize_repr, Deserialize_repr, PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
        #[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
        pub enum $name {
            $($variant = $val,)*
        }
    };
    ($name:ident, $ty:ty, $end:expr, { $($variant:ident = $val:expr,)* }) => {
        #[repr($ty)]
        #[derive(Serialize_repr, Deserialize_repr, PartialEq, Eq, Hash, TryFromPrimitive, Clone, Copy, Debug)]
        #[num_enum(error_type(name = FFError, constructor = FFError::from_enum_err))]
        pub enum $name {
            $($variant = $val,)*
        }
        impl $name {
            pub fn end() -> $ty {
                $end
            }
        }
    };
}

ffenum!(CombatStyle, i32, {
    Adaptium = NANO_STYLE_CRYSTAL as i32,
    Blastons = NANO_STYLE_ENERGY as i32,
    Cosmix = NANO_STYLE_FLUID as i32,
});

ffenum!(PlayerGuide, i16, {
    Edd = 1,
    Dexter = 2,
    Mojo = 3,
    Ben = 4,
    Computress = 5,
});

ffenum!(PlayerNameStatus, i8, {
    Pending = 0,
    Approved = 1,
    Denied = 2,
});

ffenum!(PlayerShardStatus, i8, {
    Entered = 0,
    Exited = 1,
});

ffenum!(ShardChannelStatus, u8, {
    Closed = 0,
    Empty = 1,
    Normal = 2,
    Busy = 3,
});

ffenum!(AreaType, i8, {
    Local = 0,
    Channel = 1,
    Shard = 2,
    Global = 3,
});

ffenum!(TargetSearchBy, i32, {
    PlayerID = 0,
    PlayerName = 1,
    PlayerUID = 2,
});

ffenum!(CombatantTeam, i32, {
    Unknown = 0,
    Friendly = 1,
    Mob = 2,
});

// eCharType.cs
ffenum!(CharType, i32, 5, {
    Unknown = 0,
    Player = 1,
    NPC = 2,
    Mob = 3,
});

// eTaskTypeProperty.cs
ffenum!(TaskType, i32, 7, {
    Unknown = 0,
    Talk = 1,
    GotoLocation = 2,
    UseItems = 3,
    Delivery = 4,
    Defeat = 5,
    EscortDefence = 6,
});

// eMissionTypeProperty.cs
ffenum!(MissionType, i32, 4, {
    Unknown = 0,
    Guide = 1,
    Nano = 2,
    Normal = 3,
});

ffenum!(RewardType, i32, {
    Taros = 0,
    FusionMatter = 1,
});

ffenum!(RewardCategory, usize, {
    All = 0,
    Combat = 1,
    Missions = 2,
    Eggs = 3,
    Racing = 4,
});

// eItemLocation.cs
ffenum!(ItemLocation, i32, 4, {
    Equip = 0,  /*eIL_Equip*/
    Inven = 1,  /*eIL_Inven*/
    QInven = 2, /*eIL_QInven*/
    Bank = 3,   /*eIL_Bank*/
    /*eIL__End*/
});

// eItemType.cs
ffenum!(ItemType, i16, {
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
});

// eTransportationType.cs
ffenum!(TransportationType, i32, {
    /*eTT_None*/
    Warp = 1,   /*eTT_Warp*/
    Wyvern = 2, /*eTT_Wyvern*/
    Bus = 3,    /*eTT_Bus*/
    /*eTT__End*/
});

// eCN_GM_TeleportType.cs
ffenum!(TeleportType, i32, {
    XYZ = 0,             /*eCN_GM_TeleportMapType__XYZ*/
    MapXYZ = 1,          /*eCN_GM_TeleportMapType__MapXYZ*/
    MyLocation = 2,      /*eCN_GM_TeleportMapType__MyLocation*/
    SomeoneLocation = 3, /*eCN_GM_TeleportMapType__SomeoneLocation*/
    Unstick = 4,         /*eCN_GM_TeleportMapType__Unstick*/
});

// eRideType.cs
ffenum!(RideType, i32, 2, {
    None = 0,   /*eRT_None*/
    Wyvern = 1, /*eRT_Wyvern*/
    /*eRT__End*/
});

// ePCRegenType.cs
ffenum!(PCRegenType, i32, 7, {
    None = 0,               /*ePCRegenType_None*/
    Xcom = 1,               /*ePCRegenType_Xcom*/
    Here = 2,               /*ePCRegenType_Here*/
    HereByPhoenix = 3,      /*ePCRegenType_HereByPhoenix*/
    HereByPhoenixGroup = 4, /*ePCRegenType_HereByPhoenixGroup*/
    Unstick = 5,            /*ePCRegenType_Unstick*/
    HereByPhoenixItem = 6,  /*ePCRegenType_HereByPhoenixItem*/
    /* ePCRegenType__End */
});
