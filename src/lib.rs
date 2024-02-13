#![allow(clippy::derivable_impls)]

use std::{any::Any, cmp::min, hash::Hash, time::SystemTime};

use chunk::ChunkCoords;
use defines::{
    NANO_STAMINA_MAX, SHARD_TICKS_PER_SECOND, SIZEOF_TRADE_SLOT, SIZEOF_VENDOR_TABLE_SLOT,
};
use enums::ItemType;
use error::{panic_log, FFError, FFResult, Severity};
use net::{
    ffclient::FFClient,
    packet::{sItemBase, sItemTrade, sItemVendor, sNano, sRunningQuest},
    ClientMap,
};
use player::Player;
use state::shard::ShardServerState;
use tabledata::{tdata_get, NanoStats};
use vecmath::{vec3_add, vec3_len, vec3_scale, vec3_sub, Vector3};

use crate::enums::ItemLocation;

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
pub mod helpers;
pub mod net;
pub mod state;
pub mod timer;
pub mod util;

pub mod config;
pub mod database;
pub mod tabledata;

pub mod chunk;
pub mod npc;
pub mod player;
pub mod slider;

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}
impl Position {
    pub fn distance_to(&self, other: &Position) -> u32 {
        // scaling down for the multiplication helps to avoid overflow here
        const DIST_MATH_SCALE: f32 = 100.0;
        let dx = self.x.abs_diff(other.x) as f32 / DIST_MATH_SCALE;
        let dy = self.y.abs_diff(other.y) as f32 / DIST_MATH_SCALE;
        let dz = self.z.abs_diff(other.z) as f32 / DIST_MATH_SCALE;
        ((dx * dx + dy * dy + dz * dz).sqrt() * DIST_MATH_SCALE) as u32
    }

    pub fn interpolate(&self, target: &Position, distance: f32) -> (Position, bool) {
        let source = (*self).into();
        let target = (*target).into();
        let delta = vec3_sub(target, source);
        let delta_len = vec3_len(delta);
        if delta_len <= distance {
            (target.into(), true)
        } else {
            let new_pos = vec3_add(source, vec3_scale(delta, distance / delta_len)).into();
            (new_pos, false)
        }
    }

    pub fn get_unstuck(&self) -> Position {
        const UNSTICK_XY_RANGE: i32 = 200;
        const UNSTICK_Z_BUMP: i32 = 80;
        Position {
            x: self.x + util::rand_range_inclusive(-UNSTICK_XY_RANGE, UNSTICK_XY_RANGE),
            y: self.y + util::rand_range_inclusive(-UNSTICK_XY_RANGE, UNSTICK_XY_RANGE),
            z: self.z + UNSTICK_Z_BUMP,
        }
    }
}
impl From<Vector3<f32>> for Position {
    fn from(value: Vector3<f32>) -> Self {
        Self {
            x: value[0] as i32,
            y: value[1] as i32,
            z: value[2] as i32,
        }
    }
}
impl From<Position> for Vector3<f32> {
    fn from(value: Position) -> Self {
        [value.x as f32, value.y as f32, value.z as f32]
    }
}

#[derive(Debug, Copy, Clone)]
pub struct PathPoint {
    pub pos: Position,
    pub speed: i32, // from previous point
    pub stop_ticks: usize,
}
impl PartialEq for PathPoint {
    fn eq(&self, other: &Self) -> bool {
        self.pos == other.pos
    }
}
impl Eq for PathPoint {}

#[derive(Debug, Clone)]
pub enum PathState {
    Pending,
    Moving,
    Waiting(usize),
    Done,
}

#[derive(Debug, Clone)]
pub struct Path {
    points: Vec<PathPoint>,
    cycle: bool,
    idx: usize,
    state: PathState,
}
impl Path {
    pub fn new(points: Vec<PathPoint>, cycle: bool) -> Self {
        Self {
            points,
            cycle,
            idx: 0,
            state: PathState::Pending,
        }
    }

    pub fn get_total_length(&self) -> u32 {
        let mut total_length = 0;
        for i in 0..self.points.len() - 1 {
            total_length += self.points[i].pos.distance_to(&self.points[i + 1].pos);
        }
        if self.cycle {
            total_length += self
                .points
                .last()
                .unwrap()
                .pos
                .distance_to(&self.points[0].pos);
        }
        total_length
    }

    pub fn get_target_pos(&self) -> Position {
        self.points[self.idx].pos
    }

    pub fn get_speed(&self) -> i32 {
        match self.state {
            PathState::Moving => self.points[self.idx].speed,
            _ => 0,
        }
    }

    pub fn advance(&mut self) {
        self.idx += 1;
        if self.idx == self.points.len() {
            if self.cycle {
                self.idx = 0;
            } else {
                self.idx -= 1; // hold last point as target
                self.state = PathState::Done;
            }
        }
    }

    pub fn tick(&mut self, pos: &mut Position) -> bool {
        match self.state {
            PathState::Pending => {
                self.state = PathState::Moving;
            }
            PathState::Moving => {
                let dist = self.points[self.idx].speed as f32 / SHARD_TICKS_PER_SECOND as f32;
                let target_point = self.points[self.idx];
                let target_pos = target_point.pos;
                let source_pos = *pos;
                let (new_pos, snap) = source_pos.interpolate(&target_pos, dist);
                *pos = new_pos;
                if snap {
                    // reached target
                    if target_point.stop_ticks > 0 {
                        self.state = PathState::Waiting(target_point.stop_ticks * SHARD_TICKS_PER_SECOND);
                    } else {
                        self.advance();
                        return true;
                    }
                }
            }
            PathState::Waiting(ticks_left) => {
                if ticks_left == 1 {
                    self.state = PathState::Moving;
                    self.advance();
                } else {
                    self.state = PathState::Waiting(ticks_left - 1);
                }
            }
            PathState::Done => {}
        };
        false
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
    pub speed: Option<i32>,
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
    pc_ids: [i32; 2],
    offers: [TradeOffer; 2],
}
impl TradeContext {
    pub fn new(pc_ids: [i32; 2]) -> Self {
        Self {
            pc_ids,
            offers: Default::default(),
        }
    }

    pub fn get_id_from(&self) -> i32 {
        self.pc_ids[0]
    }

    pub fn get_id_to(&self) -> i32 {
        self.pc_ids[1]
    }

    pub fn get_other_id(&self, pc_id: i32) -> i32 {
        for id in self.pc_ids {
            if id != pc_id {
                return id;
            }
        }
        panic_log("Bad trade state");
    }

    fn get_offer_mut(&mut self, pc_id: i32) -> FFResult<&mut TradeOffer> {
        let idx = self
            .pc_ids
            .iter()
            .position(|id| *id == pc_id)
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Player {} is not a part of the trade", pc_id),
            ))?;
        Ok(&mut self.offers[idx])
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
        self.offers.iter().all(|offer| offer.confirmed)
    }

    pub fn lock_in(&mut self, pc_id: i32) -> FFResult<bool> {
        let offer = self.get_offer_mut(pc_id)?;
        offer.confirmed = true;
        Ok(self.is_ready())
    }

    pub fn resolve(
        mut self,
        players: (&mut Player, &mut Player),
    ) -> FFResult<(
        [sItemTrade; SIZEOF_TRADE_SLOT as usize],
        [sItemTrade; SIZEOF_TRADE_SLOT as usize],
    )> {
        fn transfer(
            offer: &mut TradeOffer,
            from: &mut Player,
            to: &mut Player,
        ) -> FFResult<Vec<sItemTrade>> {
            // taros
            from.set_taros(from.get_taros() - offer.taros);
            to.set_taros(to.get_taros() + offer.taros);

            // items
            let mut items = Vec::new();
            for item in offer.items.iter().flatten() {
                let slot = from
                    .get_item_mut(ItemLocation::Inven, item.inven_slot_num)
                    .unwrap();
                let item_traded = Item::split_items(slot, item.quantity).unwrap();
                let free_slot = to.find_free_slot(ItemLocation::Inven)?;
                to.set_item(ItemLocation::Inven, free_slot, Some(item_traded))
                    .unwrap();
                items.push(sItemTrade {
                    iType: item_traded.ty as i16,
                    iID: item_traded.id,
                    iOpt: item_traded.quantity as i32,
                    iInvenNum: free_slot as i32,
                    iSlotNum: unused!(),
                });
            }

            Ok(items)
        }

        let blank_item = sItemTrade {
            iType: 0,
            iID: 0,
            iOpt: 0,
            iInvenNum: 0,
            iSlotNum: 0,
        };
        let mut items = (
            transfer(
                self.get_offer_mut(players.0.get_player_id()).unwrap(),
                players.0,
                players.1,
            )?,
            transfer(
                self.get_offer_mut(players.1.get_player_id()).unwrap(),
                players.1,
                players.0,
            )?,
        );
        items.0.resize(SIZEOF_TRADE_SLOT as usize, blank_item);
        items.1.resize(SIZEOF_TRADE_SLOT as usize, blank_item);
        Ok((items.1.try_into().unwrap(), items.0.try_into().unwrap()))
    }
}

pub struct CrocPotData {
    pub base_chance: f32,
    pub rarity_diff_multipliers: [f32; 4],
    pub price_multiplier_looks: u32,
    pub price_multiplier_stats: u32,
}

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

#[derive(Debug, Copy, Clone, Default)]
struct Mission {
    task_id: i32,
    target_npc_ids: [i32; 3],
    target_npc_counts: [i32; 3],
    target_item_ids: [i32; 3],
    target_item_counts: [i32; 3],
}
impl From<Mission> for sRunningQuest {
    fn from(value: Mission) -> Self {
        Self {
            m_aCurrTaskID: value.task_id,
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
    Slider(i32),
}

pub trait Entity {
    fn get_id(&self) -> EntityID;
    fn get_client<'a>(&self, client_map: &'a mut ClientMap) -> Option<&'a mut FFClient>;
    fn get_position(&self) -> Position;
    fn get_rotation(&self) -> i32;
    fn get_chunk_coords(&self) -> ChunkCoords;
    fn set_position(&mut self, pos: Position);
    fn set_rotation(&mut self, angle: i32);
    fn send_enter(&self, client: &mut FFClient) -> FFResult<()>;
    fn send_exit(&self, client: &mut FFClient) -> FFResult<()>;

    fn tick(&mut self, time: SystemTime, clients: &mut ClientMap, state: &mut ShardServerState);
    fn cleanup(&mut self, clients: &mut ClientMap, state: &mut ShardServerState);

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub trait Combatant {
    fn get_condition_bit_flag(&self) -> i32;
    fn get_level(&self) -> i16;
    fn get_hp(&self) -> i32;
    fn get_max_hp(&self) -> i32;
}
