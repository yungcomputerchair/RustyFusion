use mlua::prelude::*;
use rand::thread_rng;

use crate::{
    entity::{Combatant, Entity, EntityID, NPC},
    error::log_if_failed,
    helpers,
    path::Path as NpcPath,
    skills,
    state::ShardServerState,
    tabledata::tdata_get,
    Position,
};

use super::EntityScriptContext;

/// NPC context used in Lua script. Provides NPC bindings to Lua.
#[derive(Debug, Clone, Copy)]
pub(super) struct NpcScriptContext {
    npc_id: i32,
    state: *mut ShardServerState,
}
impl NpcScriptContext {
    pub(super) fn new(npc_id: i32, state: &mut ShardServerState) -> Self {
        Self {
            npc_id,
            state: state as *mut ShardServerState,
        }
    }

    fn with_state<T>(&self, f: impl FnOnce(&mut ShardServerState) -> LuaResult<T>) -> LuaResult<T> {
        // SAFETY: see unsafe impl Send below.
        // These wrappers help avoid aliasing issues by ensuring we only have one mutable reference
        // to state at a time. DO NOT directly dereference self.state outside of these wrappers.
        let state = unsafe { &mut *self.state };
        f(state)
    }

    fn with_npc<T>(&self, f: impl FnOnce(&mut NPC) -> LuaResult<T>) -> LuaResult<T> {
        self.with_state(|state| {
            let npc = state.get_npc_mut(self.npc_id)?;
            f(npc)
        })
    }
}

// SAFETY: NpcScriptContext is only used within a single synchronous resume() call.
// The state pointer is guaranteed valid for that duration.
unsafe impl Send for NpcScriptContext {}

impl FromLua for NpcScriptContext {
    fn from_lua(value: LuaValue, _: &Lua) -> LuaResult<Self> {
        match value {
            LuaValue::UserData(ud) => {
                let npc = ud.borrow::<Self>()?;
                Ok(*npc)
            }
            _ => Err(LuaError::runtime("expected Npc")),
        }
    }
}
impl LuaUserData for NpcScriptContext {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        luau_class!("Npc" extends "Entity", {
            // Properties

            luau_method!(methods, "id" -> "number", |_, this, ()| Ok(this.npc_id));

            luau_method!(methods, "ty" -> "number", |_, this, ()| this.with_npc(|npc| {
                Ok(npc.ty)
            }));

            luau_method!(methods, "spawn_position" -> "Position", |_, this, ()| this.with_npc(|npc| {
                Ok(npc.spawn_position)
            }));

            luau_method!(methods, "retreating" -> "boolean", |_, this, ()| this.with_npc(|npc| {
                Ok(npc.retreating)
            }));

            luau_method!(methods, "is_moving" -> "boolean", |_, this, ()| this.with_npc(|npc| {
                Ok(npc.path.is_some())
            }));

            // Stats

            luau_method!(methods, "walk_speed" -> "number", |_, this, ()| this.with_npc(|npc| {
                let stats = tdata_get()
                    .get_npc_stats(npc.ty)
                    .unwrap();

                Ok(stats.walk_speed)
            }));

            luau_method!(methods, "run_speed" -> "number", |_, this, ()| this.with_npc(|npc| {
                let stats = tdata_get()
                    .get_npc_stats(npc.ty)
                    .unwrap();

                Ok(stats.run_speed)
            }));

            luau_method!(methods, "sight_range" -> "number", |_, this, ()| this.with_npc(|npc| {
                let stats = tdata_get()
                    .get_npc_stats(npc.ty)
                    .unwrap();

                Ok(stats.sight_range)
            }));

            luau_method!(methods, "idle_range" -> "number", |_, this, ()| this.with_npc(|npc| {
                let stats = tdata_get()
                    .get_npc_stats(npc.ty)
                    .unwrap();

                Ok(stats.idle_range)
            }));

            luau_method!(methods, "combat_range" -> "number", |_, this, ()| this.with_npc(|npc| {
                let stats = tdata_get()
                    .get_npc_stats(npc.ty)
                    .unwrap();

                Ok(stats.combat_range)
            }));

            luau_method!(methods, "attack_range" -> "number", |_, this, ()| this.with_npc(|npc| {
                let stats = tdata_get()
                    .get_npc_stats(npc.ty)
                    .map_err(|e| LuaError::runtime(e.to_string()))?;

                Ok(stats.attack_range)
            }));

            luau_method!(methods, "regen_time" -> "number", |_, this, ()| this.with_npc(|npc| {
                let stats = tdata_get()
                    .get_npc_stats(npc.ty)
                    .map_err(|e| LuaError::runtime(e.to_string()))?;

                // Convert from 100ms units to seconds
                Ok(stats.regen_time as f64 / 10.0)
            }));

            luau_method!(methods, "delay_time" -> "number", |_, this, ()| this.with_npc(|npc| {
                let stats = tdata_get()
                    .get_npc_stats(npc.ty)
                    .map_err(|e| LuaError::runtime(e.to_string()))?;

                // Convert from 100ms units to seconds
                Ok(stats.delay_time as f64 / 10.0)
            }));

            // Actions

            luau_method!(methods, "clear_target" -> "()", |_, this, ()| this.with_npc(|npc| {
                npc.target_id = None;
                Ok(())
            }));

            luau_method!(methods, "set_target" -> "()", |_, this, target: EntityScriptContext| this.with_npc(|npc| {
                npc.target_id = Some(target.id());
                Ok(())
            }));

            luau_method!(methods, "attack" -> "()", |_, this, ()| this.with_state(|state| {
                let npc = state.get_npc(this.npc_id)?;
                let target_id = match npc.target_id {
                    Some(id) => id,
                    None => return Err(LuaError::runtime("No target to attack")),
                };

                skills::do_basic_attack(EntityID::NPC(this.npc_id), &[target_id], false, state)?;
                Ok(())
            }));

            luau_method!(methods, "move_to" -> "()",
                |_, this, (x, y, z, speed): (i32, i32, i32, Option<i32>)| this.with_state(|state| {
                    let target_pos = Position { x, y, z };
                    let speed = {
                        let npc = state.get_npc(this.npc_id)?;
                        speed.unwrap_or_else(|| {
                            let dist = npc.get_position().distance_to(&target_pos);
                            let walk_speed = npc.get_speed(false);
                            if dist > walk_speed as u32 {
                                npc.get_speed(true)
                            } else {
                                walk_speed
                            }
                        })
                    };

                    let mut path = NpcPath::new_single(target_pos, speed);
                    path.start();
                    NPC::tick_movement_along_path(this.npc_id, &mut path, state);
                    // Store path so is_moving() works across ticks
                    let npc = state.get_npc_mut(this.npc_id)?;
                    npc.path = Some(path);
                    Ok(())
                })
            );

            luau_method!(methods, "move_toward_entity" -> "()", |_, this, (target, speed): (EntityScriptContext, Option<i32>)| this.with_state(|state| {
                let target_pos = match state.entity_map.get_entity_raw(target.id()) {
                    Some(entity) => entity.get_position(),
                    None => return Err(LuaError::runtime("Entity not found")),
                };

                let (target_pos, too_close, speed) = {
                    let npc = state.get_npc(this.npc_id)?;
                    let stats = tdata_get()
                        .get_npc_stats(npc.ty)
                        .map_err(|e| LuaError::runtime(e.to_string()))?;

                    let following_distance = stats.radius;
                    let (target_pos, too_close) =
                        target_pos.interpolate(&npc.get_position(), following_distance as f32);

                    let speed = speed.unwrap_or_else(|| {
                        let dist = npc.get_position().distance_to(&target_pos);
                        let walk_speed = npc.get_speed(false);
                        if dist > walk_speed as u32 {
                            npc.get_speed(true)
                        } else {
                            walk_speed
                        }
                    });

                    (target_pos, too_close, speed)
                };

                if too_close {
                    return Ok(());
                }

                let mut path = NpcPath::new_single(target_pos, speed);
                path.start();
                NPC::tick_movement_along_path(this.npc_id, &mut path, state);
                let npc = state.get_npc_mut(this.npc_id)?;
                npc.path = Some(path);
                Ok(())
            }));

            luau_method!(methods, "stop" -> "()", |_, this, ()| this.with_npc(|npc| {
                npc.path = None;
                Ok(())
            }));

            luau_method!(methods, "set_retreating" -> "()", |_, this, retreating: bool| this.with_npc(|npc| {
                npc.retreating = retreating;
                Ok(())
            }));

            luau_method!(methods, "begin_death" -> "()", |_, this, ()| this.with_state(|state| {
                let last_attacked_by = state.get_npc(this.npc_id)?.last_attacked_by;
                if let Some(defeater_id) = last_attacked_by {
                    let mut rng = thread_rng();
                    log_if_failed(helpers::on_mob_defeated(
                        this.npc_id,
                        defeater_id,
                        state,
                        &mut rng,
                    ));
                }
                Ok(())
            }));

            luau_method!(methods, "despawn" -> "()", |_, this, ()| this.with_state(|state| {
                let entity_id = EntityID::NPC(this.npc_id);
                let summoned = state.get_npc(this.npc_id)?.summoned;
                state.entity_map.update(entity_id, None, true);
                if summoned {
                    state.entity_map.mark_for_cleanup(entity_id);
                }
                Ok(())
            }));

            luau_method!(methods, "respawn" -> "()", |_, this, ()| this.with_state(|state| {
                let chunk_pos = {
                    let npc = state.get_npc_mut(this.npc_id)?;
                    npc.reset();
                    npc.set_position(npc.spawn_position);
                    npc.get_chunk_coords()
                };
                state.entity_map.update(EntityID::NPC(this.npc_id), Some(chunk_pos), true);
                Ok(())
            }));

            luau_method!(methods, "set_spawn_position" -> "()",
                |_, this, (x, y, z): (i32, i32, i32)| this.with_npc(|npc| {
                    npc.spawn_position = Position { x, y, z };
                    Ok(())
                })
            );

            // Helpers

            luau_method!(methods, "in_attack_range" -> "boolean", |_, this, ()| this.with_state(|state| {
                let npc = state.get_npc(this.npc_id)?;
                let target_id = match npc.target_id {
                    Some(id) => id,
                    None => return Ok(false),
                };
                let stats = tdata_get()
                    .get_npc_stats(npc.ty)
                    .map_err(|e| LuaError::runtime(e.to_string()))?;
                let attack_range = stats.attack_range + stats.radius;
                let npc_pos = npc.get_position();
                let target = match state.entity_map.get_entity_raw(target_id) {
                    Some(entity) => entity,
                    None => return Ok(false),
                };
                Ok(npc_pos.distance_to(&target.get_position()) <= attack_range)
            }));

            luau_method!(methods, "find_nearest_enemy" -> "Entity?", |_, this, range: u32| this.with_state(|state| {
                let (npc_team, npc_pos) = {
                    let npc = state.get_npc(this.npc_id)?;
                    (npc.get_team(), npc.get_position())
                };

                let mut nearest_id: Option<EntityID> = None;
                let mut nearest_dist = u32::MAX;

                for eid in state.entity_map.get_around_entity(EntityID::NPC(this.npc_id)) {
                    let entity = match state.entity_map.get_entity_raw(eid) {
                        Some(e) => e,
                        None => continue,
                    };
                    let cb = match entity.as_combatant() {
                        Some(cb) => cb,
                        None => continue,
                    };
                    if cb.is_dead() || cb.get_team() == npc_team || cb.get_aggro_factor() <= 0.0 {
                        continue;
                    }
                    let dist = npc_pos.distance_to(&cb.get_position());
                    if dist <= range && dist < nearest_dist {
                        nearest_dist = dist;
                        nearest_id = Some(eid);
                    }
                }

                match nearest_id {
                    Some(eid) => Ok(Some(EntityScriptContext::new(eid, state))),
                    None => Ok(None),
                }
            }));

            luau_method!(methods, "get_follow_target" -> "Entity?", |_, this, ()| this.with_state(|state| {
                let loose_follow = state.get_npc(this.npc_id)?.loose_follow;
                match loose_follow {
                    Some(eid) => Ok(Some(EntityScriptContext::new(eid, state))),
                    None => Ok(None),
                }
            }));

            luau_method!(methods, "get_pack_leader" -> "Npc?", |_, this, ()| this.with_state(|state| {
                let tight_follow = state.get_npc(this.npc_id)?.tight_follow;
                let leader_id = match tight_follow {
                    Some((leader_id, _)) => leader_id,
                    None => return Ok(None),
                };

                let leader_npc_id = match leader_id {
                    EntityID::NPC(npc_id) => npc_id,
                    _ => return Ok(None),
                };

                Ok(Some(NpcScriptContext::new(leader_npc_id, state)))
            }));

            luau_method!(methods, "get_pack_offset" -> "Position", |lua, this, ()| this.with_npc(|npc| {
                let offset = npc.tight_follow
                    .map(|(_, off)| off)
                    .unwrap_or_default();

                let table = lua.create_table()?;
                table.set("x", offset.x)?;
                table.set("y", offset.y)?;
                table.set("z", offset.z)?;
                Ok(table)
            }));

            luau_method!(methods, "find_enemies_in_range" -> "{Entity}", |lua, this, range: u32| this.with_state(|state| {
                let (npc_team, npc_pos) = {
                    let npc = state.get_npc(this.npc_id)?;
                    (npc.get_team(), npc.get_position())
                };

                let result = lua.create_table()?;
                let mut idx = 1;

                let nearby = state.entity_map.get_around_entity(EntityID::NPC(this.npc_id));
                for eid in nearby {
                    let in_range = {
                        let entity = match state.entity_map.get_entity_raw(eid) {
                            Some(e) => e,
                            None => continue,
                        };
                        let cb = match entity.as_combatant() {
                            Some(cb) => cb,
                            None => continue,
                        };
                        if cb.is_dead() || cb.get_team() == npc_team || cb.get_aggro_factor() <= 0.0 {
                            continue;
                        }
                        npc_pos.distance_to(&cb.get_position()) <= range
                    };
                    if in_range {
                        result.set(idx, EntityScriptContext::new(eid, state))?;
                        idx += 1;
                    }
                }

                Ok(result)
            }));

            methods.add_meta_method(LuaMetaMethod::Eq, |_, this, other: NpcScriptContext| {
                Ok(this.npc_id == other.npc_id)
            });

            methods.add_meta_method(LuaMetaMethod::ToString, |_, this, ()| this.with_npc(|npc| {
                Ok(format!("{}", npc))
            }));

            methods.add_meta_method(LuaMetaMethod::Index, |lua, _, key: String| {
                // Only capture the key — no state pointer in the closure.
                // The NpcScriptContext (with its fresh pointer) is extracted from self at call time.
                let wrapper = lua.create_function(move |lua, args: LuaMultiValue| {
                    let mut args_vec: Vec<_> = args.into_iter().collect();
                    let this_val = args_vec
                        .first()
                        .ok_or_else(|| LuaError::runtime("missing self"))?
                        .clone();

                    let npc = NpcScriptContext::from_lua(this_val, lua)?;
                    npc.with_state(|state| {
                        let entity = EntityScriptContext::new(EntityID::NPC(npc.npc_id), state);
                        let entity_ud = lua.create_userdata(entity)?;
                        let method: LuaFunction = entity_ud.get(key.clone())?;
                        args_vec[0] = LuaValue::UserData(entity_ud);
                        method.call::<LuaMultiValue>(LuaMultiValue::from_vec(args_vec))
                    })
                })?;
                Ok(LuaValue::Function(wrapper))
            });
        }); // luau_class
    }
}
