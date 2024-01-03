use std::{
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use postgres::{tls, types::ToSql, Client, Row};

use crate::{
    config::config_get,
    error::{log, Severity},
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

    pub fn run(&mut self, name: &str, mut params: &[&(dyn ToSql + Sync)]) -> u64 {
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
            println!("{}", query);
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
        db.run("create_meta_table", &[&104_i32, &1_i32]);
    }

    db
}

pub fn db_get() -> MutexGuard<'static, Database> {
    DATABASE.lock().unwrap()
}
