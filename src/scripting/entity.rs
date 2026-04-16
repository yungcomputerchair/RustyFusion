use mlua::prelude::*;

use crate::{entity::EntityID, state::ShardServerState};

/// Entity context used in Lua script. Provides dyn Entity bindings to Lua.
#[derive(Debug, Clone, Copy)]
pub(super) struct EntityScriptContext {
    entity_id: EntityID,
    state: *mut ShardServerState,
}
impl EntityScriptContext {
    pub fn new(entity_id: EntityID, state: &mut ShardServerState) -> Self {
        Self {
            entity_id,
            state: state as *mut ShardServerState,
        }
    }

    pub fn id(&self) -> EntityID {
        self.entity_id
    }

    // SAFETY: see `unsafe impl Send` below.

    #[allow(clippy::mut_from_ref)]
    fn state_mut(&self) -> &mut ShardServerState {
        unsafe { &mut *self.state }
    }
}

// SAFETY: EntityScriptContext is only used within a single synchronous resume() call.
// The pointers are guaranteed valid for that duration.
unsafe impl Send for EntityScriptContext {}

impl FromLua for EntityScriptContext {
    fn from_lua(value: LuaValue, _: &Lua) -> LuaResult<Self> {
        match value {
            LuaValue::UserData(ud) => {
                let entity = ud.borrow::<Self>()?;
                Ok(*entity)
            }
            _ => Err(LuaError::runtime("expected Entity")),
        }
    }
}
impl LuaUserData for EntityScriptContext {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        luau_class!("Entity", {
            luau_method!(methods, "is_player" -> "boolean", |_, this, ()| {
                Ok(matches!(this.entity_id, EntityID::Player(_)))
            });

            luau_method!(methods, "is_npc" -> "boolean", |_, this, ()| {
                Ok(matches!(this.entity_id, EntityID::NPC(_)))
            });

            luau_method!(methods, "is_slider" -> "boolean", |_, this, ()| {
                Ok(matches!(this.entity_id, EntityID::Slider(_)))
            });

            luau_method!(methods, "is_egg" -> "boolean", |_, this, ()| {
                Ok(matches!(this.entity_id, EntityID::Egg(_)))
            });

            luau_method!(methods, "position" -> "Position", |_, this, ()| {
                let state = this.state_mut();
                let entity = state.entity_map.get_entity_raw(this.entity_id).ok_or_else(|| LuaError::runtime("Entity not found"))?;
                Ok(entity.get_position())
            });

            luau_method!(methods, "level" -> "number", |_, this, ()| {
                let state = this.state_mut();
                let entity = state.entity_map.get_entity_raw(this.entity_id).ok_or_else(|| LuaError::runtime("Entity not found"))?;
                if let Some(combatant) = entity.as_combatant() {
                    Ok(combatant.get_level())
                } else {
                    Ok(0)
                }
            });

            luau_method!(methods, "is_dead" -> "boolean", |_, this, ()| {
                let state = this.state_mut();
                let entity = state.entity_map.get_entity_raw(this.entity_id).ok_or_else(|| LuaError::runtime("Entity not found"))?;
                if let Some(combatant) = entity.as_combatant() {
                    Ok(combatant.is_dead())
                } else {
                    Ok(false)
                }
            });

            luau_method!(methods, "target" -> "Entity?", |_, this, ()| {
                let state = this.state_mut();
                let entity = state.entity_map.get_entity_raw(this.entity_id).ok_or_else(|| LuaError::runtime("Entity not found"))?;
                if let Some(combatant) = entity.as_combatant() {
                    Ok(combatant.get_target().map(|target_id| EntityScriptContext::new(target_id, state)))
                } else {
                    Ok(None)
                }
            });
        });

        methods.add_meta_method(LuaMetaMethod::Eq, |_, this, other: EntityScriptContext| {
            Ok(this.entity_id == other.entity_id)
        });

        methods.add_meta_method(LuaMetaMethod::ToString, |_, this, ()| {
            let state = this.state_mut();
            let entity = state
                .entity_map
                .get_entity_raw(this.entity_id)
                .ok_or_else(|| LuaError::runtime("Entity not found"))?;
            Ok(format!("{}", entity))
        });
    }
}
