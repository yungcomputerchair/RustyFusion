use std::collections::HashMap;

use mlua::{prelude::*, Variadic};

use crate::{
    entity::EntityID,
    error::{log, FFError, FFResult, Severity},
    state::ShardServerState,
    util,
};

pub struct ScriptManager {
    lua: Lua,
    registered_entities: HashMap<EntityID, LuaRegistryKey>,
}
impl ScriptManager {
    fn new_internal() -> LuaResult<Lua> {
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

        lua.load("print('Scripting engine initialized')").exec()?;

        Ok(lua)
    }

    pub fn new() -> FFResult<Self> {
        Self::new_internal()
            .map(|lua| Self {
                lua,
                registered_entities: HashMap::new(),
            })
            .map_err(FFError::from_lua_err)
    }

    fn load_script_for_entity_internal(
        &mut self,
        entity_id: EntityID,
        source: &str,
        name: Option<&str>,
    ) -> LuaResult<()> {
        let namespace = match name {
            Some(name) => format!("{}:{}", entity_id, name),
            None => format!("{}:<raw>", entity_id),
        };
        let chunk = self.lua.load(source).set_name(namespace);

        // populate the entity environment if it doesn't exist
        if !self.is_entity_registered(entity_id) {
            let entity_env_key = self.make_entity_env(entity_id)?;
            self.registered_entities.insert(entity_id, entity_env_key);
        }
        let entity_env_key = self.registered_entities.get(&entity_id).unwrap();

        let env = self.lua.registry_value::<LuaTable>(entity_env_key)?;
        chunk.set_environment(env).exec()?;
        Ok(())
    }

    fn is_entity_registered(&self, entity_id: EntityID) -> bool {
        self.registered_entities.contains_key(&entity_id)
    }

    fn make_entity_env(&self, entity_id: EntityID) -> LuaResult<LuaRegistryKey> {
        let entity_env = self.lua.create_table()?;

        // pre-populate some useful values
        entity_env.set("entity_id", entity_id.to_string())?;

        // link allowed Lua globals
        let aliases = ["print"];
        for &alias in aliases.iter() {
            entity_env.set(alias, self.lua.globals().get::<_, LuaFunction>(alias)?)?;
        }

        let entity_env_key = self.lua.create_registry_value(entity_env)?;
        Ok(entity_env_key)
    }

    pub fn load_script_for_entity(
        &mut self,
        entity_id: EntityID,
        script_name: &str,
    ) -> FFResult<()> {
        let path = format!("scripts/{}.lua", script_name);
        let source = util::get_text_file_contents(&path)?;
        self.load_script_for_entity_internal(entity_id, &source, Some(script_name))
            .map_err(FFError::from_lua_err)
    }

    pub fn load_script_for_entity_raw(
        &mut self,
        entity_id: EntityID,
        source: &str,
    ) -> FFResult<()> {
        self.load_script_for_entity_internal(entity_id, source, None)
            .map_err(FFError::from_lua_err)
    }

    fn tick_entity_script(
        &self,
        entity_id: EntityID,
        _state: &mut ShardServerState,
    ) -> LuaResult<()> {
        let entity_env_key = self.registered_entities.get(&entity_id).unwrap();
        let entity_env = self.lua.registry_value::<LuaTable>(entity_env_key)?;
        let tick_fn = entity_env.get::<_, LuaFunction>("tick")?;
        tick_fn.set_environment(entity_env.clone())?;
        tick_fn.call::<_, ()>(())?;
        Ok(())
    }

    pub fn tick_entity_scripts(&mut self, state: &mut ShardServerState) -> FFResult<()> {
        let to_tick = self.registered_entities.keys().cloned().collect::<Vec<_>>();
        for eid in to_tick {
            match self.tick_entity_script(eid, state) {
                Ok(_) => {}
                Err(e) => {
                    log(
                        Severity::Warning,
                        &format!("Error ticking script for entity {:?}: {}", eid, e),
                    );
                    self.registered_entities.remove(&eid);
                }
            }
        }
        Ok(())
    }
}
