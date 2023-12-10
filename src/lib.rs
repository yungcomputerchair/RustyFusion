#![allow(clippy::derivable_impls)]

#[macro_use]
extern crate num_derive;

use std::{any::Any, cmp::min, collections::HashMap, hash::Hash, time::SystemTime};

use defines::SIZEOF_VENDOR_TABLE_SLOT;
use enums::ItemType;
use error::{FFError, FFResult, Severity};
use net::{
    ffclient::FFClient,
    packet::{sItemBase, sItemVendor, sNano, sRunningQuest},
    ClientMap,
};
use state::shard::ShardServerState;
use tabledata::tdata_get;

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
pub mod state;
pub mod timer;
pub mod util;

pub mod config;
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

    pub fn chunk_coords(&self) -> (i32, i32) {
        let chunk_x = (self.x * chunk::NCHUNKS as i32) / chunk::MAP_BOUNDS;
        let chunk_y = (self.y * chunk::NCHUNKS as i32) / chunk::MAP_BOUNDS;
        (chunk_x, chunk_y)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Item {
    ty: ItemType,
    id: i16,
    appearance_id: Option<i16>,
    quantity: u16,
    expiry_time: Option<SystemTime>,
}
impl Item {
    pub fn new(ty: ItemType, id: i16) -> Self {
        Self {
            ty,
            id,
            appearance_id: None,
            quantity: 1,
            expiry_time: None,
        }
    }

    pub fn get_id(&self) -> i16 {
        self.id
    }

    pub fn get_type(&self) -> ItemType {
        self.ty
    }

    pub fn get_quantity(&self) -> u16 {
        self.quantity
    }

    pub fn get_stats(&self) -> FFResult<&ItemStats> {
        tdata_get().get_item_stats(self.id, self.ty)
    }

    pub fn set_expiry_time(&mut self, time: SystemTime) {
        self.expiry_time = Some(time);
    }

    pub fn set_appearance(&mut self, looks_item: &Item) {
        self.appearance_id = Some(looks_item.id);
    }

    pub fn split_items(from: &mut Option<Item>, mut quantity: u16) -> Option<Item> {
        if from.is_none() || quantity == 0 {
            return None;
        }

        let from_stack = from.as_mut().unwrap();
        quantity = min(quantity, from_stack.quantity);
        from_stack.quantity -= quantity;

        let mut result_stack = *from_stack;
        result_stack.quantity = quantity;

        if from_stack.quantity == 0 {
            *from = None;
        }
        Some(result_stack)
    }

    pub fn transfer_items(from: &mut Option<Item>, to: &mut Option<Item>) -> FFResult<()> {
        if from.is_none() {
            return Ok(());
        }

        if to.is_none() {
            *to = *from;
            *from = None;
            return Ok(());
        }

        let (from_stack, to_stack) = (from.as_mut().unwrap(), to.as_mut().unwrap());
        if from_stack.id != to_stack.id || from_stack.ty != to_stack.ty {
            std::mem::swap(from, to);
            return Ok(());
        }

        let max_stack_size = to_stack.get_stats()?.max_stack_size;
        let num_to_move = min(max_stack_size - to_stack.quantity, from_stack.quantity);
        to_stack.quantity += num_to_move;
        from_stack.quantity -= num_to_move;
        if from_stack.quantity == 0 {
            *from = None;
        }
        Ok(())
    }
}
impl Default for sItemBase {
    fn default() -> Self {
        Self {
            iType: 0,
            iID: 0,
            iOpt: 0,
            iTimeLimit: 0,
        }
    }
}
impl TryFrom<sItemBase> for Option<Item> {
    type Error = FFError;
    fn try_from(value: sItemBase) -> FFResult<Self> {
        if value.iID == 0 || value.iOpt == 0 {
            Ok(None)
        } else {
            Ok(Some(Item {
                ty: value.iType.try_into()?,
                id: value.iID,
                appearance_id: {
                    let id = (value.iOpt >> 16) as i16;
                    if id == 0 {
                        None
                    } else {
                        Some(id)
                    }
                },
                quantity: value.iOpt as u16,
                expiry_time: if value.iTimeLimit == 0 {
                    None
                } else {
                    Some(util::get_systime_from_sec(value.iTimeLimit as u64))
                },
            }))
        }
    }
}
impl From<Option<Item>> for sItemBase {
    fn from(value: Option<Item>) -> Self {
        if let Some(value) = value {
            Self {
                iType: value.ty as i16,
                iID: value.id,
                iOpt: (value.quantity as i32) | ((value.appearance_id.unwrap_or(0) as i32) << 16),
                iTimeLimit: match value.expiry_time {
                    Some(time) => util::get_timestamp_sec(time) as i32,
                    None => 0,
                },
            }
        } else {
            Self::default()
        }
    }
}

pub struct ItemStats {
    pub buy_price: u32,
    pub sell_price: u32,
    pub sellable: bool,
    pub tradeable: bool,
    pub max_stack_size: u16,
    pub required_level: i16,
    pub rarity: Option<i8>,
    pub gender: Option<i8>,
}

pub struct VendorItem {
    sort_number: i32,
    ty: ItemType,
    id: i16,
}

pub struct VendorData {
    vendor_id: i32,
    items: Vec<VendorItem>,
}
impl VendorData {
    fn new(vendor_id: i32) -> Self {
        Self {
            vendor_id,
            items: Vec::new(),
        }
    }

    fn insert(&mut self, item: VendorItem) {
        self.items.push(item);
    }

    pub fn as_arr(&self) -> FFResult<[sItemVendor; SIZEOF_VENDOR_TABLE_SLOT as usize]> {
        let mut vendor_item_structs = Vec::new();
        for item in &self.items {
            vendor_item_structs.push(sItemVendor {
                iVendorID: self.vendor_id,
                fBuyCost: tdata_get().get_item_stats(item.id, item.ty)?.buy_price as f32,
                item: sItemBase {
                    iType: item.ty as i16,
                    iID: item.id,
                    iOpt: 1,
                    iTimeLimit: 0,
                },
                iSortNum: item.sort_number,
            });
        }
        vendor_item_structs.resize(
            SIZEOF_VENDOR_TABLE_SLOT as usize,
            sItemVendor {
                iVendorID: 0,
                fBuyCost: 0.0,
                item: sItemBase::default(),
                iSortNum: 0,
            },
        );
        Ok(vendor_item_structs.try_into().unwrap())
    }

    pub fn has_item(&self, item_id: i16, item_type: ItemType) -> bool {
        self.items
            .iter()
            .any(|item| item_id == item.id && item_type == item.ty)
    }
}

#[derive(Default, Clone, Copy)]
struct TradeItem {
    pub inven_slot_num: usize,
    pub quantity: u16,
}
#[derive(Default, Clone, Copy)]
struct TradeOffer {
    taros: u32,
    items: [Option<TradeItem>; 5],
    confirmed: bool,
}
impl TradeOffer {
    fn get_count(&self, inven_slot_num: usize) -> u16 {
        let mut quantity = 0;
        for trade_item in self.items.iter().flatten() {
            if trade_item.inven_slot_num == inven_slot_num {
                quantity += trade_item.quantity;
            }
        }
        quantity
    }

    fn add_item(
        &mut self,
        trade_slot_num: usize,
        inven_slot_num: usize,
        quantity: u16,
    ) -> FFResult<u16> {
        if trade_slot_num >= self.items.len() {
            return Err(FFError::build(
                Severity::Warning,
                format!("Trade slot number {} out of range", trade_slot_num),
            ));
        }

        self.items[trade_slot_num] = Some(TradeItem {
            inven_slot_num,
            quantity,
        });

        Ok(self.get_count(inven_slot_num))
    }

    fn remove_item(&mut self, trade_slot_num: usize) -> FFResult<(u16, usize)> {
        if trade_slot_num >= self.items.len() {
            return Err(FFError::build(
                Severity::Warning,
                format!("Trade slot number {} out of range", trade_slot_num),
            ));
        }

        if self.items[trade_slot_num].is_none() {
            return Err(FFError::build(
                Severity::Warning,
                format!("Nothing in trade slot {}", trade_slot_num),
            ));
        }

        let removed_item = self.items[trade_slot_num].take().unwrap();
        Ok((
            self.get_count(removed_item.inven_slot_num),
            removed_item.inven_slot_num,
        ))
    }
}
pub struct TradeContext {
    offers: HashMap<i32, TradeOffer>,
}
impl TradeContext {
    pub fn new(pc_ids: [i32; 2]) -> Self {
        let mut offers = HashMap::new();
        offers.insert(pc_ids[0], TradeOffer::default());
        offers.insert(pc_ids[1], TradeOffer::default());
        Self { offers }
    }

    pub fn get_other_id(&self, pc_id: i32) -> i32 {
        for id in self.offers.keys() {
            if *id != pc_id {
                return *id;
            }
        }
        panic!("Bad trade state");
    }

    fn get_offer_mut(&mut self, pc_id: i32) -> FFResult<&mut TradeOffer> {
        self.offers.get_mut(&pc_id).ok_or(FFError::build(
            Severity::Warning,
            format!("Player {} is not a part of the trade", pc_id),
        ))
    }

    pub fn set_taros(&mut self, pc_id: i32, taros: u32) -> FFResult<()> {
        let offer = self.get_offer_mut(pc_id)?;
        offer.taros = taros;
        offer.confirmed = false;
        Ok(())
    }

    pub fn add_item(
        &mut self,
        pc_id: i32,
        trade_slot_num: usize,
        inven_slot_num: usize,
        quantity: u16,
    ) -> FFResult<u16> {
        let offer = self.get_offer_mut(pc_id)?;
        offer.confirmed = false;
        offer.add_item(trade_slot_num, inven_slot_num, quantity)
    }

    pub fn remove_item(&mut self, pc_id: i32, trade_slot_num: usize) -> FFResult<(u16, usize)> {
        let offer = self.get_offer_mut(pc_id)?;
        offer.confirmed = false;
        offer.remove_item(trade_slot_num)
    }

    fn is_ready(&self) -> bool {
        self.offers.iter().all(|(_, offer)| offer.confirmed)
    }

    pub fn lock_in(&mut self, pc_id: i32) -> FFResult<bool> {
        let offer = self.get_offer_mut(pc_id)?;
        offer.confirmed = true;
        Ok(self.is_ready())
    }
}

pub struct CrocPotData {
    pub base_chance: f32,
    pub rarity_diff_multipliers: [f32; 4],
    pub price_multiplier_looks: u32,
    pub price_multiplier_stats: u32,
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
    Player(i32),
    NPC(i32),
}

pub trait Entity {
    fn get_id(&self) -> EntityID;
    fn get_client<'a>(&self, client_map: &'a mut ClientMap) -> Option<&'a mut FFClient>;
    fn get_position(&self) -> Position;
    fn set_position(&mut self, pos: Position) -> (i32, i32);
    fn set_rotation(&mut self, angle: i32);
    fn send_enter(&self, client: &mut FFClient) -> FFResult<()>;
    fn send_exit(&self, client: &mut FFClient) -> FFResult<()>;

    fn cleanup(&mut self, state: &mut ShardServerState);

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
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
