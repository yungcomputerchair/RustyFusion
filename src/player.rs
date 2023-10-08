use std::fmt::Display;

use crate::{
    defines::*,
    net::{
        ffclient::FFClient,
        packet::{sPCAppearanceData, sPCLoadData2CL, sPCStyle, sPCStyle2, sTimeBuff},
        ClientMap,
    },
    util::parse_utf16,
    CombatStats, Combatant, Entity, EntityID, Item, Mission, Nano, Position,
};

#[derive(Debug, Clone, Copy, Default)]
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

#[derive(Debug, Clone, Copy, Default)]
struct PlayerFlags {
    appearance_flag: i8,
    tutorial_flag: i8,
    payzone_flag: i8,
    tip_flags: [i64; 2],
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
    current_missions: [Mission; SIZEOF_RQUEST_SLOT as usize],
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
            style: PlayerStyle {
                gender: (rand::random::<bool>() as i8) + 1,
                ..Default::default()
            },
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
            iAppearanceFlag: self.flags.appearance_flag,
            iTutorialFlag: self.flags.tutorial_flag,
            iPayzoneFlag: self.flags.payzone_flag,
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
            aRunningQuest: self.mission_data.current_missions.map(Mission::into),
            iCurrentMissionID: self.mission_data.active_mission_id,
            iWarpLocationFlag: self.flags.scamper_flag,
            aWyvernLocationFlag: self.flags.skyway_flags,
            iBuddyWarpTime: self.buddy_warp_time,
            iFatigue: unused!(),
            iFatigue_Level: unused!(),
            iFatigueRate: unused!(),
            iFirstUseFlag1: self.flags.tip_flags[0],
            iFirstUseFlag2: self.flags.tip_flags[1],
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

    pub fn set_item(&mut self, mut slot_num: usize, item: Item) -> Option<Item> {
        if slot_num < SIZEOF_EQUIP_SLOT as usize {
            return self.inventory.equipped[slot_num].replace(item);
        }

        slot_num -= SIZEOF_EQUIP_SLOT as usize;
        if slot_num < SIZEOF_INVEN_SLOT as usize {
            return self.inventory.main[slot_num].replace(item);
        }

        slot_num -= SIZEOF_INVEN_SLOT as usize;
        if slot_num < SIZEOF_QINVEN_SLOT as usize {
            return self.inventory.mission[slot_num].replace(item);
        }

        slot_num -= SIZEOF_QINVEN_SLOT as usize;
        if slot_num < SIZEOF_BANK_SLOT as usize {
            return self.inventory.bank[slot_num].replace(item);
        }

        panic!("Inventory slot number {} out of range", slot_num);
    }
}
impl Combatant for Player {
    fn get_condition_bit_flag(&self) -> i32 {
        0
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
        if let Some(client_id) = self.client_id {
            return Some(client_map.get(client_id));
        }
        None
    }

    fn get_id(&self) -> EntityID {
        EntityID::Player(self.uid)
    }
}
