use std::{cmp::min, time::SystemTime};

use crate::{
    defines::*,
    enums::ItemType,
    error::{FFError, FFResult},
    net::packet::*,
    tabledata::tdata_get,
    util,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Item {
    pub ty: ItemType,
    pub id: i16,
    appearance_id: Option<i16>,
    pub quantity: u16,
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

    pub fn get_stats(&self) -> FFResult<&ItemStats> {
        tdata_get().get_item_stats(self.id, self.ty)
    }

    pub fn get_expiry_time(&self) -> Option<SystemTime> {
        self.expiry_time
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
    pub sort_number: i32,
    pub ty: ItemType,
    pub id: i16,
}

pub struct VendorData {
    vendor_id: i32,
    items: Vec<VendorItem>,
}
impl VendorData {
    pub fn new(vendor_id: i32) -> Self {
        Self {
            vendor_id,
            items: Vec::new(),
        }
    }

    pub fn insert(&mut self, item: VendorItem) {
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

pub struct CrocPotData {
    pub base_chance: f32,
    pub rarity_diff_multipliers: [f32; 4],
    pub price_multiplier_looks: u32,
    pub price_multiplier_stats: u32,
}

#[derive(Debug)]
pub struct Reward {
    pub taros: u32,
    pub fusion_matter: u32,
    pub weapon_boosts: u32,
    pub nano_potions: u32,
    pub items: Vec<Item>,
}
impl Default for Reward {
    fn default() -> Self {
        Self {
            taros: 0,
            fusion_matter: 0,
            weapon_boosts: 0,
            nano_potions: 0,
            items: Vec::new(),
        }
    }
}
