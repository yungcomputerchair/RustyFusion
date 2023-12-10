use std::{any::Any, fmt::Display};

use crate::{
    defines::*,
    enums::ItemLocation,
    error::{FFError, FFResult, Severity},
    net::{
        ffclient::FFClient,
        packet::{
            sPCAppearanceData, sPCLoadData2CL, sPCStyle, sPCStyle2, sP_FE2CL_PC_EXIT,
            sP_FE2CL_PC_NEW, sTimeBuff, PacketID,
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
struct NanoData {
    nano_inventory: [Nano; SIZEOF_NANO_BANK_SLOT as usize],
    slot_nano_ids: [u16; SIZEOF_NANO_CARRY_SLOT as usize],
    active_slot: i16,
}
impl Default for NanoData {
    fn default() -> Self {
        Self {
            nano_inventory: [Nano::default(); SIZEOF_NANO_BANK_SLOT as usize],
            slot_nano_ids: Default::default(),
            active_slot: Default::default(),
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
    nano_data: NanoData,
    mission_data: MissionData,
    inventory: PlayerInventory,
    taros: u32,
    fusion_matter: u32,
    nano_potions: u32,
    weapon_boosts: u32,
    buddy_warp_time: i32,
    pub trade_id: Option<Uuid>,
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

    fn get_active_nano(&self) -> Option<Nano> {
        if self.nano_data.active_slot == -1 {
            return None;
        }
        Some(
            self.nano_data.nano_inventory
                [self.nano_data.slot_nano_ids[self.nano_data.active_slot as usize] as usize],
        )
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
            aNanoBank: self.nano_data.nano_inventory.map(Nano::into),
            aNanoSlots: self.nano_data.slot_nano_ids,
            iActiveNanoSlotNum: self.nano_data.active_slot,
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
            Nano: self.get_active_nano().unwrap_or_default().into(),
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
                self.get_player_id()
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
            self.flags.tip_flags |= 1_i128 << (bit_offset - 1);
            Ok(self.flags.tip_flags)
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

    fn cleanup(&mut self, state: &mut ShardServerState) {
        let pc_id = self.get_player_id();
        if state.buyback_lists.contains_key(&pc_id) {
            state.buyback_lists.remove(&pc_id);
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
