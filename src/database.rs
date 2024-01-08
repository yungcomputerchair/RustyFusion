use std::{
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use postgres::{tls, types::ToSql, Client, Row};

use crate::{
    config::config_get,
    defines::{DB_VERSION, PROTOCOL_VERSION},
    error::{log, Severity},
    player::{Player, PlayerFlags, PlayerStyle},
    util, Entity, Position,
};

pub struct Database {
    client: Option<Client>,
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
                log(
                    Severity::Fatal,
                    &format!("Couldn't read SQL file {}: {}", path, e),
                );
                panic!();
            }
        }
    }

    pub fn query(&mut self, name: &str, params: &[&(dyn ToSql + Sync)]) -> Vec<Row> {
        let query = Self::read_sql(name);
        match self.get().query(&query, params) {
            Ok(r) => r,
            Err(e) => {
                log(Severity::Fatal, &format!("DB error: {}", e));
                panic!();
            }
        }
    }

    pub fn exec(&mut self, name: &str, mut params: &[&(dyn ToSql + Sync)]) -> u64 {
        let queries = Self::read_sql(name);
        let queries = queries.split(';');
        let mut tsct = match self.get().transaction() {
            Ok(t) => t,
            Err(e) => {
                log(Severity::Fatal, &format!("DB error: {}", e));
                panic!();
            }
        };
        let mut num_updated = 0;
        for query in queries {
            if query.trim().is_empty() {
                continue;
            }
            //println!("{}", query);
            let num_params = query.char_indices().filter(|(_, c)| *c == '$').count();
            match tsct.execute(query, &params[..num_params]) {
                Ok(r) => {
                    num_updated += r;
                    params = &params[num_params..];
                }
                Err(e) => {
                    log(Severity::Fatal, &format!("DB error: {}", e));
                    panic!();
                }
            };
        }
        match tsct.commit() {
            Ok(_) => num_updated,
            Err(e) => {
                log(Severity::Fatal, &format!("DB error: {}", e));
                panic!();
            }
        }
    }

    pub fn load_player(&mut self, row: &Row) -> Player {
        let pc_uid = row.get("PlayerId");
        let mut player = Player::new(pc_uid);
        player.style = PlayerStyle {
            gender: row.get("Gender"),
            face_style: row.get("FaceStyle"),
            hair_style: row.get("HairStyle"),
            hair_color: row.get("HairColor"),
            skin_color: row.get("SkinColor"),
            eye_color: row.get("EyeColor"),
            height: row.get("Height"),
            body: row.get("Body"),
        };

        let first_name = row.get("FirstName");
        let last_name = row.get("LastName");
        let name_check = row.get("NameCheck");
        player.set_name(
            name_check,
            util::encode_utf16(first_name),
            util::encode_utf16(last_name),
        );

        player.set_position(Position {
            x: row.get("XCoordinate"),
            y: row.get("YCoodinate"),
            z: row.get("ZCoordinate"),
        });
        player.set_rotation(row.get("Angle"));

        player.set_taros(row.get("Taros"));
        player.set_fusion_matter(row.get("FusionMatter"));
        player.set_level(row.get("Level"));
        player.set_hp(row.get("HP"));
        player.set_weapon_boosts(row.get("BatteryW"));
        player.set_nano_potions(row.get("BatteryN"));

        let nano_col_names = ["Nano1", "Nano2", "Nano3"];
        for (slot, col_name) in nano_col_names.iter().enumerate() {
            let nano_id = row.get(col_name);
            player
                .change_nano(slot, if nano_id == 0 { None } else { Some(nano_id) })
                .unwrap();
        }

        let mut player_flags = PlayerFlags::default();
        let first_use_bytes: &[u8] = row.get("FirstUseFlag");
        player_flags.tip_flags = i128::from_le_bytes(first_use_bytes[..16].try_into().unwrap());
        player_flags.appearance_flag = row.get("AppearanceFlag");
        player_flags.tutorial_flag = row.get("TutorialFlag");
        player.flags = player_flags;

        let skyway_bytes: &[u8] = row.get("SkywayLocationFlag");
        player.set_skyway_flags([
            i64::from_le_bytes(skyway_bytes[..8].try_into().unwrap()),
            i64::from_le_bytes(skyway_bytes[8..16].try_into().unwrap()),
        ]);
        player.set_scamper_flag(row.get("WarpLocationFlag"));

        player
    }
}

static DATABASE: Mutex<Database> = Mutex::new(Database { client: None });

pub fn db_init() -> MutexGuard<'static, Database> {
    const DB_NAME: &str = "rustyfusion";

    let config = &config_get().general;
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
        log(
            Severity::Fatal,
            &format!("Couldn't connect to database: {}", e),
        );
        panic!();
    }
    let mut db = DATABASE.lock().unwrap();
    *db = Database {
        client: Some(db_client.unwrap()),
    };
    log(
        Severity::Info,
        &format!(
            "Connected to database ({}@{}:{})",
            config.db_username.get(),
            config.db_host.get(),
            config.db_port.get()
        ),
    );

    let meta_table_exists: &bool = &db.query("meta_table_exists", &[])[0].get(0);
    if !meta_table_exists {
        log(
            Severity::Info,
            "Meta table missing; initializing database...",
        );
        db.exec("create_tables", &[&PROTOCOL_VERSION, &DB_VERSION]);
    }

    db
}

pub fn db_get() -> MutexGuard<'static, Database> {
    DATABASE.lock().unwrap()
}
