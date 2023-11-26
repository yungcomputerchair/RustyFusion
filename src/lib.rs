#[macro_use]
extern crate num_derive;

use std::{any::Any, hash::Hash};

use chunk::EntityMap;
use error::FFResult;
use net::{
    ffclient::FFClient,
    packet::{sItemBase, sNano, sRunningQuest},
    ClientMap,
};

#[macro_export]
macro_rules! unused {
    () => {
        Default::default()
    };
}

#[macro_export]
macro_rules! placeholder {
    ($val:expr) => {{
        #[cfg(debug_assertions)]
        println!("PLACEHOLDER: {} line {}", file!(), line!());
        $val
    }};
}

pub mod defines;
pub mod enums;
pub mod error;
pub mod net;
pub mod util;

pub mod tabledata;

pub mod chunk;
pub mod npc;
pub mod player;

#[derive(Debug, Copy, Clone, Default)]
pub struct Position {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}
impl Position {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub struct Item {
    ty: i16,
    id: i16,
    options: i32,
    expiry_time: i32,
}
impl Item {
    pub fn new(ty: i16, id: i16) -> Self {
        Self {
            ty,
            id,
            options: 1,
            expiry_time: 0,
        }
    }
}
impl From<sItemBase> for Option<Item> {
    fn from(value: sItemBase) -> Self {
        if value.iID == 0 || value.iOpt == 0 {
            None
        } else {
            Some(Item {
                ty: value.iType,
                id: value.iID,
                options: value.iOpt,
                expiry_time: value.iTimeLimit,
            })
        }
    }
}
impl From<Option<Item>> for sItemBase {
    fn from(value: Option<Item>) -> Self {
        if let Some(value) = value {
            Self {
                iType: value.ty,
                iID: value.id,
                iOpt: value.options,
                iTimeLimit: value.expiry_time,
            }
        } else {
            Self {
                iType: 0,
                iID: 0,
                iOpt: 0,
                iTimeLimit: 0,
            }
        }
    }
}

#[derive(Debug, Copy, Clone, Default)]
struct Nano {
    id: i16,
    skill_id: i16,
    stamina: i16,
}
impl From<Nano> for sNano {
    fn from(value: Nano) -> Self {
        Self {
            iID: value.id,
            iSkillID: value.skill_id,
            iStamina: value.stamina,
        }
    }
}

#[derive(Debug, Copy, Clone, Default)]
struct Mission {
    id: i32,
    target_npc_ids: [i32; 3],
    target_npc_counts: [i32; 3],
    target_item_ids: [i32; 3],
    target_item_counts: [i32; 3],
}
impl From<Mission> for sRunningQuest {
    fn from(value: Mission) -> Self {
        Self {
            m_aCurrTaskID: value.id,
            m_aKillNPCID: value.target_npc_ids,
            m_aKillNPCCount: value.target_npc_counts,
            m_aNeededItemID: value.target_item_ids,
            m_aNeededItemCount: value.target_item_counts,
        }
    }
}
impl From<Option<Mission>> for sRunningQuest {
    fn from(value: Option<Mission>) -> Self {
        if let Some(mission) = value {
            return mission.into();
        }

        Self {
            m_aCurrTaskID: 0,
            m_aKillNPCID: [0, 0, 0],
            m_aKillNPCCount: [0, 0, 0],
            m_aNeededItemID: [0, 0, 0],
            m_aNeededItemCount: [0, 0, 0],
        }
    }
}

#[derive(Eq, Hash, PartialEq, Clone, Copy, Debug)]
pub enum EntityID {
    Player(i64),
    NPC(i32),
}

pub trait Entity {
    fn get_id(&self) -> EntityID;
    fn get_client<'a>(&self, client_map: &'a mut ClientMap) -> Option<&'a mut FFClient>;
    fn get_position(&self) -> Position;
    fn set_position(
        &mut self,
        pos: Position,
        entity_map: &mut EntityMap,
        client_map: &mut ClientMap,
    );
    fn set_rotation(&mut self, angle: i32);
    fn send_enter(&self, client: &mut FFClient) -> FFResult<()>;
    fn send_exit(&self, client: &mut FFClient) -> FFResult<()>;

    fn as_any(&mut self) -> &mut dyn Any;
}

#[derive(Debug, Copy, Clone, Default)]
struct CombatStats {
    level: i16,
    _max_hp: i32,
    hp: i32,
}

pub trait Combatant {
    fn get_condition_bit_flag(&self) -> i32;
    fn get_level(&self) -> i16;
    fn get_hp(&self) -> i32;
}
