use std::{any::Any, fmt::Display};

use crate::{
    chunk::{pos_to_chunk_coords, EntityMap},
    defines::*,
    enums::eItemLocation,
    error::SimpleError,
    net::{
        ffclient::FFClient,
        packet::{
            sPCAppearanceData, sPCLoadData2CL, sPCStyle, sPCStyle2, sP_FE2CL_PC_EXIT,
            sP_FE2CL_PC_NEW, sTimeBuff, PacketID,
        },
        ClientMap,
    },
    util::parse_utf16,
    CombatStats, Combatant, Entity, EntityID, Item, Mission, Nano, Position, Result,
};

use num_traits::ToPrimitive;

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
            gender: 1,
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
    scamper_flag: i32,
    skyway_flags: [i64; WYVERN_LOCATION_FLAG_SIZE as usize],
    mission_flag: [i64; SIZEOF_QUESTFLAG_NUMBER as usize],
    repeat_mission_flag: [i64; SIZEOF_REPEAT_QUESTFLAG_NUMBER as usize],
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
    taros: i32,
    fusion_matter: i32,
    nano_potions: i32,
    weapon_boosts: i32,
    buddy_warp_time: i32,
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
            iBatteryW: self.weapon_boosts,
            iBatteryN: self.nano_potions,
            iCandy: self.taros,
            iFusionMatter: self.fusion_matter,
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
            aQuestFlag: self.flags.mission_flag,
            aRepeatQuestFlag: self.flags.repeat_mission_flag,
            aRunningQuest: self
                .mission_data
                .current_missions
                .map(Option::<Mission>::into),
            iCurrentMissionID: self.mission_data.active_mission_id,
            iWarpLocationFlag: self.flags.scamper_flag,
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
            iID: self.uid as i32,
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

    pub fn set_item_with_location(
        &mut self,
        location: eItemLocation,
        slot_num: usize,
        item: Option<Item>,
    ) -> Result<Option<Item>> {
        let mut slot_from = None;
        match location {
            eItemLocation::eIL_Equip => {
                if slot_num < SIZEOF_EQUIP_SLOT as usize {
                    slot_from = Some(&mut self.inventory.equipped[slot_num]);
                }
            }
            eItemLocation::eIL_Inven => {
                if slot_num < SIZEOF_INVEN_SLOT as usize {
                    slot_from = Some(&mut self.inventory.main[slot_num]);
                }
            }
            eItemLocation::eIL_QInven => {
                if slot_num < SIZEOF_QINVEN_SLOT as usize {
                    slot_from = Some(&mut self.inventory.mission[slot_num]);
                }
            }
            eItemLocation::eIL_Bank => {
                if slot_num < SIZEOF_BANK_SLOT as usize {
                    slot_from = Some(&mut self.inventory.bank[slot_num]);
                }
            }
            eItemLocation::eIL__End => {}
        }

        if let Some(slot_from) = slot_from {
            let old_item = slot_from.take();
            *slot_from = item;
            Ok(old_item)
        } else {
            Err(SimpleError::build(format!(
                "Bad slot number: {slot_num} (location {})",
                location.to_i32().unwrap_or(-1)
            )))
        }
    }

    pub fn set_item(&mut self, mut slot_num: usize, item: Option<Item>) -> Result<Option<Item>> {
        if slot_num < SIZEOF_EQUIP_SLOT as usize {
            return self.set_item_with_location(eItemLocation::eIL_Equip, slot_num, item);
        }

        slot_num -= SIZEOF_EQUIP_SLOT as usize;
        if slot_num < SIZEOF_INVEN_SLOT as usize {
            return self.set_item_with_location(eItemLocation::eIL_Inven, slot_num, item);
        }

        slot_num -= SIZEOF_INVEN_SLOT as usize;
        if slot_num < SIZEOF_QINVEN_SLOT as usize {
            return self.set_item_with_location(eItemLocation::eIL_QInven, slot_num, item);
        }

        slot_num -= SIZEOF_QINVEN_SLOT as usize;
        if slot_num < SIZEOF_BANK_SLOT as usize {
            return self.set_item_with_location(eItemLocation::eIL_Bank, slot_num, item);
        }

        Err(SimpleError::build(format!("Bad slot number: {slot_num}")))
    }

    pub fn get_equipped(&self) -> [Option<Item>; 9] {
        self.inventory.equipped
    }

    pub fn update_special_state(&mut self, flags: i8) -> i8 {
        self.special_state ^= flags;
        self.special_state
    }

    pub fn update_first_use_flag(&mut self, bit_offset: i32) -> i128 {
        self.flags.tip_flags |= 1_i128 << (bit_offset - 1);
        self.flags.tip_flags
    }

    pub fn set_appearance_flag(&mut self) {
        self.flags.appearance_flag = true;
    }

    pub fn set_tutorial_flag(&mut self) {
        self.flags.tutorial_flag = true;
    }

    pub fn set_taros(&mut self, taros: i32) {
        self.taros = taros;
    }

    pub fn set_hp(&mut self, hp: i32) {
        self.combat_stats.hp = hp;
    }

    pub fn set_fusion_matter(&mut self, fusion_matter: i32) {
        self.fusion_matter = fusion_matter;
    }

    pub fn set_weapon_boosts(&mut self, weapon_boosts: i32) {
        self.weapon_boosts = weapon_boosts;
    }

    pub fn set_nano_potions(&mut self, nano_potions: i32) {
        self.nano_potions = nano_potions;
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
        EntityID::Player(self.uid)
    }

    fn get_position(&self) -> Position {
        self.position
    }

    fn set_position(
        &mut self,
        pos: Position,
        entity_map: &mut EntityMap,
        client_map: &mut ClientMap,
    ) {
        self.position = pos;
        let chunk = pos_to_chunk_coords(self.position);
        entity_map.update(self.get_id(), Some(chunk), Some(client_map));
    }

    fn set_rotation(&mut self, angle: i32) {
        self.rotation = angle % 360;
    }

    fn send_enter(&self, client: &mut FFClient) -> Result<()> {
        let pkt = sP_FE2CL_PC_NEW {
            PCAppearanceData: self.get_appearance_data(),
        };
        client.send_packet(PacketID::P_FE2CL_PC_NEW, &pkt)?;
        Ok(())
    }

    fn send_exit(&self, client: &mut FFClient) -> Result<()> {
        let pkt = sP_FE2CL_PC_EXIT {
            iID: self.uid as i32,
            iExitType: unused!(),
        };
        client.send_packet(PacketID::P_FE2CL_PC_EXIT, &pkt)?;
        Ok(())
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}
