use std::{any::Any, fmt::Display};

use crate::{
    defines::*,
    enums::ItemLocation,
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
    CombatStats, Combatant, Entity, EntityID, Item, Mission, Nano, Position,
};

use num_traits::{clamp, clamp_min};
use uuid::Uuid;

pub const TEST_ACC_UID_START: i64 = i64::MAX - 3;

#[derive(Debug, Clone, Copy)]
struct PlayerStyle {
    gender: i8,
    face_style: i8,
    hair_style: i8,
    hair_color: i8,
    skin_color: i8,
    eye_color: i8,
    height: i8,
    body: i8,
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
struct PlayerFlags {
    appearance_flag: bool,
    tutorial_flag: bool,
    payzone_flag: bool,
    tip_flags: i128,
    scamper_flags: i32,
    skyway_flags: [i64; WYVERN_LOCATION_FLAG_SIZE as usize],
    mission_flags: [i64; SIZEOF_QUESTFLAG_NUMBER as usize],
    repeat_mission_flags: [i64; SIZEOF_REPEAT_QUESTFLAG_NUMBER as usize],
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

#[derive(Debug, Clone, Copy, Default)]
struct GuideData {
    current_guide: i16,
    total_guides: i16,
}

#[derive(Debug, Clone, Copy)]
struct Nanocom {
    nano_inventory: [Option<Nano>; SIZEOF_NANO_BANK_SLOT as usize],
    equipped_ids: [Option<u16>; SIZEOF_NANO_CARRY_SLOT as usize],
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
    instance_id: u64,
    style: PlayerStyle,
    flags: PlayerFlags,
    name: PlayerName,
    special_state: i8,
    combat_stats: CombatStats,
    guide_data: GuideData,
    nano_data: Nanocom,
    mission_data: MissionData,
    inventory: PlayerInventory,
    taros: u32,
    fusion_matter: u32,
    nano_potions: u32,
    weapon_boosts: u32,
    buddy_warp_time: i32,
    pub trade_id: Option<Uuid>,
    pub trade_offered_to: Option<i32>,
}
impl Player {
    pub fn new(uid: i64) -> Self {
        Self {
            uid,
            client_id: None,
            combat_stats: CombatStats {
                _max_hp: placeholder!(100),
                hp: placeholder!(100),
                level: 1,
            },
            position: Position {
                x: placeholder!(632032),
                y: placeholder!(187177),
                z: placeholder!(-5500),
            },
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

    pub fn set_style(&mut self, style: sPCStyle) {
        self.style = PlayerStyle {
            gender: style.iGender,
            face_style: style.iFaceStyle,
            hair_style: style.iHairStyle,
            hair_color: style.iHairColor,
            skin_color: style.iSkinColor,
            eye_color: style.iEyeColor,
            height: style.iHeight,
            body: style.iBody,
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
        self.instance_id as i32
    }

    pub fn change_nano(&mut self, slot: usize, nano_id: Option<u16>) -> FFResult<()> {
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

    pub fn set_active_nano_slot(&mut self, slot: Option<usize>) {
        self.nano_data.active_slot = slot;
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

    pub fn unlock_nano(&mut self, nano_id: usize, selected_skill: usize) -> FFResult<()> {
        if nano_id >= SIZEOF_NANO_BANK_SLOT as usize {
            return Err(FFError::build(
                Severity::Warning,
                format!("Invalid nano ID: {}", nano_id),
            ));
        }
        self.nano_data.nano_inventory[nano_id] = Some(Nano::new(nano_id as i16, selected_skill)?);
        Ok(())
    }

    pub fn get_load_data(&self) -> sPCLoadData2CL {
        sPCLoadData2CL {
            iUserLevel: self.perms,
            PCStyle: self.get_style(),
            PCStyle2: self.get_style_2(),
            iLevel: self.combat_stats.level,
            iMentor: self.guide_data.current_guide,
            iMentorCount: self.guide_data.total_guides,
            iHP: self.combat_stats.hp,
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
            aNanoSlots: self.nano_data.equipped_ids.map(|id| id.unwrap_or(0)),
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
            aQuestFlag: self.flags.mission_flags,
            aRepeatQuestFlag: self.flags.repeat_mission_flags,
            aRunningQuest: self
                .mission_data
                .current_missions
                .map(Option::<Mission>::into),
            iCurrentMissionID: self.mission_data.active_mission_id,
            iWarpLocationFlag: self.flags.scamper_flags,
            aWyvernLocationFlag: self.flags.skyway_flags,
            iBuddyWarpTime: self.buddy_warp_time,
            iFatigue: unused!(),
            iFatigue_Level: unused!(),
            iFatigueRate: unused!(),
            iFirstUseFlag1: self.flags.tip_flags as i64,
            iFirstUseFlag2: (self.flags.tip_flags >> 8) as i64,
            aiPCSkill: [unused!(); 33],
        }
    }

    pub fn get_appearance_data(&self) -> sPCAppearanceData {
        sPCAppearanceData {
            iID: self.id.unwrap_or_default(),
            PCStyle: self.get_style(),
            iConditionBitFlag: self.get_condition_bit_flag(),
            iPCState: placeholder!(0),
            iSpecialState: self.special_state,
            iLv: self.combat_stats.level,
            iHP: self.combat_stats.hp,
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
        self.flags.scamper_flags
    }

    pub fn get_skyway_flags(&self) -> [i64; WYVERN_LOCATION_FLAG_SIZE as usize] {
        self.flags.skyway_flags
    }

    pub fn update_scamper_flags(&mut self, bit_offset: i32) -> FFResult<i32> {
        if !(1..=32).contains(&bit_offset) {
            Err(FFError::build(
                Severity::Warning,
                format!("Scamper flag offset out of range: {}", bit_offset),
            ))
        } else {
            self.flags.scamper_flags |= 1 << (bit_offset - 1);
            Ok(self.flags.scamper_flags)
        }
    }

    pub fn update_skyway_flags(
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
            self.flags.skyway_flags[idx] = 1 << offset;
            Ok(self.flags.skyway_flags)
        }
    }

    pub fn set_appearance_flag(&mut self) {
        self.flags.appearance_flag = true;
    }

    pub fn set_tutorial_flag(&mut self) {
        self.flags.tutorial_flag = true;
    }

    pub fn set_payzone_flag(&mut self) {
        self.flags.payzone_flag = true;
    }

    pub fn set_taros(&mut self, taros: u32) -> u32 {
        self.taros = clamp(taros, 0, PC_CANDY_MAX);
        self.taros
    }

    pub fn set_hp(&mut self, hp: i32) -> i32 {
        self.combat_stats.hp = clamp_min(hp, 0);
        self.combat_stats.hp
    }

    pub fn set_level(&mut self, level: i16) -> i16 {
        self.combat_stats.level = clamp(level, 1, PC_LEVEL_MAX as i16);
        self.combat_stats.level
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
            self.set_appearance_flag();
            self.set_tutorial_flag();
            self.set_payzone_flag();
            self.flags.scamper_flags = -1;
            self.flags.tip_flags = -1;
            self.flags.skyway_flags = [-1; WYVERN_LOCATION_FLAG_SIZE as usize];

            // unlock all nanos
            for i in 1..SIZEOF_NANO_BANK_SLOT as usize {
                self.unlock_nano(i, 0).unwrap();
            }
        } // TODO GM special state
    }
}
impl Combatant for Player {
    fn get_condition_bit_flag(&self) -> i32 {
        placeholder!(0)
    }

    fn get_level(&self) -> i16 {
        self.combat_stats.level
    }

    fn get_hp(&self) -> i32 {
        self.combat_stats.hp
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

    fn set_position(&mut self, pos: Position) -> (i32, i32) {
        self.position = pos;
        self.position.chunk_coords()
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

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
