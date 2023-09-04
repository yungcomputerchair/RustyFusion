use std::fmt::Display;

use crate::{
    net::packet::{sPCLoadData2CL, sPCStyle, sPCStyle2, sTimeBuff},
    util::parse_utf16,
    CombatStats, Combatant, Item, Mission, Nano, Position,
};

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

struct PlayerFlags {
    appearance_flag: i8,
    tutorial_flag: i8,
    payzone_flag: i8,
    tip_flags: [i64; 2],
    scamper_flag: i32,
    skyway_flags: [i64; 2],
    mission_flag: [i64; 32],
    repeat_mission_flag: [i64; 8],
}

struct PlayerName {
    name_check: i8,
    first_name: [u16; 9],
    last_name: [u16; 17],
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

struct GuideData {
    current_guide: i16,
    total_guides: i16,
}

struct NanoData {
    nano_inventory: [Nano; 37],
    slot_nano_ids: [u16; 3],
    active_slot: i16,
}

struct MissionData {
    current_missions: [Mission; 9],
    active_mission_id: i32,
}

struct PlayerInventory {
    main: [Item; 50],
    equipped: [Item; 9],
    mission: [Item; 50],
}

pub struct Player {
    uid: i64,
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
    fn get_style(&self) -> sPCStyle {
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
            iClass: 0,
        }
    }

    fn get_style_2(&self) -> sPCStyle2 {
        sPCStyle2 {
            iAppearanceFlag: self.flags.appearance_flag,
            iTutorialFlag: self.flags.tutorial_flag,
            iPayzoneFlag: self.flags.payzone_flag,
        }
    }

    fn get_mapnum(&self) -> i32 {
        self.instance_id as i32
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
            aEquip: self.inventory.equipped.map(Item::into),
            aInven: self.inventory.main.map(Item::into),
            aQInven: self.inventory.mission.map(Item::into),
            aNanoBank: self.nano_data.nano_inventory.map(Nano::into),
            aNanoSlots: self.nano_data.slot_nano_ids,
            iActiveNanoSlotNum: self.nano_data.active_slot,
            iConditionBitFlag: self.get_condition_bit_flag(),
            eCSTB___Add: 0,
            TimeBuff: sTimeBuff {
                iTimeLimit: 0,
                iTimeDuration: 0,
                iTimeRepeat: 0,
                iValue: 0,
                iConfirmNum: 0,
            },
            aQuestFlag: self.flags.mission_flag,
            aRepeatQuestFlag: self.flags.repeat_mission_flag,
            aRunningQuest: self.mission_data.current_missions.map(Mission::into),
            iCurrentMissionID: self.mission_data.active_mission_id,
            iWarpLocationFlag: self.flags.scamper_flag,
            aWyvernLocationFlag: self.flags.skyway_flags,
            iBuddyWarpTime: self.buddy_warp_time,
            iFatigue: 0,
            iFatigue_Level: 0,
            iFatigueRate: 0,
            iFirstUseFlag1: self.flags.tip_flags[0],
            iFirstUseFlag2: self.flags.tip_flags[1],
            aiPCSkill: [0; 33],
        }
    }
}

impl Combatant for Player {
    fn get_condition_bit_flag(&self) -> i32 {
        0
    }

    fn get_combat_stats(&self) -> CombatStats {
        self.combat_stats
    }
}
