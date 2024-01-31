use std::time::Duration;

use postgres::{tls, types::ToSql, GenericClient, Row};

use crate::{
    defines::{DB_VERSION, PROTOCOL_VERSION, SIZEOF_QUESTFLAG_NUMBER, WYVERN_LOCATION_FLAG_SIZE},
    net::packet::{sItemBase, sNano},
    player::{PlayerFlags, PlayerStyle},
    util, Combatant, Entity, Item, Nano, Position,
};

use super::*;

impl FFError {
    fn from_db_err(e: postgres::Error) -> Self {
        FFError::build(Severity::Warning, format!("Database error: {}", e))
    }
}

pub struct PostgresDatabase {
    client: postgres::Client,
    config: postgres::Config,
}
impl std::fmt::Debug for PostgresDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Postgres Database ({:?})", self.config)
    }
}
impl PostgresDatabase {
    pub fn connect(config: &GeneralConfig) -> FFResult<Box<dyn Database>> {
        let mut db_config = postgres::Client::configure();
        db_config
            .host(&config.db_host.get())
            .port(config.db_port.get())
            .user(&config.db_username.get())
            .password(config.db_password.get())
            .dbname(DB_NAME)
            .connect_timeout(Duration::from_secs(5));
        let mut db_client = db_config
            .connect(tls::NoTls)
            .map_err(FFError::from_db_err)?;

        let meta_table_exists: bool =
            Self::query(&mut db_client, "meta_table_exists", &[])?[0].get(0);
        if !meta_table_exists {
            log(
                Severity::Info,
                "Meta table missing; initializing database...",
            );
            Self::exec(
                &mut db_client,
                "create_tables",
                &[&PROTOCOL_VERSION, &DB_VERSION],
            )?;
        }

        Ok(Box::new(Self {
            client: db_client,
            config: db_config,
        }))
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

    fn query(
        client: &mut impl GenericClient,
        name: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> FFResult<Vec<Row>> {
        let query = Self::read_sql(name);
        client.query(&query, params).map_err(FFError::from_db_err)
    }

    fn prep(client: &mut impl GenericClient, name: &str) -> FFResult<postgres::Statement> {
        let query = Self::read_sql(name);
        client.prepare(&query).map_err(FFError::from_db_err)
    }

    fn exec(
        client: &mut impl GenericClient,
        name: &str,
        mut params: &[&(dyn ToSql + Sync)],
    ) -> FFResult<u64> {
        let queries = Self::read_sql(name);
        let queries: Vec<&str> = queries.split(';').collect();

        // implicit transaction
        let mut tsct = client.transaction().map_err(FFError::from_db_err)?;

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
                    panic_log(&format!("DB error: {}", e));
                }
            };
        }

        tsct.commit().map_err(FFError::from_db_err)?;
        Ok(num_updated)
    }

    fn save_player_internal(client: &mut impl GenericClient, player: &Player) -> FFResult<()> {
        let mut tsct = client.transaction().map_err(FFError::from_db_err)?;
        let client = &mut tsct;
        let save_item = Self::prep(client, "save_item")?;
        let save_nano = Self::prep(client, "save_nano")?;
        let pc_uid = player.get_uid();

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

        Self::exec(
            client,
            "save_player",
            &[
                &pc_uid,
                &(player.get_level() as Int),
                &(player.get_equipped_nano_ids()[0] as Int),
                &(player.get_equipped_nano_ids()[1] as Int),
                &(player.get_equipped_nano_ids()[2] as Int),
                &(player.flags.tutorial_flag as Int),
                &(player.flags.payzone_flag as Int),
                &position.x,
                &position.y,
                &position.z,
                &player.get_rotation(),
                &player.get_hp(),
                &(player.get_fusion_matter() as Int),
                &(player.get_taros() as Int),
                &(player.get_weapon_boosts() as Int),
                &(player.get_nano_potions() as Int),
                &((player.get_guide() as i16) as Int),
                &player.get_active_mission_id(),
                &player.get_scamper_flags(),
                &skyway_bytes,
                &player.flags.tip_flags.to_le_bytes().as_slice(),
                &quest_bytes,
            ],
        )?;

        Self::exec(client, "clear_nanos", &[&pc_uid])?;
        for nano in player.get_nano_iter() {
            let nano_raw: sNano = Some(nano.clone()).into();
            client
                .execute(
                    &save_nano,
                    &[
                        &pc_uid,
                        &(nano_raw.iID as Int),
                        &(nano_raw.iSkillID as Int),
                        &(nano_raw.iStamina as Int),
                    ],
                )
                .map_err(FFError::from_db_err)?;
        }

        Self::exec(client, "clear_items", &[&pc_uid])?;
        for (slot_num, item) in player.get_item_iter() {
            let item_raw: sItemBase = Some(*item).into();
            client
                .execute(
                    &save_item,
                    &[
                        &pc_uid,
                        &(slot_num as Int),
                        &(item_raw.iID as Int),
                        &(item_raw.iType as Int),
                        &item_raw.iOpt,
                        &item_raw.iTimeLimit,
                    ],
                )
                .map_err(FFError::from_db_err)?;
        }

        tsct.commit().map_err(FFError::from_db_err)?;
        Ok(())
    }

    fn load_player_internal(client: &mut impl GenericClient, row: &Row) -> FFResult<Player> {
        let pc_uid = row.get("PlayerId");
        let slot_num: Int = row.get("Slot");
        let mut player = Player::new(pc_uid, slot_num as usize);
        let appearance_flag: Int = row.get("AppearanceFlag");
        player.style = if appearance_flag != 0 {
            Some(PlayerStyle {
                gender: row.get::<_, Int>("Gender") as i8,
                face_style: row.get::<_, Int>("FaceStyle") as i8,
                hair_style: row.get::<_, Int>("HairStyle") as i8,
                hair_color: row.get::<_, Int>("HairColor") as i8,
                skin_color: row.get::<_, Int>("SkinColor") as i8,
                eye_color: row.get::<_, Int>("EyeColor") as i8,
                height: row.get::<_, Int>("Height") as i8,
                body: row.get::<_, Int>("Body") as i8,
            })
        } else {
            None
        };

        let first_name = row.get("FirstName");
        let last_name = row.get("LastName");
        let name_check: Int = row.get("NameCheck");
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

        player.set_taros(row.get::<_, Int>("Taros") as u32);
        player.set_fusion_matter(row.get::<_, Int>("FusionMatter") as u32);
        player.set_level(row.get::<_, Int>("Level") as i16);
        player.set_hp(row.get("HP"));
        player.set_weapon_boosts(row.get::<_, Int>("BatteryW") as u32);
        player.set_nano_potions(row.get::<_, Int>("BatteryN") as u32);

        let nano_col_names = ["Nano1", "Nano2", "Nano3"];
        for (slot, col_name) in nano_col_names.iter().enumerate() {
            let nano_id = row.get::<_, Int>(col_name) as i16;
            player
                .change_nano(slot, if nano_id == 0 { None } else { Some(nano_id) })
                .unwrap();
        }
        let nanos = Self::query(client, "load_nanos", &[&pc_uid])?;
        for nano in nanos {
            let nano_raw = sNano {
                iID: nano.get::<_, Int>("ID") as i16,
                iSkillID: nano.get::<_, Int>("Skill") as i16,
                iStamina: nano.get::<_, Int>("Stamina") as i16,
            };
            let nano: Option<Nano> = nano_raw.try_into().unwrap();
            player.set_nano(nano.unwrap());
        }

        let mut player_flags = PlayerFlags::default();
        let first_use_bytes: &[u8] = row.get("FirstUseFlag");
        player_flags.tip_flags = i128::from_le_bytes(first_use_bytes[..16].try_into().unwrap());
        player_flags.tutorial_flag = row.get::<_, Int>("TutorialFlag") != 0;
        player.flags = player_flags;

        let skyway_bytes: &[u8] = row.get("SkywayLocationFlag");
        player.set_skyway_flags([
            BigInt::from_le_bytes(skyway_bytes[..8].try_into().unwrap()),
            BigInt::from_le_bytes(skyway_bytes[8..16].try_into().unwrap()),
        ]);
        player.set_scamper_flag(row.get("WarpLocationFlag"));

        let items = Self::query(client, "load_items", &[&pc_uid])?;
        for item in items {
            let slot_num = item.get::<_, Int>("Slot") as usize;
            let item_raw = sItemBase {
                iType: item.get::<_, Int>("Type") as i16,
                iID: item.get::<_, Int>("ID") as i16,
                iOpt: item.get::<_, Int>("Opt"),
                iTimeLimit: item.get::<_, Int>("TimeLimit"),
            };
            let item: Option<Item> = item_raw.try_into().unwrap();
            let (loc, slot_num) = util::slot_num_to_loc_and_slot_num(slot_num).unwrap();
            player.set_item(loc, slot_num, item).unwrap();
        }

        Ok(player)
    }
}
impl Database for PostgresDatabase {
    fn init_player(&mut self, acc_id: BigInt, player: &Player) -> FFResult<()> {
        let client = &mut self.client;
        let updated = Self::exec(
            client,
            "init_player",
            &[
                &player.get_uid(),
                &acc_id,
                &player.get_first_name(),
                &player.get_last_name(),
                &(player.get_style().iNameCheck as Int),
                &(player.get_slot_num() as Int),
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
        )?;
        assert_eq!(updated, 1 + 1);
        Ok(())
    }

    fn update_player_appearance(&mut self, player: &Player) -> FFResult<()> {
        let client = &mut self.client;
        let style = player.style.unwrap_or_default();
        let apperance_flag: Int = if player.style.is_some() { 1 } else { 0 };
        let updated = Self::exec(
            client,
            "update_appearance",
            &[
                &player.get_uid(),
                &(style.body as Int),
                &(style.eye_color as Int),
                &(style.face_style as Int),
                &(style.gender as Int),
                &(style.hair_color as Int),
                &(style.hair_style as Int),
                &(style.height as Int),
                &(style.skin_color as Int),
                //
                &player.get_uid(),
                &apperance_flag,
            ],
        )?;
        assert_eq!(updated, 1 + 1);
        Ok(())
    }

    fn find_account(&mut self, username: &Text) -> FFResult<Option<BigInt>> {
        let client = &mut self.client;
        let rows = Self::query(client, "find_account", &[username])?;
        if rows.is_empty() {
            Ok(None)
        } else {
            Ok(Some(rows[0].get("AccountId")))
        }
    }

    fn create_account(&mut self, username: &Text, password_hashed: &Text) -> FFResult<BigInt> {
        let client = &mut self.client;
        let updated = Self::exec(client, "create_account", &[username, password_hashed])?;
        assert_eq!(updated, 1);
        let acc_id = Self::query(client, "find_account", &[username])?[0].get("AccountId");
        Ok(acc_id)
    }

    fn update_selected_player(&mut self, acc_id: BigInt, slot_num: Int) -> FFResult<()> {
        let client = &mut self.client;
        let updated = Self::exec(client, "update_selected", &[&acc_id, &slot_num])?;
        assert_eq!(updated, 1);
        Ok(())
    }

    fn load_player(&mut self, acc_id: BigInt, pc_uid: BigInt) -> FFResult<Player> {
        let client = &mut self.client;
        let rows = Self::query(client, "load_players", &[&acc_id])?;
        for row in &rows {
            if row.get::<_, BigInt>("PlayerId") == pc_uid {
                return Self::load_player_internal(client, row);
            }
        }
        Err(FFError::build(
            Severity::Warning,
            format!(
                "Player with UID {} not found for account with ID {}",
                pc_uid, acc_id
            ),
        ))
    }

    fn load_players(&mut self, acc_id: BigInt) -> FFResult<Vec<Player>> {
        let client = &mut self.client;
        let chars = Self::query(client, "load_players", &[&acc_id])?;
        let mut players = Vec::with_capacity(chars.len());
        for row in chars {
            players.push(Self::load_player_internal(client, &row)?);
        }
        Ok(players)
    }

    fn save_player(&mut self, player: &Player) -> FFResult<()> {
        Self::save_player_internal(&mut self.client, player)
    }

    fn save_players(&mut self, players: &[&Player]) -> FFResult<()> {
        let mut tsct = self.client.transaction().map_err(FFError::from_db_err)?;
        for player in players {
            Self::save_player_internal(&mut tsct, player)?;
        }
        tsct.commit().map_err(FFError::from_db_err)?;
        Ok(())
    }

    fn delete_player(&mut self, pc_uid: BigInt) -> FFResult<()> {
        let client = &mut self.client;
        let updated: u64 = Self::exec(client, "delete_player", &[&pc_uid])?;
        assert_eq!(updated, 1);
        Ok(())
    }
}
