use std::{any::Any, cmp::max, fmt::Display, time::SystemTime};

use crate::{
    chunk::{ChunkCoords, InstanceID},
    defines::*,
    enums::{ItemLocation, ItemType, PlayerGuide},
    error::{FFError, FFResult, Severity},
    net::{
        ffclient::FFClient,
        packet::{
            sPCAppearanceData, sPCLoadData2CL, sPCStyle, sPCStyle2, sP_FE2CL_PC_EXIT,
            sP_FE2CL_PC_NEW, sP_FE2CL_REP_PC_TRADE_CONFIRM_CANCEL, sTimeBuff,
            PacketID::{self, *},
        },
        ClientMap,
    },
    state::shard::ShardServerState,
    util::parse_utf16,
    Combatant, Entity, EntityID, Item, Mission, Nano, Position,
};

use num_traits::{clamp, clamp_max, clamp_min};
use rand::Rng;
use uuid::Uuid;

pub const TEST_ACC_UID_START: i64 = i64::MAX - 3;

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
            gender: if rand::random::<bool>() { 1 } else { 2 },
            face_style: 1,
            hair_style: 1,
            hair_color: 1,
            skin_color: 1,
            eye_color: 1,
            height: 1,
            body: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerFlags {
    pub appearance_flag: bool,
    pub tutorial_flag: bool,
    pub payzone_flag: bool,
    pub tip_flags: i128,
}

#[derive(Debug, Clone, Copy, Default)]
struct PlayerName {
    name_check: i8,
    first_name: [u16; SIZEOF_PC_FIRST_NAME as usize],
    last_name: [u16; SIZEOF_PC_LAST_NAME as usize],
}
impl Display for PlayerName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {}",
            parse_utf16(&self.first_name),
            parse_utf16(&self.last_name)
        )
    }
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

#[derive(Debug, Default, Clone, Copy)]
struct TransportData {
    pub scamper_flags: i32,
    pub skyway_flags: [i64; WYVERN_LOCATION_FLAG_SIZE as usize],
}
impl TransportData {
    pub fn set_all(&mut self) {
        self.scamper_flags = -1;
        self.skyway_flags = [-1; WYVERN_LOCATION_FLAG_SIZE as usize];
    }

    pub fn set_scamper_flag(&mut self, bit_offset: i32) -> FFResult<i32> {
        if !(1..=32).contains(&bit_offset) {
            Err(FFError::build(
                Severity::Warning,
                format!("Scamper flag offset out of range: {}", bit_offset),
            ))
        } else {
            self.scamper_flags |= 1 << (bit_offset - 1);
            Ok(self.scamper_flags)
        }
    }

    pub fn set_skyway_flag(
        &mut self,
        bit_offset: i32,
    ) -> FFResult<[i64; WYVERN_LOCATION_FLAG_SIZE as usize]> {
        if !(1..=(WYVERN_LOCATION_FLAG_SIZE as i32 * 64)).contains(&bit_offset) {
            Err(FFError::build(
                Severity::Warning,
                format!("Skyway flag offset out of range: {}", bit_offset),
            ))
        } else {
            let idx = if bit_offset > 32 { 1 } else { 0 };
            let offset = (bit_offset - 1) % 32;
            self.skyway_flags[idx] = 1 << offset;
            Ok(self.skyway_flags)
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Nanocom {
    nano_inventory: [Option<Nano>; SIZEOF_NANO_BANK_SLOT as usize],
    equipped_ids: [Option<i16>; SIZEOF_NANO_CARRY_SLOT as usize],
    active_slot: Option<usize>,
}
impl Default for Nanocom {
    fn default() -> Self {
        Self {
            nano_inventory: [None; SIZEOF_NANO_BANK_SLOT as usize],
            equipped_ids: [None; SIZEOF_NANO_CARRY_SLOT as usize],
            active_slot: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct MissionData {
    current_missions: [Option<Mission>; SIZEOF_RQUEST_SLOT as usize],
    active_mission_id: i32,
    mission_flags: [i64; SIZEOF_QUESTFLAG_NUMBER as usize],
    repeat_mission_flags: [i64; SIZEOF_REPEAT_QUESTFLAG_NUMBER as usize],
}

#[derive(Debug, Clone, Copy)]
struct PlayerInventory {
    main: [Option<Item>; SIZEOF_INVEN_SLOT as usize],
    equipped: [Option<Item>; SIZEOF_EQUIP_SLOT as usize],
    mission: [Option<Item>; SIZEOF_QINVEN_SLOT as usize],
    bank: [Option<Item>; SIZEOF_BANK_SLOT as usize],
}
impl Default for PlayerInventory {
    fn default() -> Self {
        Self {
            main: [None; SIZEOF_INVEN_SLOT as usize],
            equipped: Default::default(),
            mission: [None; SIZEOF_QINVEN_SLOT as usize],
            bank: [None; SIZEOF_BANK_SLOT as usize],
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Player {
    id: Option<i32>,
    uid: i64,
    client_id: Option<usize>,
    perms: i16,
    position: Position,
    rotation: i32,
    pub instance_id: InstanceID,
    pub style: PlayerStyle,
    pub flags: PlayerFlags,
    name: PlayerName,
    special_state: i8,
    level: i16,
    hp: i32,
    guide_data: GuideData,
    nano_data: Nanocom,
    mission_data: MissionData,
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
    pub pre_warp_map_num: u32,
}
impl Player {
    pub fn new(uid: i64) -> Self {
        Self {
            uid,
            ..Default::default()
        }
    }

    pub fn get_player_id(&self) -> i32 {
        self.id
            .unwrap_or_else(|| panic!("Player with UID {} has no ID", self.uid))
    }

    pub fn set_player_id(&mut self, pc_id: i32) {
        self.id = Some(pc_id);
    }

    pub fn set_client_id(&mut self, client_id: usize) {
        self.client_id = Some(client_id);
    }

    pub fn get_style(&self) -> sPCStyle {
        sPCStyle {
            iPC_UID: self.uid,
            iNameCheck: self.name.name_check,
            szFirstName: self.name.first_name,
            szLastName: self.name.last_name,
            iGender: self.style.gender,
            iFaceStyle: self.style.face_style,
            iHairStyle: self.style.hair_style,
            iHairColor: self.style.hair_color,
            iSkinColor: self.style.skin_color,
            iEyeColor: self.style.eye_color,
            iHeight: self.style.height,
            iBody: self.style.body,
            iClass: unused!(),
        }
    }

    pub fn get_style_2(&self) -> sPCStyle2 {
        sPCStyle2 {
            iAppearanceFlag: if self.flags.appearance_flag { 1 } else { 0 },
            iTutorialFlag: if self.flags.tutorial_flag { 1 } else { 0 },
            iPayzoneFlag: if self.flags.payzone_flag { 1 } else { 0 },
        }
    }

    fn get_mapnum(&self) -> i32 {
        self.instance_id.map_num as i32
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
                let nano = self.nano_data.nano_inventory[nano_id as usize].as_ref();
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
                let nano = self.nano_data.nano_inventory[nano_id as usize].as_mut();
                Some(nano.expect("Locked nano equipped"))
            }
            None => None,
        }
    }

    pub fn unlock_nano(&mut self, nano_id: i16) -> FFResult<&mut Nano> {
        let new_level = max(self.get_level(), nano_id);
        let nano_id = nano_id as usize;
        if nano_id >= SIZEOF_NANO_BANK_SLOT as usize {
            return Err(FFError::build(
                Severity::Warning,
                format!("Invalid nano ID: {}", nano_id),
            ));
        }
        self.nano_data.nano_inventory[nano_id] = Some(Nano::new(nano_id as i16));
        self.set_level(new_level);
        Ok(self.nano_data.nano_inventory[nano_id].as_mut().unwrap())
    }

    pub fn get_nano(&self, nano_id: i16) -> FFResult<&Nano> {
        let nano_id = nano_id as usize;
        if nano_id >= SIZEOF_NANO_BANK_SLOT as usize {
            return Err(FFError::build(
                Severity::Warning,
                format!("Invalid nano ID: {}", nano_id),
            ));
        }
        self.nano_data.nano_inventory[nano_id]
            .as_ref()
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Nano {} is locked", nano_id),
            ))
    }

    pub fn get_nano_mut(&mut self, nano_id: i16) -> FFResult<&mut Nano> {
        let nano_id = nano_id as usize;
        if nano_id >= SIZEOF_NANO_BANK_SLOT as usize {
            return Err(FFError::build(
                Severity::Warning,
                format!("Invalid nano ID: {}", nano_id),
            ));
        }
        self.nano_data.nano_inventory[nano_id]
            .as_mut()
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Nano {} is locked", nano_id),
            ))
    }

    pub fn tune_nano(&mut self, nano_id: i16, skill_selection: Option<usize>) -> FFResult<()> {
        let nano = self.get_nano_mut(nano_id)?;

        if let Some(skill_idx) = skill_selection {
            if skill_idx >= SIZEOF_NANO_SKILLS {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("Invalid nano skill index: {}", skill_idx),
                ));
            }
        }

        nano.tune(skill_selection);
        Ok(())
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
            iSpecialState: self.special_state,
            iMapNum: self.get_mapnum(),
            iX: self.position.x,
            iY: self.position.y,
            iZ: self.position.z,
            iAngle: self.rotation,
            aEquip: self.inventory.equipped.map(Option::<Item>::into),
            aInven: self.inventory.main.map(Option::<Item>::into),
            aQInven: self.inventory.mission.map(Option::<Item>::into),
            aNanoBank: self.nano_data.nano_inventory.map(Option::<Nano>::into),
            aNanoSlots: self.nano_data.equipped_ids.map(|id| id.unwrap_or(0) as u16),
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
            aQuestFlag: self.mission_data.mission_flags,
            aRepeatQuestFlag: self.mission_data.repeat_mission_flags,
            aRunningQuest: self
                .mission_data
                .current_missions
                .map(Option::<Mission>::into),
            iCurrentMissionID: self.mission_data.active_mission_id,
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

    pub fn get_appearance_data(&self) -> sPCAppearanceData {
        sPCAppearanceData {
            iID: self.id.unwrap_or_default(),
            PCStyle: self.get_style(),
            iConditionBitFlag: self.get_condition_bit_flag(),
            iPCState: self.get_state_bit_flag(),
            iSpecialState: self.special_state,
            iLv: self.level,
            iHP: self.hp,
            iMapNum: self.get_mapnum(),
            iX: self.position.x,
            iY: self.position.y,
            iZ: self.position.z,
            iAngle: self.rotation,
            ItemEquip: self.inventory.equipped.map(Option::<Item>::into),
            Nano: self.get_active_nano().copied().into(),
            eRT: unused!(),
        }
    }

    pub fn set_name(&mut self, name_check: i8, first_name: [u16; 9], last_name: [u16; 17]) {
        self.name = PlayerName {
            name_check,
            first_name,
            last_name,
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
            ItemLocation::QInven => {
                if slot_num < SIZEOF_QINVEN_SLOT as usize {
                    Ok(&self.inventory.mission[slot_num])
                } else {
                    err
                }
            }
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
            ItemLocation::QInven => {
                if slot_num < SIZEOF_QINVEN_SLOT as usize {
                    Ok(&mut self.inventory.mission[slot_num])
                } else {
                    err_oob
                }
            }
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

    pub fn find_free_slot(&self, location: ItemLocation) -> FFResult<usize> {
        let inven = match location {
            ItemLocation::Equip => self.inventory.equipped.as_slice(),
            ItemLocation::Inven => self.inventory.main.as_slice(),
            ItemLocation::QInven => self.inventory.mission.as_slice(),
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
        found.extend(
            self.find_items(ItemLocation::QInven, &f)
                .iter()
                .map(|slot_num| (ItemLocation::QInven, *slot_num)),
        );
        found
    }

    pub fn find_items(&self, location: ItemLocation, f: impl Fn(&Item) -> bool) -> Vec<usize> {
        let inven = match location {
            ItemLocation::Equip => self.inventory.equipped.as_slice(),
            ItemLocation::Inven => self.inventory.main.as_slice(),
            ItemLocation::QInven => self.inventory.mission.as_slice(),
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

    pub fn get_equipped(&self) -> [Option<Item>; 9] {
        self.inventory.equipped
    }

    pub fn get_taros(&self) -> u32 {
        self.taros
    }

    pub fn get_fusion_matter(&self) -> u32 {
        self.fusion_matter
    }

    pub fn update_special_state(&mut self, flags: i8) -> i8 {
        self.special_state ^= flags;
        self.special_state
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

    pub fn update_scamper_flags(&mut self, bit_offset: i32) -> FFResult<i32> {
        self.transport_data.set_scamper_flag(bit_offset)
    }

    pub fn update_skyway_flags(
        &mut self,
        bit_offset: i32,
    ) -> FFResult<[i64; WYVERN_LOCATION_FLAG_SIZE as usize]> {
        self.transport_data.set_skyway_flag(bit_offset)
    }

    pub fn set_creation_done(&mut self) {
        self.flags.appearance_flag = true;
    }

    pub fn set_tutorial_done(&mut self) {
        self.flags.tutorial_flag = true;
        // unlock buttercup
        self.unlock_nano(ID_BUTTERCUP).unwrap();
        self.tune_nano(ID_BUTTERCUP, Some(0)).unwrap();
        self.change_nano(0, Some(ID_BUTTERCUP)).unwrap();
        // equip lightning gun
        self.set_item(
            ItemLocation::Equip,
            EQUIP_SLOT_HAND as usize,
            Some(Item::new(ItemType::Hand, ID_LIGHTNING_GUN)),
        )
        .unwrap();
        // TODO delete all active missions
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

    pub fn set_god_mode(&mut self, god_mode: bool) {
        if god_mode {
            // max stats
            self.set_fusion_matter(PC_FUSIONMATTER_MAX);
            self.set_hp(i32::MAX);
            self.set_level(PC_LEVEL_MAX as i16);
            self.set_taros(PC_CANDY_MAX);
            self.set_tutorial_done();
            self.set_future_done();
            self.transport_data.set_all();
            self.flags.tip_flags = -1;

            // unlock all nanos, tune to first skill
            for i in 1..SIZEOF_NANO_BANK_SLOT as i16 {
                self.unlock_nano(i).unwrap();
                self.tune_nano(i, Some(0)).unwrap();
            }

            // fill empty nanocom slots with random nanos
            let mut rng = rand::thread_rng();
            for i in 0..SIZEOF_NANO_CARRY_SLOT as usize {
                if self.nano_data.equipped_ids[i].is_none() {
                    let nano_id = rng.gen_range(1..SIZEOF_NANO_BANK_SLOT as i16);
                    self.nano_data.equipped_ids[i] = Some(nano_id);
                }
            }
        } // TODO GM special state
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
            let client_other = clients.get_from_player_id(pc_id_other).unwrap();
            let pkt_cancel = sP_FE2CL_REP_PC_TRADE_CONFIRM_CANCEL {
                iID_Request: pc_id,
                iID_From: trade.get_id_from(),
                iID_To: trade.get_id_to(),
            };
            let _ = client_other.send_packet(P_FE2CL_REP_PC_TRADE_CONFIRM_CANCEL, &pkt_cancel);
        }
    }

    fn tick(&mut self, _time: SystemTime, _clients: &mut ClientMap, _state: &mut ShardServerState) {
        // TODO
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
