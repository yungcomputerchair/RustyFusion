use serde::{Deserialize, Serialize};

use crate::{
    net::packet::{sItemBase, sNano, sPCStyle},
    player::{PlayerFlags, PlayerStyle},
    util, Combatant, Entity, Item, Nano, Position,
};

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DbAccount {
    #[serde(rename = "_id")]
    account_id: BigInt,
    username: Text,
    password_hash: Text,
    player_uids: [BigInt; 4], // references to player collection
    selected_slot: Int,
    account_level: Int,
    creation_time: Int,
    last_login_time: Int,
    banned_until_time: Int,
    banned_since_time: Int,
    ban_reason: Text,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DbItem {
    slot_number: Int,
    id: Int,
    ty: Int,
    opt: Int,
    time_limit: Int,
}
impl From<(usize, &Item)> for DbItem {
    fn from(values: (usize, &Item)) -> Self {
        let (slot_number, item) = values;
        let item_raw: sItemBase = Some(*item).into();
        Self {
            slot_number: slot_number as Int,
            id: item_raw.iID as Int,
            ty: item_raw.iType as Int,
            opt: item_raw.iOpt,
            time_limit: item_raw.iTimeLimit,
        }
    }
}
impl TryFrom<DbItem> for (usize, Option<Item>) {
    type Error = FFError;
    fn try_from(item: DbItem) -> FFResult<Self> {
        let slot_num = item.slot_number;
        let item_raw = sItemBase {
            iID: item.id as i16,
            iType: item.ty as i16,
            iOpt: item.opt,
            iTimeLimit: item.time_limit,
        };
        let item: Option<Item> = item_raw.try_into()?;
        Ok((slot_num as usize, item))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DbNano {
    id: Int,
    skill_id: Int,
    stamina: Int,
}
impl From<&Nano> for DbNano {
    fn from(nano: &Nano) -> Self {
        let nano_raw: sNano = Some(nano.clone()).into();
        Self {
            id: nano_raw.iID as Int,
            skill_id: nano_raw.iSkillID as Int,
            stamina: nano_raw.iStamina as Int,
        }
    }
}
impl TryFrom<DbNano> for Option<Nano> {
    type Error = FFError;
    fn try_from(nano: DbNano) -> FFResult<Self> {
        let nano_raw = sNano {
            iID: nano.id as i16,
            iSkillID: nano.skill_id as i16,
            iStamina: nano.stamina as i16,
        };
        let nano: Option<Nano> = nano_raw.try_into()?;
        Ok(nano)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DbStyle {
    body: Int,
    eye_color: Int,
    face_style: Int,
    gender: Int,
    hair_color: Int,
    hair_style: Int,
    height: Int,
    skin_color: Int,
}
impl From<&PlayerStyle> for DbStyle {
    fn from(style: &PlayerStyle) -> Self {
        Self {
            body: style.body as Int,
            eye_color: style.eye_color as Int,
            face_style: style.face_style as Int,
            gender: style.gender as Int,
            hair_color: style.hair_color as Int,
            hair_style: style.hair_style as Int,
            height: style.height as Int,
            skin_color: style.skin_color as Int,
        }
    }
}
impl TryFrom<DbStyle> for PlayerStyle {
    type Error = FFError;

    fn try_from(style: DbStyle) -> FFResult<Self> {
        let style_raw = sPCStyle {
            iGender: style.gender as i8,
            iFaceStyle: style.face_style as i8,
            iHairStyle: style.hair_style as i8,
            iHairColor: style.hair_color as i8,
            iSkinColor: style.skin_color as i8,
            iEyeColor: style.eye_color as i8,
            iHeight: style.height as i8,
            iBody: style.body as i8,
            iClass: unused!(),
            iPC_UID: unused!(),
            iNameCheck: unused!(),
            szFirstName: unused!(),
            szLastName: unused!(),
        };
        style_raw.try_into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DbPlayer {
    #[serde(rename = "_id")]
    uid: BigInt,
    account_id: BigInt, // reference to account collection
    first_name: String,
    last_name: String,
    name_check: Int,
    style: Option<DbStyle>,
    tutorial_flag: Int,
    payzone_flag: Int,
    level: Int,
    equipped_nano_ids: [Int; 3],
    pos: [Int; 3],
    angle: Int,
    hp: Int,
    fusion_matter: Int,
    taros: Int,
    weapon_boosts: Int,
    nano_potions: Int,
    guide: Int,
    active_mission_id: Int,
    scamper_flags: Int,
    skyway_bytes: Bytes,
    tip_flags_bytes: Bytes,
    quest_bytes: Bytes,
    nanos: Vec<DbNano>,
    items: Vec<DbItem>,
}
impl From<(BigInt, &Player)> for DbPlayer {
    fn from(values: (BigInt, &Player)) -> Self {
        let (account_id, player) = values;

        let mut skyway_bytes = Vec::new();
        for sec in player.get_skyway_flags() {
            skyway_bytes.extend_from_slice(&sec.to_le_bytes());
        }

        let mut quest_bytes = Vec::new();
        for sec in player.get_mission_flags() {
            quest_bytes.extend_from_slice(&sec.to_le_bytes());
        }

        let position = if player.instance_id.instance_num.is_some() {
            player.get_pre_warp().position
        } else {
            player.get_position()
        };

        let nanos: Vec<DbNano> = player.get_nano_iter().map(|nano| nano.into()).collect();
        let items: Vec<DbItem> = player
            .get_item_iter()
            .map(|(slot_num, item)| (slot_num, item).into())
            .collect();

        Self {
            uid: player.get_uid(),
            account_id,
            first_name: player.get_first_name(),
            last_name: player.get_last_name(),
            name_check: placeholder!(1) as Int,
            style: player.style.as_ref().map(|style| style.into()),
            level: player.get_level() as Int,
            equipped_nano_ids: player.get_equipped_nano_ids().map(|nid| nid as Int),
            tutorial_flag: player.flags.tutorial_flag as Int,
            payzone_flag: player.flags.payzone_flag as Int,
            pos: [position.x, position.y, position.z],
            angle: player.get_rotation(),
            hp: player.get_hp(),
            fusion_matter: player.get_fusion_matter() as Int,
            taros: player.get_taros() as Int,
            weapon_boosts: player.get_weapon_boosts() as Int,
            nano_potions: player.get_nano_potions() as Int,
            guide: (player.get_guide() as i16) as Int,
            active_mission_id: player.get_active_mission_id(),
            scamper_flags: player.get_scamper_flags(),
            skyway_bytes,
            tip_flags_bytes: player.flags.tip_flags.to_le_bytes().to_vec(),
            quest_bytes,
            //
            nanos,
            items,
        }
    }
}
impl TryFrom<(usize, DbPlayer)> for Player {
    type Error = FFError;

    fn try_from(values: (usize, DbPlayer)) -> FFResult<Self> {
        let (slot_num, db_player) = values;

        let mut player = Player::new(db_player.uid, slot_num);
        player.style = if let Some(style) = db_player.style {
            Some(style.try_into()?)
        } else {
            None
        };

        player.set_name(
            db_player.name_check as i8,
            util::encode_utf16(&db_player.first_name),
            util::encode_utf16(&db_player.last_name),
        );

        player.set_position(Position {
            x: db_player.pos[0],
            y: db_player.pos[1],
            z: db_player.pos[2],
        });
        player.set_rotation(db_player.angle);

        player.set_taros(db_player.taros as u32);
        player.set_fusion_matter(db_player.fusion_matter as u32);
        player.set_level(db_player.level as i16);
        player.set_hp(db_player.hp);
        player.set_weapon_boosts(db_player.weapon_boosts as u32);
        player.set_nano_potions(db_player.nano_potions as u32);

        for (slot, nano_id) in db_player.equipped_nano_ids.into_iter().enumerate() {
            player
                .change_nano(
                    slot,
                    if nano_id == 0 {
                        None
                    } else {
                        Some(nano_id as i16)
                    },
                )
                .unwrap();
        }
        for nano in db_player.nanos {
            let nano: FFResult<Option<Nano>> = nano.try_into();
            if let Err(e) = nano {
                log_error(&e);
                continue;
            }
            if let Some(nano) = nano.unwrap() {
                player.set_nano(nano);
            }
        }

        let first_use_bytes: &[u8] = &db_player.tip_flags_bytes;
        player.flags = PlayerFlags {
            tutorial_flag: db_player.tutorial_flag != 0,
            payzone_flag: db_player.payzone_flag != 0,
            tip_flags: i128::from_le_bytes(first_use_bytes[..16].try_into().unwrap()),
        };

        let skyway_bytes: &[u8] = &db_player.skyway_bytes;
        player.set_skyway_flags([
            i64::from_le_bytes(skyway_bytes[..8].try_into().unwrap()),
            i64::from_le_bytes(skyway_bytes[8..16].try_into().unwrap()),
        ]);
        player.set_scamper_flag(db_player.scamper_flags);

        for item in db_player.items {
            let values: FFResult<(usize, Option<Item>)> = item.try_into();
            if let Err(e) = values {
                log_error(&e);
                continue;
            }
            let (slot_num, item) = values.unwrap();
            let inv_loc = util::slot_num_to_loc_and_slot_num(slot_num);
            if let Err(e) = inv_loc {
                log_error(&e);
                continue;
            }
            let (loc, slot_num) = inv_loc.unwrap();
            log_if_failed(player.set_item(loc, slot_num, item));
        }

        Ok(player)
    }
}
