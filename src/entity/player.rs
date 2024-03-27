use std::{
    any::Any,
    collections::HashMap,
    fmt::Display,
    time::{Duration, SystemTime},
};

use crate::{
    chunk::{ChunkCoords, InstanceID},
    database::db_get,
    defines::*,
    entity::{Combatant, Entity, EntityID},
    enums::{ItemLocation, ItemType, PlayerGuide, PlayerShardStatus, RewardType, RideType},
    error::{log, log_if_failed, panic_if_failed, panic_log, FFError, FFResult, Severity},
    item::Item,
    mission::MissionJournal,
    nano::Nano,
    net::{
        packet::{
            PacketID::{self, *},
            *,
        },
        ClientMap, FFClient,
    },
    path::Path,
    state::ShardServerState,
    tabledata::{tdata_get, TripData},
    util::{self, clamp, clamp_max, clamp_min},
    Position,
};

use rand::Rng;
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub struct PlayerStyle {
    pub gender: i8,
    pub face_style: i8,
    pub hair_style: i8,
    pub hair_color: i8,
    pub skin_color: i8,
    pub eye_color: i8,
    pub height: i8,
    pub body: i8,
}
impl TryFrom<sPCStyle> for PlayerStyle {
    type Error = FFError;

    fn try_from(style: sPCStyle) -> FFResult<Self> {
        // TODO style validation
        Ok(Self {
            gender: style.iGender,
            face_style: style.iFaceStyle,
            hair_style: style.iHairStyle,
            hair_color: style.iHairColor,
            skin_color: style.iSkinColor,
            eye_color: style.iEyeColor,
            height: style.iHeight,
            body: style.iBody,
        })
    }
}
impl Default for PlayerStyle {
    fn default() -> Self {
        Self {
            gender: 1,
            face_style: 1,
            hair_style: 1,
            hair_color: 1,
            skin_color: 1,
            eye_color: 1,
            height: 0,
            body: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerFlags {
    pub name_check_flag: bool,
    pub tutorial_flag: bool,
    pub payzone_flag: bool,
    pub tip_flags: i128,
}

#[derive(Debug, Clone, Copy)]
struct GuideData {
    current_guide: PlayerGuide,
    total_guides: usize,
}
impl Default for GuideData {
    fn default() -> Self {
        Self {
            current_guide: PlayerGuide::Computress,
            total_guides: 0,
        }
    }
}

#[derive(Debug, Clone)]
struct SkywayRideState {
    trip_data: &'static TripData,
    path: Path,
    monkey_pos: Position,
    resume_time: SystemTime,
}

#[derive(Debug, Default, Clone)]
struct TransportData {
    pub scamper_flags: i32,
    pub skyway_flags: [i64; WYVERN_LOCATION_FLAG_SIZE as usize],
    pub skyway_ride: Option<SkywayRideState>,
}
impl TransportData {
    pub fn set_scamper_flag(&mut self, location_id: i32) -> FFResult<i32> {
        match location_id {
            (1..=32) => {
                let offset = location_id - 1;
                self.scamper_flags |= 1 << offset;
                Ok(self.scamper_flags)
            }
            _ => Err(FFError::build(
                Severity::Warning,
                format!("Invalid S.C.A.M.P.E.R. location ID: {}", location_id),
            )),
        }
    }

    pub fn test_scamper_flag(&self, location_id: i32) -> FFResult<bool> {
        match location_id {
            1..=32 => {
                let offset = location_id - 1;
                Ok(self.scamper_flags & (1 << offset) != 0)
            }
            _ => Err(FFError::build(
                Severity::Warning,
                format!("Invalid S.C.A.M.P.E.R. location ID: {}", location_id),
            )),
        }
    }

    pub fn set_skyway_flag(
        &mut self,
        location_id: i32,
    ) -> FFResult<[i64; WYVERN_LOCATION_FLAG_SIZE as usize]> {
        match location_id {
            1..=63 => {
                let offset = location_id - 1;
                self.skyway_flags[0] |= 1 << offset;
            }
            64..=127 => {
                let offset = location_id - 64;
                self.skyway_flags[1] |= 1 << offset;
            }
            _ => {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("Invalid skyway location ID: {}", location_id),
                ))
            }
        };
        Ok(self.skyway_flags)
    }

    pub fn test_skyway_flag(&self, location_id: i32) -> FFResult<bool> {
        match location_id {
            (1..=63) => {
                let offset = location_id - 1;
                Ok(self.skyway_flags[0] & (1 << offset) != 0)
            }
            (64..=127) => {
                let offset = location_id - 64;
                Ok(self.skyway_flags[1] & (1 << offset) != 0)
            }
            _ => Err(FFError::build(
                Severity::Warning,
                format!("Invalid skyway location ID: {}", location_id),
            )),
        }
    }
}

#[derive(Debug, Clone)]
struct Nanocom {
    nano_inventory: HashMap<i16, Nano>,
    equipped_ids: [Option<i16>; SIZEOF_NANO_CARRY_SLOT as usize],
    active_slot: Option<usize>,
}
impl Nanocom {
    pub fn as_bank(&self) -> [sNano; SIZEOF_NANO_BANK_SLOT as usize] {
        let mut bank = [None.into(); SIZEOF_NANO_BANK_SLOT as usize];
        for (id, nano) in &self.nano_inventory {
            let idx = *id as usize;
            if idx < SIZEOF_NANO_BANK_SLOT as usize {
                bank[idx] = Some(nano.clone()).into();
            }
        }
        bank
    }
}
impl Default for Nanocom {
    fn default() -> Self {
        Self {
            nano_inventory: HashMap::new(),
            equipped_ids: [None; SIZEOF_NANO_CARRY_SLOT as usize],
            active_slot: None,
        }
    }
}

#[derive(Debug, Clone)]
struct PlayerInventory {
    main: [Option<Item>; SIZEOF_INVEN_SLOT as usize],
    equipped: [Option<Item>; SIZEOF_EQUIP_SLOT as usize],
    quest: [Option<(i16, usize)>; SIZEOF_QINVEN_SLOT as usize],
    bank: [Option<Item>; SIZEOF_BANK_SLOT as usize],
}
impl Default for PlayerInventory {
    fn default() -> Self {
        Self {
            main: [None; SIZEOF_INVEN_SLOT as usize],
            equipped: [None; SIZEOF_EQUIP_SLOT as usize],
            quest: [None; SIZEOF_QINVEN_SLOT as usize],
            bank: [None; SIZEOF_BANK_SLOT as usize],
        }
    }
}
impl PlayerInventory {
    fn get_quest_item_arr(&self) -> [sItemBase; SIZEOF_QINVEN_SLOT as usize] {
        self.quest.map(|vals| {
            let mut item_raw = sItemBase::default();
            if let Some((id, count)) = vals {
                item_raw.iType = ItemType::Quest as i16;
                item_raw.iID = id;
                item_raw.iOpt = count as i32;
            }
            item_raw
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct RewardRates {
    combat: f32,
    missions: f32,
    eggs: f32,
    racing: f32,
}
impl Default for RewardRates {
    fn default() -> Self {
        Self {
            combat: 1.0,
            missions: 1.0,
            eggs: 1.0,
            racing: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RewardData {
    taros: RewardRates,
    fusion_matter: RewardRates,
}
impl RewardData {
    pub fn set_reward_rate(
        &mut self,
        reward_type: RewardType,
        idx: usize,
        val: f32,
    ) -> FFResult<()> {
        let reward_rates = match reward_type {
            RewardType::Taros => &mut self.taros,
            RewardType::FusionMatter => &mut self.fusion_matter,
        };
        let rate = match idx {
            1 => Ok(&mut reward_rates.combat),
            2 => Ok(&mut reward_rates.missions),
            3 => Ok(&mut reward_rates.eggs),
            4 => Ok(&mut reward_rates.racing),
            0 => {
                for idx in 1..5 {
                    self.set_reward_rate(reward_type, idx, val).unwrap();
                }
                return Ok(());
            }
            _ => Err(FFError::build(
                Severity::Warning,
                format!("Invalid reward rate index: {}", idx),
            )),
        }?;
        *rate = val / 100.0; // val is in percent
        Ok(())
    }

    pub fn get_reward_rate(&self, reward_type: RewardType, idx: usize) -> FFResult<f32> {
        let reward_rates = match reward_type {
            RewardType::Taros => &self.taros,
            RewardType::FusionMatter => &self.fusion_matter,
        };
        match idx {
            1 => Ok(reward_rates.combat),
            2 => Ok(reward_rates.missions),
            3 => Ok(reward_rates.eggs),
            4 => Ok(reward_rates.racing),
            _ => Err(FFError::build(
                Severity::Warning,
                format!("Invalid reward rate index: {}", idx),
            )),
        }
    }

    pub fn get_rates_as_array(&self, reward_type: RewardType) -> [f32; 5] {
        let reward_rates = match reward_type {
            RewardType::Taros => &self.taros,
            RewardType::FusionMatter => &self.fusion_matter,
        };
        [
            unused!(),
            reward_rates.combat,
            reward_rates.missions,
            reward_rates.eggs,
            reward_rates.racing,
        ]
    }
}

#[derive(Debug, Clone, Default)]
pub struct PreWarpData {
    pub instance_id: InstanceID,
    pub position: Position,
}

#[derive(Debug, Clone, Default)]
pub struct Player {
    id: Option<i32>,
    slot_num: usize,
    uid: i64,
    pub first_name: String,
    pub last_name: String,
    client_id: Option<usize>,
    perms: i16,
    pub show_gm_marker: bool,
    pub invisible: bool,
    pub invulnerable: bool,
    pub in_menu: bool,
    pub in_combat: bool,
    pub freechat_muted: bool,
    pub reward_data: RewardData,
    position: Position,
    rotation: i32,
    pub instance_id: InstanceID,
    pub style: Option<PlayerStyle>,
    pub flags: PlayerFlags,
    level: i16,
    hp: i32,
    guide_data: GuideData,
    nano_data: Nanocom,
    pub mission_journal: MissionJournal,
    inventory: PlayerInventory,
    taros: u32,
    fusion_matter: u32,
    nano_potions: u32,
    weapon_boosts: u32,
    buddy_warp_time: i32,
    transport_data: TransportData,
    pub trade_id: Option<Uuid>,
    pub trade_offered_to: Option<i32>,
    pub vehicle_speed: Option<i32>,
    pre_warp_data: PreWarpData,
}
impl Player {
    pub fn new(uid: i64, slot_num: usize) -> Self {
        Self {
            uid,
            slot_num,
            hp: placeholder!(400),
            level: 1,
            ..Default::default()
        }
    }

    pub fn get_uid(&self) -> i64 {
        self.uid
    }

    pub fn get_slot_num(&self) -> usize {
        self.slot_num
    }

    pub fn get_perms(&self) -> i16 {
        self.perms
    }

    pub fn get_player_id(&self) -> i32 {
        self.id
            .unwrap_or_else(|| panic_log(&format!("Player with UID {} has no ID", self.uid)))
    }

    pub fn set_player_id(&mut self, pc_id: i32) {
        self.id = Some(pc_id);
    }

    pub fn set_client_id(&mut self, client_id: usize) {
        self.client_id = Some(client_id);
    }

    pub fn get_style(&self) -> sPCStyle {
        let style = self.style.unwrap_or_default();
        sPCStyle {
            iPC_UID: self.uid,
            iNameCheck: if self.flags.name_check_flag { 1 } else { 0 },
            szFirstName: util::encode_utf16(&self.first_name),
            szLastName: util::encode_utf16(&self.last_name),
            iGender: style.gender,
            iFaceStyle: style.face_style,
            iHairStyle: style.hair_style,
            iHairColor: style.hair_color,
            iSkinColor: style.skin_color,
            iEyeColor: style.eye_color,
            iHeight: style.height,
            iBody: style.body,
            iClass: unused!(),
        }
    }

    pub fn get_style_2(&self) -> sPCStyle2 {
        sPCStyle2 {
            iAppearanceFlag: if self.style.is_some() { 1 } else { 0 },
            iTutorialFlag: if self.flags.tutorial_flag { 1 } else { 0 },
            iPayzoneFlag: if self.flags.payzone_flag { 1 } else { 0 },
        }
    }

    pub fn get_mapnum(&self) -> u32 {
        self.instance_id.map_num
    }

    pub fn change_nano(&mut self, slot: usize, nano_id: Option<i16>) -> FFResult<()> {
        if !(0..SIZEOF_NANO_CARRY_SLOT as usize).contains(&slot) {
            return Err(FFError::build(
                Severity::Warning,
                format!("Invalid nano slot: {}", slot),
            ));
        }
        self.nano_data.equipped_ids[slot] = nano_id;
        Ok(())
    }

    pub fn set_nano(&mut self, nano: Nano) {
        self.nano_data.nano_inventory.insert(nano.get_id(), nano);
    }

    pub fn get_active_nano_slot(&self) -> Option<usize> {
        self.nano_data.active_slot
    }

    pub fn set_active_nano_slot(&mut self, slot: Option<usize>) -> FFResult<()> {
        if let Some(slot) = slot {
            if !(0..SIZEOF_NANO_CARRY_SLOT as usize).contains(&slot) {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("Invalid nano slot: {}", slot),
                ));
            }
        }
        self.nano_data.active_slot = slot;
        Ok(())
    }

    pub fn get_active_nano(&self) -> Option<&Nano> {
        match self.nano_data.active_slot {
            Some(active_slot) => {
                let nano_id =
                    self.nano_data.equipped_ids[active_slot].expect("Empty nano equipped");
                let nano = self.nano_data.nano_inventory.get(&nano_id);
                Some(nano.expect("Locked nano equipped"))
            }
            None => None,
        }
    }

    pub fn get_active_nano_mut(&mut self) -> Option<&mut Nano> {
        match self.nano_data.active_slot {
            Some(active_slot) => {
                let nano_id =
                    self.nano_data.equipped_ids[active_slot].expect("Empty nano equipped");
                let nano = self.nano_data.nano_inventory.get_mut(&nano_id);
                Some(nano.expect("Locked nano equipped"))
            }
            None => None,
        }
    }

    pub fn unlock_nano(&mut self, nano_id: i16) -> FFResult<&mut Nano> {
        if self.nano_data.nano_inventory.contains_key(&nano_id) {
            return Err(FFError::build(
                Severity::Warning,
                format!("Nano {} is already unlocked", nano_id),
            ));
        }

        self.nano_data
            .nano_inventory
            .insert(nano_id, Nano::new(nano_id));
        Ok(self.get_nano_mut(nano_id).unwrap())
    }

    pub fn get_nano(&self, nano_id: i16) -> Option<&Nano> {
        self.nano_data.nano_inventory.get(&nano_id)
    }

    pub fn get_nano_mut(&mut self, nano_id: i16) -> Option<&mut Nano> {
        self.nano_data.nano_inventory.get_mut(&nano_id)
    }

    pub fn tune_nano(&mut self, nano_id: i16, skill_selection: Option<i16>) -> FFResult<()> {
        let nano = self.get_nano_mut(nano_id).ok_or(FFError::build(
            Severity::Warning,
            format!("Nano {} is locked", nano_id),
        ))?;

        let stats = tdata_get().get_nano_stats(nano_id).unwrap();

        if let Some(skill_id) = skill_selection {
            if !stats.skills.contains(&skill_id) {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("Invalid skill id {} for nano {}", skill_id, nano.get_id()),
                ));
            }
        }

        nano.tune(skill_selection);
        Ok(())
    }

    pub fn get_equipped_nano_ids(&self) -> [u16; SIZEOF_NANO_CARRY_SLOT as usize] {
        self.nano_data.equipped_ids.map(|id| id.unwrap_or(0) as u16)
    }

    pub fn get_nano_iter(&self) -> impl Iterator<Item = &Nano> {
        self.nano_data.nano_inventory.values()
    }

    pub fn get_load_data(&self) -> sPCLoadData2CL {
        sPCLoadData2CL {
            iUserLevel: self.perms,
            PCStyle: self.get_style(),
            PCStyle2: self.get_style_2(),
            iLevel: self.level,
            iMentor: self.guide_data.current_guide as i16,
            iMentorCount: self.guide_data.total_guides as i16,
            iHP: self.hp,
            iBatteryW: self.weapon_boosts as i32,
            iBatteryN: self.nano_potions as i32,
            iCandy: self.taros as i32,
            iFusionMatter: self.fusion_matter as i32,
            iSpecialState: self.get_special_state_bit_flag(),
            iMapNum: self.get_mapnum() as i32,
            iX: self.position.x,
            iY: self.position.y,
            iZ: self.position.z,
            iAngle: self.rotation,
            aEquip: self.inventory.equipped.map(Option::<Item>::into),
            aInven: self.inventory.main.map(Option::<Item>::into),
            aQInven: self.inventory.get_quest_item_arr(),
            aNanoBank: self.nano_data.as_bank(),
            aNanoSlots: self.get_equipped_nano_ids(),
            iActiveNanoSlotNum: match self.nano_data.active_slot {
                Some(active_slot) => active_slot as i16,
                None => -1,
            },
            iConditionBitFlag: self.get_condition_bit_flag(),
            eCSTB___Add: placeholder!(0),
            TimeBuff: sTimeBuff {
                iTimeLimit: placeholder!(0),
                iTimeDuration: placeholder!(0),
                iTimeRepeat: placeholder!(0),
                iValue: placeholder!(0),
                iConfirmNum: placeholder!(0),
            },
            aQuestFlag: self.mission_journal.completed_mission_flags,
            aRepeatQuestFlag: unused!(),
            aRunningQuest: self.mission_journal.get_running_quests(),
            iCurrentMissionID: self.mission_journal.get_active_mission_id().unwrap_or(0),
            iWarpLocationFlag: self.transport_data.scamper_flags,
            aWyvernLocationFlag: self.transport_data.skyway_flags,
            iBuddyWarpTime: self.buddy_warp_time,
            iFatigue: unused!(),
            iFatigue_Level: unused!(),
            iFatigueRate: unused!(),
            iFirstUseFlag1: self.flags.tip_flags as i64,
            iFirstUseFlag2: (self.flags.tip_flags >> 64) as i64,
            aiPCSkill: [unused!(); 33],
        }
    }

    pub fn get_state_bit_flag(&self) -> i8 {
        let mut flags = 0;
        if self.vehicle_speed.is_some() {
            flags |= FLAG_PC_STATE_VEHICLE;
        }
        flags
    }

    pub fn get_special_state_bit_flag(&self) -> i8 {
        let mut flags = 0;
        if self.show_gm_marker {
            flags |= CN_SPECIAL_STATE_FLAG__PRINT_GM;
        }
        if self.invulnerable {
            flags |= CN_SPECIAL_STATE_FLAG__INVULNERABLE;
        }
        if self.invisible {
            flags |= CN_SPECIAL_STATE_FLAG__INVISIBLE;
        }
        if self.in_menu {
            flags |= CN_SPECIAL_STATE_FLAG__FULL_UI;
        }
        if self.in_combat {
            flags |= CN_SPECIAL_STATE_FLAG__COMBAT;
        }
        if self.freechat_muted {
            flags |= CN_SPECIAL_STATE_FLAG__MUTE_FREECHAT;
        }
        flags as i8
    }

    pub fn get_appearance_data(&self) -> sPCAppearanceData {
        sPCAppearanceData {
            iID: self.id.unwrap_or_default(),
            PCStyle: self.get_style(),
            iConditionBitFlag: self.get_condition_bit_flag(),
            iPCState: self.get_state_bit_flag(),
            iSpecialState: self.get_special_state_bit_flag(),
            iLv: self.level,
            iHP: self.hp,
            iMapNum: self.get_mapnum() as i32,
            iX: self.position.x,
            iY: self.position.y,
            iZ: self.position.z,
            iAngle: self.rotation,
            ItemEquip: self.inventory.equipped.map(Option::<Item>::into),
            Nano: self.get_active_nano().cloned().into(),
            eRT: unused!(),
        }
    }

    pub fn get_item(&self, location: ItemLocation, slot_num: usize) -> FFResult<&Option<Item>> {
        let err = Err(FFError::build(
            Severity::Warning,
            format!("Bad slot number: {slot_num} (location {:?})", location),
        ));
        match location {
            ItemLocation::Equip => {
                if slot_num < SIZEOF_EQUIP_SLOT as usize {
                    Ok(&self.inventory.equipped[slot_num])
                } else {
                    err
                }
            }
            ItemLocation::Inven => {
                if slot_num < SIZEOF_INVEN_SLOT as usize {
                    Ok(&self.inventory.main[slot_num])
                } else {
                    err
                }
            }
            ItemLocation::QInven => unimplemented!("Quest items not accessible by slot number"),
            ItemLocation::Bank => {
                if slot_num < SIZEOF_BANK_SLOT as usize {
                    Ok(&self.inventory.bank[slot_num])
                } else {
                    err
                }
            }
        }
    }

    pub fn get_item_mut(
        &mut self,
        location: ItemLocation,
        slot_num: usize,
    ) -> FFResult<&mut Option<Item>> {
        let err_oob = Err(FFError::build(
            Severity::Warning,
            format!("Bad slot number: {slot_num} (location {:?})", location),
        ));
        let err_trading = Err(FFError::build(
            Severity::Warning,
            format!(
                "Can't mutate inventory. Player {} trading",
                self.id.unwrap_or_default()
            ),
        ));

        let res = match location {
            ItemLocation::Equip => {
                if slot_num < SIZEOF_EQUIP_SLOT as usize {
                    Ok(&mut self.inventory.equipped[slot_num])
                } else {
                    err_oob
                }
            }
            ItemLocation::Inven => {
                if slot_num < SIZEOF_INVEN_SLOT as usize {
                    Ok(&mut self.inventory.main[slot_num])
                } else {
                    err_oob
                }
            }
            ItemLocation::QInven => unimplemented!("Quest items not accessible by slot number"),
            ItemLocation::Bank => {
                if slot_num < SIZEOF_BANK_SLOT as usize {
                    Ok(&mut self.inventory.bank[slot_num])
                } else {
                    err_oob
                }
            }
        };

        if res.as_ref().is_ok_and(|v| v.is_some()) && self.trade_id.is_some() {
            return err_trading;
        }

        res
    }

    pub fn set_item(
        &mut self,
        location: ItemLocation,
        slot_num: usize,
        item: Option<Item>,
    ) -> FFResult<Option<Item>> {
        let slot_from = self.get_item_mut(location, slot_num)?;
        let old_item = slot_from.take();
        *slot_from = item;
        Ok(old_item)
    }

    pub fn get_quest_item_count(&self, item_id: i16) -> usize {
        self.inventory
            .quest
            .iter()
            .flatten()
            .find(|(qitem_id, _)| *qitem_id == item_id)
            .map(|(_, count)| *count)
            .unwrap_or(0)
    }

    pub fn set_quest_item_count(&mut self, item_id: i16, count: usize) -> usize {
        for (idx, slot) in self.inventory.quest.iter_mut().enumerate() {
            if let Some((qitem_id, _)) = slot {
                if *qitem_id == item_id {
                    if count == 0 {
                        *slot = None;
                    } else {
                        *slot = Some((item_id, count));
                    }
                    return idx;
                }
            } else {
                *slot = Some((item_id, count));
                return idx;
            }
        }
        panic_log("No free quest item slots");
    }

    pub fn get_free_slots(&self, location: ItemLocation) -> usize {
        match location {
            ItemLocation::Equip => self
                .inventory
                .equipped
                .iter()
                .filter(|slot| slot.is_none())
                .count(),
            ItemLocation::Inven => self
                .inventory
                .main
                .iter()
                .filter(|slot| slot.is_none())
                .count(),
            ItemLocation::QInven => self
                .inventory
                .quest
                .iter()
                .filter(|slot| slot.is_none())
                .count(),
            ItemLocation::Bank => self
                .inventory
                .bank
                .iter()
                .filter(|slot| slot.is_none())
                .count(),
        }
    }

    pub fn find_free_slot(&self, location: ItemLocation) -> FFResult<usize> {
        let inven = match location {
            ItemLocation::Equip => self.inventory.equipped.as_slice(),
            ItemLocation::Inven => self.inventory.main.as_slice(),
            ItemLocation::QInven => unimplemented!("Quest item inventory not searchable"),
            ItemLocation::Bank => self.inventory.bank.as_slice(),
        };

        for (slot_num, slot) in inven.iter().enumerate() {
            if slot.is_none() {
                return Ok(slot_num);
            }
        }
        Err(FFError::build(
            Severity::Warning,
            format!(
                "Player {} has no free slots in {:?}",
                self.get_player_id(),
                location
            ),
        ))
    }

    pub fn find_items_any(&self, f: impl Fn(&Item) -> bool) -> Vec<(ItemLocation, usize)> {
        let mut found = Vec::new();
        found.extend(
            self.find_items(ItemLocation::Equip, &f)
                .iter()
                .map(|slot_num| (ItemLocation::Equip, *slot_num)),
        );
        found.extend(
            self.find_items(ItemLocation::Inven, &f)
                .iter()
                .map(|slot_num| (ItemLocation::Inven, *slot_num)),
        );
        found.extend(
            self.find_items(ItemLocation::Bank, &f)
                .iter()
                .map(|slot_num| (ItemLocation::Bank, *slot_num)),
        );
        found
    }

    pub fn find_items(&self, location: ItemLocation, f: impl Fn(&Item) -> bool) -> Vec<usize> {
        let inven = match location {
            ItemLocation::Equip => self.inventory.equipped.as_slice(),
            ItemLocation::Inven => self.inventory.main.as_slice(),
            ItemLocation::QInven => unimplemented!("Quest item inventory not searchable"),
            ItemLocation::Bank => self.inventory.bank.as_slice(),
        };

        inven
            .iter()
            .enumerate()
            .filter_map(|(slot_num, slot)| {
                if let Some(item) = slot {
                    if f(item) {
                        Some(slot_num)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn get_item_iter(&self) -> impl Iterator<Item = (usize, &Item)> {
        let inv_slot_max =
            (SIZEOF_EQUIP_SLOT + SIZEOF_INVEN_SLOT + SIZEOF_BANK_SLOT + SIZEOF_QINVEN_SLOT)
                as usize;
        (0..inv_slot_max).filter_map(move |slot_num| {
            let (loc, slot_num_loc) = util::slot_num_to_loc_and_slot_num(slot_num).unwrap();
            if loc == ItemLocation::QInven {
                return None;
            }

            let item = self.get_item(loc, slot_num_loc).unwrap();
            item.as_ref().map(|item| (slot_num, item))
        })
    }

    pub fn get_quest_item_iter(&self) -> impl Iterator<Item = (i16, usize)> + '_ {
        self.inventory
            .quest
            .iter()
            .flatten()
            .map(|(id, count)| (*id, *count))
    }

    pub fn get_equipped(&self) -> [Option<Item>; 9] {
        self.inventory.equipped
    }

    pub fn get_taros(&self) -> u32 {
        self.taros
    }

    pub fn get_fusion_matter(&self) -> u32 {
        self.fusion_matter
    }

    pub fn update_first_use_flag(&mut self, bit_offset: i32) -> FFResult<i128> {
        if !(1..=129).contains(&bit_offset) {
            Err(FFError::build(
                Severity::Warning,
                format!("First use flag offset out of range: {}", bit_offset),
            ))
        } else {
            self.flags.tip_flags |= 1 << (bit_offset - 1);
            Ok(self.flags.tip_flags)
        }
    }

    pub fn get_scamper_flags(&self) -> i32 {
        self.transport_data.scamper_flags
    }

    pub fn get_skyway_flags(&self) -> [i64; WYVERN_LOCATION_FLAG_SIZE as usize] {
        self.transport_data.skyway_flags
    }

    pub fn set_scamper_flag(&mut self, flags: i32) {
        self.transport_data.scamper_flags = flags;
    }

    pub fn set_skyway_flags(&mut self, flags: [i64; WYVERN_LOCATION_FLAG_SIZE as usize]) {
        self.transport_data.skyway_flags = flags;
    }

    pub fn unlock_scamper_location(&mut self, location_id: i32) -> FFResult<i32> {
        self.transport_data.set_scamper_flag(location_id)
    }

    pub fn is_scamper_location_unlocked(&self, location_id: i32) -> FFResult<bool> {
        self.transport_data.test_scamper_flag(location_id)
    }

    pub fn unlock_skyway_location(
        &mut self,
        location_id: i32,
    ) -> FFResult<[i64; WYVERN_LOCATION_FLAG_SIZE as usize]> {
        self.transport_data.set_skyway_flag(location_id)
    }

    pub fn is_skyway_location_unlocked(&self, location_id: i32) -> FFResult<bool> {
        self.transport_data.test_skyway_flag(location_id)
    }

    pub fn set_tutorial_done(&mut self) {
        self.flags.tutorial_flag = true;
        // unlock buttercup
        let buttercup_stats = tdata_get().get_nano_stats(ID_BUTTERCUP).unwrap();
        self.unlock_nano(ID_BUTTERCUP).unwrap();
        self.tune_nano(ID_BUTTERCUP, Some(buttercup_stats.skills[0]))
            .unwrap();
        self.change_nano(0, Some(ID_BUTTERCUP)).unwrap();
        // equip lightning gun
        self.set_item(
            ItemLocation::Equip,
            EQUIP_SLOT_HAND as usize,
            Some(Item::new(ItemType::Hand, ID_LIGHTNING_GUN)),
        )
        .unwrap();
        // TODO delete all active missions
        // place in Sector V future
        let mut rand = rand::thread_rng();
        let range = 0; //PC_START_LOCATION_RANDOM_RANGE as i32 / 2;
        self.position = Position {
            x: 632032 + rand.gen_range(-range..=range),
            y: 187177 + rand.gen_range(-range..=range),
            z: -5500,
        }
    }

    pub fn get_guide(&self) -> PlayerGuide {
        self.guide_data.current_guide
    }

    pub fn update_guide(&mut self, guide: PlayerGuide) -> usize {
        self.guide_data.current_guide = guide;
        self.guide_data.total_guides =
            clamp_max(self.guide_data.total_guides + 1, i16::MAX as usize);
        self.guide_data.total_guides
    }

    pub fn set_future_done(&mut self) {
        self.flags.payzone_flag = true;
        // TODO delete all active missions
    }

    pub fn is_future_done(&self) -> bool {
        self.flags.payzone_flag
    }

    pub fn set_taros(&mut self, taros: u32) -> u32 {
        self.taros = clamp(taros, 0, PC_CANDY_MAX);
        self.taros
    }

    pub fn set_hp(&mut self, hp: i32) -> i32 {
        self.hp = clamp_min(hp, 0);
        self.hp
    }

    pub fn set_level(&mut self, level: i16) -> i16 {
        self.level = clamp(level, 1, PC_LEVEL_MAX as i16);
        self.level
    }

    pub fn set_fusion_matter(&mut self, fusion_matter: u32) -> u32 {
        self.fusion_matter = clamp(fusion_matter, 0, PC_FUSIONMATTER_MAX);
        self.fusion_matter
    }

    pub fn get_weapon_boosts(&self) -> u32 {
        self.weapon_boosts
    }

    pub fn get_nano_potions(&self) -> u32 {
        self.nano_potions
    }

    pub fn set_weapon_boosts(&mut self, weapon_boosts: u32) -> u32 {
        self.weapon_boosts = clamp(weapon_boosts, 0, PC_BATTERY_MAX);
        self.weapon_boosts
    }

    pub fn set_nano_potions(&mut self, nano_potions: u32) -> u32 {
        self.nano_potions = clamp(nano_potions, 0, PC_BATTERY_MAX);
        self.nano_potions
    }

    pub fn set_pre_warp(&mut self) {
        self.pre_warp_data = PreWarpData {
            instance_id: self.instance_id,
            position: self.position,
        }
    }

    pub fn get_pre_warp(&self) -> &PreWarpData {
        &self.pre_warp_data
    }

    pub fn start_skyway_ride(&mut self, trip_data: &'static TripData, mut path: Path) {
        path.tick(&mut self.position); // advance to Moving state
        self.transport_data.skyway_ride = Some(SkywayRideState {
            trip_data,
            path,
            monkey_pos: self.position,
            resume_time: SystemTime::now(),
        });
    }

    pub fn disconnect(pc_id: i32, state: &mut ShardServerState, clients: &mut ClientMap) {
        let player = state.get_player(pc_id).unwrap();
        let pc_uid = player.get_uid();
        let mut db = db_get();
        panic_if_failed(db.save_player(player, None));
        log(
            Severity::Info,
            &format!(
                "{} left (channel {})",
                player, player.instance_id.channel_num
            ),
        );

        let id = EntityID::Player(pc_id);
        let entity_map = &mut state.entity_map;
        entity_map.update(id, None, Some(clients));
        let mut player = entity_map.untrack(id);
        player.cleanup(clients, state);
        player.get_client(clients).unwrap().disconnect();

        let pkt_pc = sP_FE2LS_UPDATE_PC_SHARD {
            iPC_UID: pc_uid,
            ePSS: PlayerShardStatus::Exited as i8,
        };
        let pkt_chan = sP_FE2LS_UPDATE_CHANNEL_STATUSES {
            aChannelStatus: state.entity_map.get_channel_statuses().map(|s| s as u8),
        };
        match clients.get_login_server() {
            Some(login_server) => {
                log_if_failed(login_server.send_packet(P_FE2LS_UPDATE_PC_SHARD, &pkt_pc));
                log_if_failed(login_server.send_packet(P_FE2LS_UPDATE_CHANNEL_STATUSES, &pkt_chan));
            }
            None => {
                log(
                    Severity::Warning,
                    "Player::disconnect: No login server connected! Things may break.",
                );
            }
        }
    }
}
impl Combatant for Player {
    fn get_condition_bit_flag(&self) -> i32 {
        placeholder!(0)
    }

    fn get_level(&self) -> i16 {
        self.level
    }

    fn get_hp(&self) -> i32 {
        self.hp
    }

    fn get_max_hp(&self) -> i32 {
        placeholder!(400)
    }
}
impl Entity for Player {
    fn get_client<'a>(&self, client_map: &'a mut ClientMap) -> Option<&'a mut FFClient> {
        self.client_id.map(|key| client_map.get(key))
    }

    fn get_id(&self) -> EntityID {
        EntityID::Player(self.get_player_id())
    }

    fn get_position(&self) -> Position {
        self.position
    }

    fn get_rotation(&self) -> i32 {
        self.rotation
    }

    fn get_chunk_coords(&self) -> ChunkCoords {
        ChunkCoords::from_pos_inst(self.position, self.instance_id)
    }

    fn set_position(&mut self, pos: Position) {
        self.position = pos;
    }

    fn set_rotation(&mut self, angle: i32) {
        self.rotation = angle % 360;
    }

    fn send_enter(&self, client: &mut FFClient) -> FFResult<()> {
        let pkt = sP_FE2CL_PC_NEW {
            PCAppearanceData: self.get_appearance_data(),
        };
        client.send_packet(PacketID::P_FE2CL_PC_NEW, &pkt)
    }

    fn send_exit(&self, client: &mut FFClient) -> FFResult<()> {
        let pkt = sP_FE2CL_PC_EXIT {
            iID: self.get_player_id(),
            iExitType: unused!(),
        };
        client.send_packet(PacketID::P_FE2CL_PC_EXIT, &pkt)
    }

    fn cleanup(&mut self, clients: &mut ClientMap, state: &mut ShardServerState) {
        let pc_id = self.get_player_id();

        // cleanup the buyback list
        if state.buyback_lists.contains_key(&pc_id) {
            state.buyback_lists.remove(&pc_id);
        }

        // cleanup ongoing trade
        if let Some(trade_id) = self.trade_id {
            let trade = state.ongoing_trades.remove(&trade_id).unwrap();
            let pc_id_other = trade.get_other_id(pc_id);
            let player_other = state.get_player_mut(pc_id_other).unwrap();
            player_other.trade_id = None;
            let client_other = player_other.get_client(clients).unwrap();
            let pkt_cancel = sP_FE2CL_REP_PC_TRADE_CONFIRM_CANCEL {
                iID_Request: pc_id,
                iID_From: trade.get_id_from(),
                iID_To: trade.get_id_to(),
            };
            log_if_failed(
                client_other.send_packet(P_FE2CL_REP_PC_TRADE_CONFIRM_CANCEL, &pkt_cancel),
            );
        }
    }

    fn tick(&mut self, time: SystemTime, clients: &mut ClientMap, state: &mut ShardServerState) {
        let pc_id = self.id.unwrap();
        // Skyway ride
        if let Some(ref mut ride) = self.transport_data.skyway_ride {
            if ride.resume_time > time {
                return;
            }

            if ride.path.is_done() {
                // we're done!
                let final_pos = ride.monkey_pos;
                let cost = ride.trip_data.cost;
                self.set_taros(self.taros - cost);
                self.set_position(final_pos);
                self.transport_data.skyway_ride = None;
                crate::helpers::broadcast_monkey(pc_id, RideType::None, clients, state);
                return;
            }

            // N.B. the client doesn't treat monkey movement like every other movement.
            // instead of using the speed value from the packet, it uses the distance between the
            // current position and the target position. so we can only send move packets once we've
            // covered about the same distance as the speed.
            // 100% causes the client to go too fast and pause, but 80% seems to work fine.
            const SPEED_TO_DISTANCE_FACTOR: f32 = 1.0;

            // tick the path until we've covered the same distance as the speed
            let speed = ride.path.get_speed() as u32;
            let distance_to_cover = (speed as f32 * SPEED_TO_DISTANCE_FACTOR) as u32;
            let mut distance = 0;
            while distance < distance_to_cover {
                let old_pos = ride.monkey_pos;
                ride.path.tick(&mut ride.monkey_pos);
                distance += old_pos.distance_to(&ride.monkey_pos);
                if ride.path.is_done() {
                    break;
                }
            }

            // update the player's chunk.
            // We don't actually update their position until they land
            let chunk_coords = ChunkCoords::from_pos_inst(ride.monkey_pos, self.instance_id);
            state
                .entity_map
                .update(EntityID::Player(pc_id), Some(chunk_coords), Some(clients));

            // send the move packet
            let pkt = sP_FE2CL_PC_BROOMSTICK_MOVE {
                iPC_ID: pc_id,
                iToX: ride.monkey_pos.x,
                iToY: ride.monkey_pos.y,
                iToZ: ride.monkey_pos.z,
                iSpeed: unused!(),
            };
            state
                .entity_map
                .for_each_around(EntityID::Player(pc_id), clients, |c| {
                    c.send_packet(PacketID::P_FE2CL_PC_BROOMSTICK_MOVE, &pkt)
                });

            // wait for the client to catch up. in theory, takes one second.
            ride.resume_time = time + Duration::from_secs(1);
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
impl Display for Player {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let title = match self.perms {
            0 => Some("GM"),
            1 => Some("Mod"),
            _ => None,
        };
        let title = match title {
            Some(title) => format!("({}) ", title),
            None => String::new(),
        };
        write!(
            f,
            "{}{} {} ({})",
            title,
            self.first_name,
            self.last_name,
            self.get_uid()
        )
    }
}

#[derive(Debug)]
pub enum PlayerSearchQuery {
    ByID(i32),
    ByUID(i64),
    ByName(String, String),
}
impl PlayerSearchQuery {
    pub fn execute(&self, state: &ShardServerState) -> Option<i32> {
        match self {
            PlayerSearchQuery::ByID(pc_id) => {
                if state.entity_map.get_player(*pc_id).is_some() {
                    Some(*pc_id)
                } else {
                    None
                }
            }
            PlayerSearchQuery::ByUID(pc_uid) => state
                .entity_map
                .find_players(|player| player.get_uid() == *pc_uid)
                .first()
                .copied(),
            PlayerSearchQuery::ByName(first_name, last_name) => state
                .entity_map
                .find_players(|player| {
                    player.first_name.eq_ignore_ascii_case(first_name)
                        && player.last_name.eq_ignore_ascii_case(last_name)
                })
                .first()
                .copied(),
        }
    }
}
