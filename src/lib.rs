#[macro_use]
extern crate num_derive;

use std::{error::Error, result};

use net::packet::{sItemBase, sNano, sRunningQuest};

pub type Result<T> = result::Result<T, Box<dyn Error>>;

pub const CN_PACKET_BUFFER_SIZE: usize = 4096;

pub mod error;
pub mod net;
pub mod player;
pub mod util;

#[derive(Debug, Copy, Clone, Default)]
struct Position {
    x: i32,
    y: i32,
    z: i32,
}

#[derive(Debug, Copy, Clone, Default)]
struct Item {
    ty: i16,
    id: i16,
    options: i32,
    expiry_time: i32,
}
impl From<Item> for sItemBase {
    fn from(value: Item) -> Self {
        Self {
            iType: value.ty,
            iID: value.id,
            iOpt: value.options,
            iTimeLimit: value.expiry_time,
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

#[derive(Debug, Copy, Clone)]
struct CombatStats {
    level: i16,
    _max_hp: i32,
    hp: i32,
}

trait Combatant {
    fn get_condition_bit_flag(&self) -> i32;
    fn get_combat_stats(&self) -> CombatStats;
}
