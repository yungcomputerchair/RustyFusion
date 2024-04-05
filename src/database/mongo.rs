use std::time::SystemTime;

use mongodb::{
    bson::doc,
    sync::{Client, ClientSession},
};
use serde::{Deserialize, Serialize};

use crate::{
    database::*,
    defines::*,
    entity::{Combatant, Entity, PlayerFlags, PlayerStyle},
    enums::PlayerGuide,
    item::Item,
    mission::Task,
    nano::Nano,
    net::packet::*,
    tabledata::tdata_get,
    util, Position,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DbMeta {
    db_version: Int,
    protocol_version: Int,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DbAccount {
    #[serde(rename = "_id")]
    account_id: BigInt,
    username: Text,
    password_hash: Text,
    player_uids: Vec<BigInt>, // references to player collection
    selected_slot: Int,
    account_level: Int,
    creation_time: Int,
    last_login_time: Int,
    banned_until_time: Int,
    banned_since_time: Int,
    ban_reason: Text,
}
impl From<DbAccount> for Account {
    fn from(acc: DbAccount) -> Self {
        Self {
            id: acc.account_id,
            username: acc.username,
            password_hashed: acc.password_hash,
            selected_slot: acc.selected_slot as u8,
            account_level: acc.account_level as i16,
            banned_until: util::get_systime_from_sec(acc.banned_until_time as u64),
            ban_reason: acc.ban_reason,
        }
    }
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
struct DbQuestItem {
    id: Int,
    count: Int,
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
impl From<DbNano> for Option<Nano> {
    fn from(nano: DbNano) -> Self {
        let nano_raw = sNano {
            iID: nano.id as i16,
            iSkillID: nano.skill_id as i16,
            iStamina: nano.stamina as i16,
        };
        nano_raw.into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DbTask {
    task_id: Int,
    npc_remaining_counts: [Int; 3],
}
impl From<&Task> for DbTask {
    fn from(task: &Task) -> Self {
        Self {
            task_id: task.get_task_id(),
            npc_remaining_counts: task.get_remaining_enemy_defeats().map(|c| c as Int),
        }
    }
}
impl TryFrom<DbTask> for Task {
    type Error = FFError;
    fn try_from(task: DbTask) -> FFResult<Self> {
        let npc_remaining_counts = task.npc_remaining_counts.map(|c| c as usize);
        let task_def = tdata_get().get_task_definition(task.task_id)?;
        let mut task: Task = task_def.into();
        task.set_remaining_enemy_defeats(npc_remaining_counts);
        Ok(task)
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
    save_time: Int,
    slot_number: Int,
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
    nanos: Option<Vec<DbNano>>,
    items: Option<Vec<DbItem>>,
    quest_items: Option<Vec<DbQuestItem>>,
    running_quests: Option<Vec<DbTask>>,
}
impl From<(BigInt, &Player, Int)> for DbPlayer {
    fn from(values: (BigInt, &Player, Int)) -> Self {
        let (account_id, player, save_time) = values;

        let mut skyway_bytes = Vec::new();
        for sec in player.get_skyway_flags() {
            skyway_bytes.extend_from_slice(&sec.to_le_bytes());
        }

        let mut quest_bytes = Vec::new();
        for sec in player.mission_journal.completed_mission_flags {
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
        let quest_items: Vec<DbQuestItem> = player
            .get_quest_item_iter()
            .map(|(item_id, count)| DbQuestItem {
                id: item_id as Int,
                count: count as Int,
            })
            .collect();
        let running_quests: Vec<DbTask> = player
            .mission_journal
            .get_current_tasks()
            .iter()
            .map(|task| task.into())
            .collect();

        Self {
            uid: player.get_uid(),
            account_id,
            save_time,
            slot_number: player.get_slot_num() as Int,
            first_name: player.first_name.clone(),
            last_name: player.last_name.clone(),
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
            active_mission_id: player.mission_journal.get_active_mission_id().unwrap_or(0),
            scamper_flags: player.get_scamper_flags(),
            skyway_bytes,
            tip_flags_bytes: player.flags.tip_flags.to_le_bytes().to_vec(),
            quest_bytes,
            //
            nanos: Some(nanos),
            items: Some(items),
            quest_items: Some(quest_items),
            running_quests: Some(running_quests),
        }
    }
}
impl TryFrom<(DbPlayer, i16)> for Player {
    type Error = FFError;

    fn try_from(values: (DbPlayer, i16)) -> FFResult<Self> {
        let (db_player, perms) = values;
        let mut player = Player::new(db_player.uid, db_player.slot_number as usize, perms);
        player.style = if let Some(style) = db_player.style {
            Some(style.try_into()?)
        } else {
            None
        };

        player.first_name = db_player.first_name;
        player.last_name = db_player.last_name;

        player.set_position(Position {
            x: db_player.pos[0],
            y: db_player.pos[1],
            z: db_player.pos[2],
        });
        player.set_rotation(db_player.angle);

        player.set_taros(db_player.taros as u32);
        player.set_level(db_player.level as i16)?;
        // fusion matter must be set after level
        player.set_fusion_matter(db_player.fusion_matter as u32, None);
        player.set_hp(db_player.hp);
        player.set_weapon_boosts(db_player.weapon_boosts as u32);
        player.set_nano_potions(db_player.nano_potions as u32);

        for (slot, nano_id) in db_player.equipped_nano_ids.into_iter().enumerate() {
            player.change_nano(
                slot,
                if nano_id == 0 {
                    None
                } else {
                    Some(nano_id as i16)
                },
            )?;
        }
        for nano in db_player.nanos.unwrap_or_default() {
            let nano: Option<Nano> = nano.into();
            if let Some(nano) = nano {
                player.set_nano(nano);
            }
        }

        let first_use_bytes: &[u8] = &db_player.tip_flags_bytes;
        player.flags = PlayerFlags {
            name_check_flag: db_player.name_check != 0,
            tutorial_flag: db_player.tutorial_flag != 0,
            payzone_flag: db_player.payzone_flag != 0,
            tip_flags: i128::from_le_bytes(first_use_bytes[..16].try_into().unwrap()),
        };

        // TODO get total number of guides from DB (currently not stored)
        let guide: PlayerGuide = (db_player.guide as i16).try_into()?;
        if guide != PlayerGuide::Computress {
            player.update_guide(guide);
        }

        let skyway_bytes: &[u8] = &db_player.skyway_bytes;
        player.set_skyway_flags([
            i64::from_le_bytes(skyway_bytes[..8].try_into().unwrap()),
            i64::from_le_bytes(skyway_bytes[8..16].try_into().unwrap()),
        ]);
        player.set_scamper_flag(db_player.scamper_flags);

        for item in db_player.items.unwrap_or_default() {
            let values: (usize, Option<Item>) = item.try_into()?;
            let (slot_num, item) = values;
            if item.is_some_and(|item| {
                item.get_expiry_time()
                    .is_some_and(|et| et < SystemTime::now())
            }) {
                // item is expired; skip it
                continue;
            }

            let (loc, slot_num) = util::slot_num_to_loc_and_slot_num(slot_num)?;
            player.set_item(loc, slot_num, item)?;
        }

        for quest_item in db_player.quest_items.unwrap_or_default() {
            player.set_quest_item_count(quest_item.id as i16, quest_item.count as usize)?;
        }

        let quest_bytes: &[u8] = &db_player.quest_bytes;
        for i in 0..player.mission_journal.completed_mission_flags.len() {
            player.mission_journal.completed_mission_flags[i] =
                BigInt::from_le_bytes(quest_bytes[i * 8..(i + 1) * 8].try_into().unwrap());
        }

        for task in db_player.running_quests.unwrap_or_default() {
            let mut task: Task = task.try_into()?;
            task.fail_time = None;
            player.mission_journal.start_task(task)?;
        }

        if db_player.active_mission_id != 0 {
            log_if_failed(
                player
                    .mission_journal
                    .set_active_mission_id(db_player.active_mission_id),
            );
        }

        Ok(player)
    }
}

impl FFError {
    fn from_db_error(e: mongodb::error::Error) -> Self {
        FFError::build(Severity::Warning, format!("Database error: {}", e))
    }
}

pub struct MongoDatabase {
    db: mongodb::sync::Database,
    client: mongodb::sync::Client,
    conn_str: String,
}
impl std::fmt::Debug for MongoDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Mongo Database ({})", self.conn_str)
    }
}
impl MongoDatabase {
    pub fn connect(config: &GeneralConfig) -> FFResult<Box<dyn Database>> {
        match Self::connect_internal(config, true) {
            Ok(db) => Ok(db),
            Err(e) => {
                log_error(&e);
                log(
                    Severity::Info,
                    "Attempting connection without authentication...",
                );
                Self::connect_internal(config, false)
            }
        }
    }

    fn connect_internal(config: &GeneralConfig, do_auth: bool) -> FFResult<Box<dyn Database>> {
        let conn_str = if do_auth {
            format!(
                "mongodb://{}:{}@{}:{}",
                config.db_username.get(),
                config.db_password.get(),
                config.db_host.get(),
                config.db_port.get(),
            )
        } else {
            format!(
                "mongodb://{}:{}",
                config.db_host.get(),
                config.db_port.get(),
            )
        };
        let client = Client::with_uri_str(&conn_str).map_err(FFError::from_db_error)?;

        // check if the meta table exists and create it if it doesn't
        let db = client.database(DB_NAME);
        let meta = db.collection::<DbMeta>("meta");
        if meta
            .find_one(None, None)
            .map_err(FFError::from_db_error)?
            .is_none()
        {
            log(
                Severity::Info,
                "Meta table missing; initializing database...",
            );
            meta.insert_one(
                DbMeta {
                    db_version: DB_VERSION,
                    protocol_version: PROTOCOL_VERSION,
                },
                None,
            )
            .map_err(FFError::from_db_error)?;
        }

        Ok(Box::new(Self {
            db,
            client,
            conn_str,
        }))
    }

    fn save_player_internal(
        &mut self,
        player: &Player,
        tsct: &mut ClientSession,
        state_timestamp: Int,
    ) -> FFResult<()> {
        let pc_uid = player.get_uid();
        // find the existing player so we can grab the account ID
        let existing_player = self
            .db
            .collection::<DbPlayer>("players")
            .find_one_with_session(doc! { "_id": pc_uid }, None, tsct)
            .map_err(FFError::from_db_error)?
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Player with UID {} not found in database", pc_uid),
            ))?;

        if existing_player.save_time >= state_timestamp {
            return Ok(());
        }

        let player: DbPlayer = (existing_player.account_id, player, state_timestamp).into();
        self.db
            .collection::<DbPlayer>("players")
            .replace_one_with_session(doc! { "_id": player.uid }, player, None, tsct)
            .map_err(FFError::from_db_error)?;
        Ok(())
    }
}
impl Database for MongoDatabase {
    fn find_account(&mut self, username: &Text) -> FFResult<Option<Account>> {
        let result = self
            .db
            .collection::<DbAccount>("accounts")
            .find_one(doc! { "username": username }, None)
            .map_err(FFError::from_db_error)?
            .map(|acc| acc.into());
        Ok(result)
    }

    fn create_account(&mut self, username: &Text, password_hashed: &Text) -> FFResult<Account> {
        let account_id = util::get_uid();
        let timestamp_now = util::get_timestamp_sec(SystemTime::now()) as Int;

        let accounts = self.db.collection::<DbAccount>("accounts");
        let account_level = if accounts
            .estimated_document_count(None)
            .is_ok_and(|count| count == 0)
        {
            CN_ACCOUNT_LEVEL__MASTER
        } else {
            config_get().login.default_account_level.get()
        } as Int;

        let account = DbAccount {
            account_id,
            username: username.clone(),
            password_hash: password_hashed.clone(),
            player_uids: Vec::new(),
            selected_slot: 1,
            account_level,
            creation_time: timestamp_now,
            last_login_time: timestamp_now,
            banned_until_time: 0,
            banned_since_time: 0,
            ban_reason: String::new(),
        };
        accounts
            .insert_one(account, None)
            .map_err(FFError::from_db_error)?;
        let new_acc = self.find_account(username)?.unwrap();
        Ok(new_acc)
    }

    fn init_player(&mut self, acc_id: BigInt, player: &Player) -> FFResult<()> {
        let state_timestamp = util::get_timestamp_sec(SystemTime::now()) as Int;
        let mut tsct = self
            .client
            .start_session(None)
            .map_err(FFError::from_db_error)?;
        tsct.start_transaction(None)
            .map_err(FFError::from_db_error)?;

        // first add the player document
        let player: DbPlayer = (acc_id, player, state_timestamp).into();
        let pc_uid = player.uid;
        self.db
            .collection::<DbPlayer>("players")
            .insert_one_with_session(player, None, &mut tsct)
            .map_err(FFError::from_db_error)?;

        // then update the account document
        self.db
            .collection::<DbAccount>("accounts")
            .update_one_with_session(
                doc! { "_id": acc_id },
                doc! { "$push": { "player_uids": pc_uid } },
                None,
                &mut tsct,
            )
            .map_err(FFError::from_db_error)?;

        tsct.commit_transaction().map_err(FFError::from_db_error)
    }

    fn update_player_appearance(&mut self, player: &Player) -> FFResult<()> {
        self.save_player(player, None)
    }

    fn update_selected_player(&mut self, acc_id: BigInt, slot_num: Int) -> FFResult<()> {
        let timestamp_now = util::get_timestamp_sec(SystemTime::now()) as Int;
        let result = self
            .db
            .collection::<DbAccount>("accounts")
            .update_one(
                doc! { "_id": acc_id },
                doc! { "$set": { "selected_slot": slot_num, "last_login_time": timestamp_now } },
                None,
            )
            .map_err(FFError::from_db_error)?;
        if result.matched_count == 0 {
            return Err(FFError::build(
                Severity::Warning,
                format!("Account with ID {} not found in database", acc_id),
            ));
        }
        Ok(())
    }

    fn save_player(&mut self, player: &Player, state_time: Option<SystemTime>) -> FFResult<()> {
        let state_time = state_time.unwrap_or(SystemTime::now());
        let state_timestamp = util::get_timestamp_sec(state_time) as Int;
        let mut tsct = self
            .client
            .start_session(None)
            .map_err(FFError::from_db_error)?;
        tsct.start_transaction(None)
            .map_err(FFError::from_db_error)?;
        self.save_player_internal(player, &mut tsct, state_timestamp)?;
        tsct.commit_transaction().map_err(FFError::from_db_error)
    }

    fn save_players(
        &mut self,
        players: &[&Player],
        state_time: Option<SystemTime>,
    ) -> FFResult<()> {
        let state_time = state_time.unwrap_or(SystemTime::now());
        let state_timestamp = util::get_timestamp_sec(state_time) as Int;
        let mut tsct = self
            .client
            .start_session(None)
            .map_err(FFError::from_db_error)?;
        tsct.start_transaction(None)
            .map_err(FFError::from_db_error)?;
        for player in players {
            self.save_player_internal(player, &mut tsct, state_timestamp)?;
        }
        tsct.commit_transaction().map_err(FFError::from_db_error)
    }

    fn load_player(&mut self, acc_id: BigInt, pc_uid: BigInt) -> FFResult<Player> {
        // get the account from the account collection
        let db_acc = self
            .db
            .collection::<DbAccount>("accounts")
            .find_one(doc! { "_id": acc_id }, None)
            .map_err(FFError::from_db_error)?
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Account with ID {} not found in database", acc_id),
            ))?;

        // get the player from the player collection
        let db_player = self
            .db
            .collection::<DbPlayer>("players")
            .find_one(doc! { "_id": pc_uid }, None)
            .map_err(FFError::from_db_error)?
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Player with UID {} not found in database", pc_uid),
            ))?;

        if db_player.account_id != acc_id {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "Player with UID {} does not belong to account with ID {}",
                    pc_uid, acc_id
                ),
            ));
        }

        (db_player, db_acc.account_level as i16).try_into()
    }

    fn load_players(&mut self, acc_id: BigInt) -> FFResult<Vec<Player>> {
        // get the player uids from the account
        let db_acc = self
            .db
            .collection::<DbAccount>("accounts")
            .find_one(doc! { "_id": acc_id }, None)
            .map_err(FFError::from_db_error)?
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Account with ID {} not found in database", acc_id),
            ))?;
        let player_uids = db_acc.player_uids;

        // get the players from the player collection
        let mut players = Vec::with_capacity(4);
        for pc_uid in player_uids.iter() {
            if *pc_uid == 0 {
                continue;
            }
            let player: DbPlayer = self
                .db
                .collection::<DbPlayer>("players")
                .find_one(doc! { "_id": pc_uid }, None)
                .map_err(FFError::from_db_error)?
                .ok_or(FFError::build(
                    Severity::Warning,
                    format!("Player with UID {} not found in database", pc_uid),
                ))?;
            let player: FFResult<Player> = (player, db_acc.account_level as i16).try_into();
            if let Err(e) = player {
                log_error(&e);
                continue;
            }
            players.push(player.unwrap());
        }
        Ok(players)
    }

    fn delete_player(&mut self, pc_uid: BigInt) -> FFResult<()> {
        // first find the account that owns the player
        let acc_id = self
            .db
            .collection::<DbPlayer>("players")
            .find_one(doc! { "_id": pc_uid }, None)
            .map_err(FFError::from_db_error)?
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Player with UID {} not found in database", pc_uid),
            ))?
            .account_id;

        let mut tsct = self
            .client
            .start_session(None)
            .map_err(FFError::from_db_error)?;
        tsct.start_transaction(None)
            .map_err(FFError::from_db_error)?;

        // then remove the player from the player collection
        self.db
            .collection::<DbPlayer>("players")
            .delete_one_with_session(doc! { "_id": pc_uid }, None, &mut tsct)
            .map_err(FFError::from_db_error)?;

        // then remove the player UID from the account
        self.db
            .collection::<DbAccount>("accounts")
            .update_one_with_session(
                doc! { "_id": acc_id },
                doc! { "$pull": { "player_uids": pc_uid } },
                None,
                &mut tsct,
            )
            .map_err(FFError::from_db_error)?;

        tsct.commit_transaction().map_err(FFError::from_db_error)
    }
}
