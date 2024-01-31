use std::{
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use postgres::{tls, types::ToSql, Client, Row};

use crate::{
    config::{config_get, GeneralConfig},
    defines::{DB_VERSION, PROTOCOL_VERSION, SIZEOF_QUESTFLAG_NUMBER, WYVERN_LOCATION_FLAG_SIZE},
    error::{log, panic_log, Severity},
    net::packet::{sItemBase, sNano},
    player::{Player, PlayerFlags, PlayerStyle},
    util, Combatant, Entity, Item, Nano, Position,
};

pub struct Database {
    client: Option<Client>,
    transaction: bool, // explicit transaction
}
impl Database {
    fn get(&mut self) -> &mut Client {
        self.client.as_mut().expect("Database not initialized")
    }

    fn read_sql(name: &str) -> String {
        let path = format!("sql/{}.sql", name);
        match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                panic_log(&format!("Couldn't read SQL file {}: {}", path, e));
            }
        }
    }

    pub fn query(&mut self, name: &str, params: &[&(dyn ToSql + Sync)]) -> Vec<Row> {
        let query = Self::read_sql(name);
        match self.get().query(&query, params) {
            Ok(r) => r,
            Err(e) => {
                panic_log(&format!("DB error: {}", e));
            }
        }
    }

    pub fn begin_transaction(&mut self) {
        if self.transaction {
            self.rollback_transaction();
            panic_log("Tried to begin transaction while already in one");
        }
        log(Severity::Debug, "Beginning transaction");
        self.transaction = true;
        self.get().execute("BEGIN", &[]).unwrap();
    }

    pub fn rollback_transaction(&mut self) {
        if !self.transaction {
            panic_log("Tried to rollback transaction while not in one");
        }
        self.transaction = false;
        self.get().execute("ROLLBACK", &[]).unwrap();
        log(Severity::Debug, "Rolled back transaction");
    }

    pub fn commit_transaction(&mut self) {
        if !self.transaction {
            panic_log("Tried to commit transaction while not in one");
        }
        self.transaction = false;
        self.get().execute("COMMIT", &[]).unwrap();
        log(Severity::Debug, "Committed transaction");
    }

    pub fn prep(&mut self, name: &str) -> postgres::Statement {
        let query = Self::read_sql(name);
        match self.get().prepare(&query) {
            Ok(r) => r,
            Err(e) => {
                panic_log(&format!("DB error: {}", e));
            }
        }
    }

    pub fn exec(&mut self, name: &str, mut params: &[&(dyn ToSql + Sync)]) -> u64 {
        let queries = Self::read_sql(name);
        let queries: Vec<&str> = queries.split(';').collect();
        let implicit_transaction = if self.transaction || queries.len() < 2 {
            false
        } else {
            self.begin_transaction();
            true
        };
        let mut num_updated = 0;
        for query in queries {
            if query.trim().is_empty() {
                continue;
            }
            //println!("{}", query);
            let num_params = query.char_indices().filter(|(_, c)| *c == '$').count();
            match self.get().execute(query, &params[..num_params]) {
                Ok(r) => {
                    num_updated += r;
                    params = &params[num_params..];
                }
                Err(e) => {
                    if self.transaction {
                        self.rollback_transaction();
                    }
                    panic_log(&format!("DB error: {}", e));
                }
            };
        }
        if implicit_transaction {
            self.commit_transaction();
        }
        num_updated
    }

    pub fn init_player(&mut self, acc_id: i64, player: &Player) {
        self.exec(
            "init_player",
            &[
                &player.get_uid(),
                &acc_id,
                &player.get_first_name(),
                &player.get_last_name(),
                &(player.get_style().iNameCheck as i32),
                &(player.get_slot_num() as i32),
                &player.get_position().x,
                &player.get_position().y,
                &player.get_position().z,
                &player.get_rotation(),
                &player.get_hp(),
                &[0_u8; (64 / 8) * WYVERN_LOCATION_FLAG_SIZE as usize].as_slice(),
                &[0_u8; 128 / 8].as_slice(),
                &[0_u8; (32 / 8) * SIZEOF_QUESTFLAG_NUMBER as usize].as_slice(),
                //
                &player.get_uid(),
            ],
        );
    }

    pub fn update_player_appearance(&mut self, player: &Player) {
        let style = player.style.unwrap_or_default();
        let apperance_flag: i32 = if player.style.is_some() { 1 } else { 0 };
        self.exec(
            "update_appearance",
            &[
                &player.get_uid(),
                &(style.body as i32),
                &(style.eye_color as i32),
                &(style.face_style as i32),
                &(style.gender as i32),
                &(style.hair_color as i32),
                &(style.hair_style as i32),
                &(style.height as i32),
                &(style.skin_color as i32),
                //
                &player.get_uid(),
                &apperance_flag,
            ],
        );
    }

    fn load_player_internal(&mut self, row: &Row) -> Player {
        let pc_uid = row.get("PlayerId");
        let slot_num: i32 = row.get("Slot");
        let mut player = Player::new(pc_uid, slot_num as usize);
        let appearance_flag: i32 = row.get("AppearanceFlag");
        player.style = if appearance_flag != 0 {
            Some(PlayerStyle {
                gender: row.get::<_, i32>("Gender") as i8,
                face_style: row.get::<_, i32>("FaceStyle") as i8,
                hair_style: row.get::<_, i32>("HairStyle") as i8,
                hair_color: row.get::<_, i32>("HairColor") as i8,
                skin_color: row.get::<_, i32>("SkinColor") as i8,
                eye_color: row.get::<_, i32>("EyeColor") as i8,
                height: row.get::<_, i32>("Height") as i8,
                body: row.get::<_, i32>("Body") as i8,
            })
        } else {
            None
        };

        let first_name = row.get("FirstName");
        let last_name = row.get("LastName");
        let name_check: i32 = row.get("NameCheck");
        player.set_name(
            name_check as i8,
            util::encode_utf16(first_name),
            util::encode_utf16(last_name),
        );

        player.set_position(Position {
            x: row.get("XCoordinate"),
            y: row.get("YCoordinate"),
            z: row.get("ZCoordinate"),
        });
        player.set_rotation(row.get("Angle"));

        player.set_taros(row.get::<_, i32>("Taros") as u32);
        player.set_fusion_matter(row.get::<_, i32>("FusionMatter") as u32);
        player.set_level(row.get::<_, i32>("Level") as i16);
        player.set_hp(row.get("HP"));
        player.set_weapon_boosts(row.get::<_, i32>("BatteryW") as u32);
        player.set_nano_potions(row.get::<_, i32>("BatteryN") as u32);

        let nano_col_names = ["Nano1", "Nano2", "Nano3"];
        for (slot, col_name) in nano_col_names.iter().enumerate() {
            let nano_id = row.get::<_, i32>(col_name) as i16;
            player
                .change_nano(slot, if nano_id == 0 { None } else { Some(nano_id) })
                .unwrap();
        }
        let nanos = self.query("load_nanos", &[&pc_uid]);
        for nano in nanos {
            let nano_raw = sNano {
                iID: nano.get::<_, i32>("ID") as i16,
                iSkillID: nano.get::<_, i32>("Skill") as i16,
                iStamina: nano.get::<_, i32>("Stamina") as i16,
            };
            let nano: Option<Nano> = nano_raw.try_into().unwrap();
            player.set_nano(nano.unwrap());
        }

        let mut player_flags = PlayerFlags::default();
        let first_use_bytes: &[u8] = row.get("FirstUseFlag");
        player_flags.tip_flags = i128::from_le_bytes(first_use_bytes[..16].try_into().unwrap());
        player_flags.tutorial_flag = row.get::<_, i32>("TutorialFlag") != 0;
        player.flags = player_flags;

        let skyway_bytes: &[u8] = row.get("SkywayLocationFlag");
        player.set_skyway_flags([
            i64::from_le_bytes(skyway_bytes[..8].try_into().unwrap()),
            i64::from_le_bytes(skyway_bytes[8..16].try_into().unwrap()),
        ]);
        player.set_scamper_flag(row.get("WarpLocationFlag"));

        let items = self.query("load_items", &[&pc_uid]);
        for item in items {
            let slot_num = item.get::<_, i32>("Slot") as usize;
            let item_raw = sItemBase {
                iType: item.get::<_, i32>("Type") as i16,
                iID: item.get::<_, i32>("ID") as i16,
                iOpt: item.get::<_, i32>("Opt"),
                iTimeLimit: item.get::<_, i32>("TimeLimit"),
            };
            let item: Option<Item> = item_raw.try_into().unwrap();
            let (loc, slot_num) = util::slot_num_to_loc_and_slot_num(slot_num).unwrap();
            player.set_item(loc, slot_num, item).unwrap();
        }

        player
    }

    pub fn load_player(&mut self, acc_id: i64, pc_uid: i64) -> Option<Player> {
        let rows = self.query("load_players", &[&acc_id]);
        for row in &rows {
            if row.get::<_, i64>("PlayerId") == pc_uid {
                let player = self.load_player_internal(row);
                return Some(player);
            }
        }
        None
    }

    pub fn load_players(&mut self, acc_id: i64) -> Vec<Player> {
        let chars = self.query("load_players", &[&acc_id]);
        chars
            .iter()
            .map(|row| self.load_player_internal(row))
            .collect()
    }

    pub fn save_player(&mut self, player: &Player, transacted: bool) {
        let save_item = self.prep("save_item");
        let save_nano = self.prep("save_nano");
        let pc_uid = player.get_uid();

        if !transacted {
            self.begin_transaction();
        }

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

        self.exec(
            "save_player",
            &[
                &pc_uid,
                &(player.get_level() as i32),
                &(player.get_equipped_nano_ids()[0] as i32),
                &(player.get_equipped_nano_ids()[1] as i32),
                &(player.get_equipped_nano_ids()[2] as i32),
                &(player.flags.tutorial_flag as i32),
                &(player.flags.payzone_flag as i32),
                &position.x,
                &position.y,
                &position.z,
                &player.get_rotation(),
                &player.get_hp(),
                &(player.get_fusion_matter() as i32),
                &(player.get_taros() as i32),
                &(player.get_weapon_boosts() as i32),
                &(player.get_nano_potions() as i32),
                &((player.get_guide() as i16) as i32),
                &player.get_active_mission_id(),
                &player.get_scamper_flags(),
                &skyway_bytes,
                &player.flags.tip_flags.to_le_bytes().as_slice(),
                &quest_bytes,
            ],
        );

        self.exec("clear_nanos", &[&pc_uid]);
        for nano in player.get_nano_iter() {
            let nano_raw: sNano = Some(nano.clone()).into();
            if let Err(e) = self.get().execute(
                &save_nano,
                &[
                    &pc_uid,
                    &(nano_raw.iID as i32),
                    &(nano_raw.iSkillID as i32),
                    &(nano_raw.iStamina as i32),
                ],
            ) {
                panic_log(&format!("DB error: {}", e));
            }
        }

        self.exec("clear_items", &[&pc_uid]);
        for (slot_num, item) in player.get_item_iter() {
            let item_raw: sItemBase = Some(*item).into();
            if let Err(e) = self.get().execute(
                &save_item,
                &[
                    &pc_uid,
                    &(slot_num as i32),
                    &(item_raw.iID as i32),
                    &(item_raw.iType as i32),
                    &item_raw.iOpt,
                    &item_raw.iTimeLimit,
                ],
            ) {
                panic_log(&format!("DB error: {}", e));
            }
        }

        if !transacted {
            self.commit_transaction();
        }
    }
}

static DATABASE: Mutex<Database> = Mutex::new(Database {
    client: None,
    transaction: false,
});

fn db_connect(config: &GeneralConfig) -> Database {
    const DB_NAME: &str = "rustyfusion";

    let mut db_config = Client::configure();
    db_config
        .host(&config.db_host.get())
        .port(config.db_port.get())
        .user(&config.db_username.get())
        .password(config.db_password.get())
        .dbname(DB_NAME)
        .connect_timeout(Duration::from_secs(5));
    let db_client = db_config.connect(tls::NoTls);
    if let Err(e) = db_client {
        panic_log(&format!("Couldn't connect to database: {}", e));
    }
    let mut db = Database {
        client: Some(db_client.unwrap()),
        transaction: false,
    };

    let meta_table_exists: bool = db.query("meta_table_exists", &[])[0].get(0);
    if !meta_table_exists {
        log(
            Severity::Info,
            "Meta table missing; initializing database...",
        );
        db.exec("create_tables", &[&PROTOCOL_VERSION, &DB_VERSION]);
    }

    db
}

pub fn db_init() -> MutexGuard<'static, Database> {
    let mut db = DATABASE.lock().unwrap();
    let config = &config_get().general;
    *db = db_connect(config);
    log(
        Severity::Info,
        &format!(
            "Connected to database ({}@{}:{})",
            config.db_username.get(),
            config.db_host.get(),
            config.db_port.get()
        ),
    );
    db
}

pub fn db_get() -> MutexGuard<'static, Database> {
    DATABASE.lock().unwrap()
}
