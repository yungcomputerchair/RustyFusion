use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard, OnceLock},
};

use mlua::{prelude::*, Variadic};

use crate::{
    entity::EntityID,
    error::{log, log_if_failed, panic_log, FFError, FFResult, Severity},
    state::ShardServerState,
    util,
};

const TICK_CALLBACK_TABLE: &str = "onTick";

static SCRIPT_MANAGER: OnceLock<Mutex<ScriptManager>> = OnceLock::new();

enum LuaScript {
    File(String),
    Raw(String),
}

pub fn scripting_init() {
    match SCRIPT_MANAGER.get() {
        Some(_) => panic_log("Scripting already initialized"),
        None => {
            log(Severity::Info, "Initializing scripting engine...");
            match ScriptManager::new() {
                Ok(sm) => {
                    let _ = SCRIPT_MANAGER.set(Mutex::new(sm));
                }
                Err(e) => panic_log(&format!(
                    "Failed to initialize scripting engine: {}",
                    e.get_msg()
                )),
            }
        }
    }
}

pub fn scripting_get() -> MutexGuard<'static, ScriptManager> {
    match SCRIPT_MANAGER.get() {
        None => panic_log("Scripting accessed before init"),
        Some(mutex) => mutex.lock().unwrap(),
    }
}

pub struct ScriptManager {
    lua: Lua,
    global_env: LuaRegistryKey,
    entity_envs: HashMap<EntityID, LuaRegistryKey>,
    assigned_scripts: Vec<(Option<EntityID>, LuaScript)>,
}
impl ScriptManager {
    fn new_internal() -> LuaResult<(Lua, LuaRegistryKey)> {
        let lua = Lua::new();

        // Redirect Lua print() to our log() function
        let log_lua = lua.create_function(|_, args: Variadic<String>| {
            let mut msg = String::new();
            msg.push_str("LUA: ");
            for (pos, arg) in args.iter().enumerate() {
                if pos > 0 {
                    msg.push(' ');
                }
                msg.push_str(arg);
            }
            log(Severity::Info, &msg);
            Ok(())
        })?;
        lua.globals().set("print", log_lua)?;

        // Shared state for all scripts
        let blackboard = lua.create_table()?;
        lua.globals().set("bb", blackboard)?;

        // Environment for non-entity scripts
        let global_env = Self::make_env(&lua)?;
        let global_env_key = lua.create_registry_value(global_env)?;

        lua.load("print('Scripting engine initialized')").exec()?;

        Ok((lua, global_env_key))
    }

    fn new() -> FFResult<Self> {
        Self::new_internal()
            .map(|(lua, global_env_key)| Self {
                lua,
                global_env: global_env_key,
                entity_envs: HashMap::new(),
                assigned_scripts: Vec::new(),
            })
            .map_err(FFError::from_lua_err)
    }

    fn load_script_internal(
        &mut self,
        entity_id: Option<EntityID>,
        source: &str,
        name: Option<&str>,
    ) -> LuaResult<()> {
        let script_scope = match entity_id {
            Some(eid) => eid.to_string(),
            None => "Global".to_string(),
        };
        let namespace = match name {
            Some(name) => format!("{}:{}", script_scope, name),
            None => format!("{}:<raw>", script_scope),
        };
        let chunk = self.lua.load(source).set_name(namespace);

        // populate the environment
        let env_key = match entity_id {
            Some(eid) => {
                // populate the entity environment if it doesn't exist
                if !self.is_entity_registered(eid) {
                    let entity_env_key = Self::make_env(&self.lua)?;
                    let entity_env = self
                        .lua
                        .registry_value::<LuaTable>(&entity_env_key)
                        .unwrap();
                    entity_env.set("entity_id", eid.to_string())?;
                    self.entity_envs.insert(eid, entity_env_key);
                }
                self.entity_envs.get(&eid).unwrap()
            }
            None => &self.global_env,
        };

        let env = self.lua.registry_value::<LuaTable>(env_key)?;
        chunk.set_environment(env).exec()?;
        Ok(())
    }

    fn is_entity_registered(&self, entity_id: EntityID) -> bool {
        self.entity_envs.contains_key(&entity_id)
    }

    fn make_env(lua: &Lua) -> LuaResult<LuaRegistryKey> {
        let env = lua.create_table()?;

        // link allowed Lua globals
        let aliases = ["bb", "print", "table"];
        for &alias in aliases.iter() {
            env.set(alias, lua.globals().get::<_, LuaValue>(alias)?)?;
        }

        // link custom globals
        let tick_callbacks = lua.create_table()?;
        env.set(TICK_CALLBACK_TABLE, tick_callbacks)?;

        let env_key = lua.create_registry_value(env)?;
        Ok(env_key)
    }

    pub fn load_script(&mut self, entity_id: Option<EntityID>, script_name: &str) -> FFResult<()> {
        let path = format!("scripts/{}.lua", script_name);
        let source = util::get_text_file_contents(&path)?;
        self.load_script_internal(entity_id, &source, Some(script_name))
            .map_err(FFError::from_lua_err)?;
        self.assigned_scripts
            .push((entity_id, LuaScript::File(script_name.to_string())));
        Ok(())
    }

    pub fn load_script_raw(&mut self, entity_id: Option<EntityID>, source: &str) -> FFResult<()> {
        self.load_script_internal(entity_id, source, None)
            .map_err(FFError::from_lua_err)?;
        self.assigned_scripts
            .push((entity_id, LuaScript::Raw(source.to_string())));
        Ok(())
    }

    fn tick_scripts(
        &self,
        entity_id: Option<EntityID>,
        _state: &mut ShardServerState,
    ) -> LuaResult<()> {
        let script_env_key = match entity_id {
            Some(eid) => self.entity_envs.get(&eid).unwrap(),
            None => &self.global_env,
        };
        let script_env = self.lua.registry_value::<LuaTable>(script_env_key)?;
        let tick_callbacks = script_env.get::<_, LuaTable>(TICK_CALLBACK_TABLE).unwrap();
        for tick_fn in tick_callbacks.sequence_values::<LuaFunction>() {
            tick_fn?.call(())?;
        }
        Ok(())
    }

    pub fn tick(&mut self, state: &mut ShardServerState) -> FFResult<()> {
        // tick global scripts first
        if let Err(e) = self.tick_scripts(None, state) {
            return Err(FFError::build(
                Severity::Warning,
                format!("Error ticking global scripts: {}", e),
            ));
        }

        // tick entity scripts
        let to_tick = self.entity_envs.keys().cloned().collect::<Vec<_>>();
        for eid in to_tick {
            if let Err(e) = self.tick_scripts(Some(eid), state) {
                log(
                    Severity::Warning,
                    &format!("Error ticking script for entity {:?}: {}", eid, e),
                );
                // if something goes wrong, we unregister the entity completely
                self.entity_envs.remove(&eid);
            }
        }
        Ok(())
    }

    pub fn reload(&mut self) -> FFResult<()> {
        let loaded_scripts: Vec<(Option<EntityID>, LuaScript)> =
            self.assigned_scripts.drain(..).collect();
        self.reset()?;
        for (eid, script) in loaded_scripts {
            log_if_failed(match script {
                LuaScript::File(script_name) => self.load_script(eid, &script_name),
                LuaScript::Raw(source) => self.load_script_raw(eid, &source),
            });
        }
        Ok(())
    }

    fn reset(&mut self) -> FFResult<()> {
        self.entity_envs.clear();
        self.lua.expire_registry_values(); // just in case
        let (new_lua, new_global_env) = Self::new_internal().map_err(FFError::from_lua_err)?;
        self.lua = new_lua;
        self.global_env = new_global_env;
        Ok(())
    }
}
