use std::{
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use postgres::{tls, types::ToSql, Client, Row};

use crate::{
    config::config_get,
    defines::{DB_VERSION, PROTOCOL_VERSION},
    error::{log, FFResult, Severity},
    net::packet::sPCStyle,
    player::Player,
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

    pub fn load_player(&mut self, row: &Row) -> FFResult<Player> {
        let pc_uid = row.get("PlayerId");
        let mut player = Player::new(pc_uid);
        player.set_position(Position {
            x: row.get("XCoordinate"),
            y: row.get("YCoodinate"),
            z: row.get("ZCoordinate"),
        });
        let first_name = row.get("FirstName");
        let last_name = row.get("LastName");
        let name_check = row.get("NameCheck");
        player.set_name(
            name_check,
            util::encode_utf16(first_name),
            util::encode_utf16(last_name),
        );
        player.set_level(row.get("Level"));
        let style = sPCStyle {
            iPC_UID: todo!(),
            iNameCheck: todo!(),
            szFirstName: todo!(),
            szLastName: todo!(),
            iGender: todo!(),
            iFaceStyle: todo!(),
            iHairStyle: todo!(),
            iHairColor: todo!(),
            iSkinColor: todo!(),
            iEyeColor: todo!(),
            iHeight: todo!(),
            iBody: todo!(),
            iClass: todo!(),
        };

        Ok(player)
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
