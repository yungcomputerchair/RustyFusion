use std::collections::HashMap;

use mlua::{prelude::*, Variadic};

use crate::{
    entity::EntityID,
    error::{log, FFError, FFResult, Severity},
    state::ShardServerState,
    util,
};

const TICK_CALLBACK_TABLE: &str = "onTick";

pub struct ScriptManager {
    lua: Lua,
    global_env: LuaRegistryKey,
    entity_envs: HashMap<EntityID, LuaRegistryKey>,
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

    pub fn new() -> FFResult<Self> {
        Self::new_internal()
            .map(|(lua, global_env_key)| Self {
                lua,
                global_env: global_env_key,
                entity_envs: HashMap::new(),
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
            .map_err(FFError::from_lua_err)
    }

    pub fn load_script_raw(&mut self, entity_id: Option<EntityID>, source: &str) -> FFResult<()> {
        self.load_script_internal(entity_id, source, None)
            .map_err(FFError::from_lua_err)
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
}
