use std::time::Duration;

use mlua::prelude::*;

use crate::{
    entity::{Entity, EntityID},
    enums::{BuffID, BuffType},
    skills::BuffInstance,
    state::ShardServerState,
    Position,
};

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

    fn with_state<T>(&self, f: impl FnOnce(&mut ShardServerState) -> LuaResult<T>) -> LuaResult<T> {
        // SAFETY: see unsafe impl Send below.
        // These wrappers help avoid aliasing issues by ensuring we only have one mutable reference
        // to state at a time. DO NOT directly dereference self.state outside of these wrappers.
        let state = unsafe { &mut *self.state };
        f(state)
    }

    fn with_entity<T>(&self, f: impl FnOnce(&mut dyn Entity) -> LuaResult<T>) -> LuaResult<T> {
        self.with_state(|state| {
            let entity = state.get_entity_mut(self.entity_id)?;
            f(entity)
        })
    }
}

// SAFETY: EntityScriptContext is only used within a single synchronous resume() call.
// The state pointer is guaranteed valid for that duration.
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

            luau_method!(methods, "position" -> "Position", |_, this, ()| this.with_entity(|entity| {
                Ok(entity.get_position())
            }));

            luau_method!(methods, "level" -> "number", |_, this, ()| this.with_entity(|entity| {
                if let Some(combatant) = entity.as_combatant() {
                    Ok(combatant.get_level())
                } else {
                    Ok(0)
                }
            }));

            luau_method!(methods, "hp" -> "number", |_, this, ()| this.with_entity(|entity| {
                if let Some(combatant) = entity.as_combatant() {
                    Ok(combatant.get_hp())
                } else {
                    Ok(0)
                }
            }));

            luau_method!(methods, "max_hp" -> "number", |_, this, ()| this.with_entity(|entity| {
                if let Some(combatant) = entity.as_combatant() {
                    Ok(combatant.get_max_hp())
                } else {
                    Ok(0)
                }
            }));

            luau_method!(methods, "is_dead" -> "boolean", |_, this, ()| this.with_entity(|entity| {
                if let Some(combatant) = entity.as_combatant() {
                    Ok(combatant.is_dead())
                } else {
                    Ok(false)
                }
            }));

            luau_method!(methods, "reset" -> "()", |_, this, ()| this.with_entity(|entity| {
                if let Some(combatant) = entity.as_combatant_mut() {
                    combatant.reset();
                }
                Ok(())
            }));

            luau_method!(methods, "target" -> "Entity?", |_, this, ()| this.with_state(|state| {
                let entity = state.get_entity(this.entity_id)?;
                if let Some(combatant) = entity.as_combatant() {
                    Ok(combatant.get_target().map(|target_id| EntityScriptContext::new(target_id, state)))
                } else {
                    Ok(None)
                }
            }));

            luau_method!(methods, "apply_buff" -> "boolean", |_, this, (buff_id, values, duration, source): (i32, Vec<i32>, Option<f32>, Option<EntityScriptContext>)| this.with_state(|state| {
                let entity = state.get_entity_mut(this.entity_id)?;
                if let Some(combatant) = entity.as_combatant_mut() {
                    let buff_id: BuffID = buff_id.try_into().map_err(|_| LuaError::runtime(format!("Invalid buff ID: {}", buff_id)))?;
                    let value = values.first().cloned().unwrap_or(0);
                    let sub_value = values.get(1).cloned();
                    let special_value = values.get(2).cloned();
                    let duration = duration.map(Duration::from_secs_f32);
                    let buff = BuffInstance::new(BuffType::Shiny, value, sub_value, special_value, duration);
                    combatant.apply_buff(buff_id, buff, source.map(|s| s.entity_id));
                    Ok(true)
                } else {
                    Ok(false)
                }
            }));

            // Helpers

            luau_method!(methods, "distance_to" -> "number", |_, this, (x, y, z): (i32, i32, i32)| this.with_entity(|entity| {
                let pos = entity.get_position();
                let target = Position { x, y, z };
                Ok(pos.distance_to(&target))
            }));

            luau_method!(methods, "distance_to_entity" -> "number", |_, this, target: EntityScriptContext| this.with_state(|state| {
                let pos = state.get_entity(this.entity_id)?.get_position();
                let target_entity = state.entity_map.get_entity_raw(target.entity_id).ok_or_else(|| LuaError::runtime("Target entity not found"))?;
                let target_pos = target_entity.get_position();
                Ok(pos.distance_to(&target_pos))
            }));
        });

        methods.add_meta_method(LuaMetaMethod::Eq, |_, this, other: EntityScriptContext| {
            Ok(this.entity_id == other.entity_id)
        });

        methods.add_meta_method(LuaMetaMethod::ToString, |_, this, ()| {
            this.with_entity(|entity| Ok(format!("{}", entity)))
        });
    }
}
