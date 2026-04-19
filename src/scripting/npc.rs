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
    npc: *mut NPC,
    state: *mut ShardServerState,
}
impl NpcScriptContext {
    pub(super) fn new(npc: &mut NPC, state: &mut ShardServerState) -> Self {
        Self {
            npc: npc as *mut NPC,
            state: state as *mut ShardServerState,
        }
    }

    // SAFETY: see `unsafe impl Send` below.

    fn npc(&self) -> &NPC {
        unsafe { &*self.npc }
    }

    #[allow(clippy::mut_from_ref)]
    fn npc_mut(&self) -> &mut NPC {
        unsafe { &mut *self.npc }
    }

    #[allow(clippy::mut_from_ref)]
    fn state_mut(&self) -> &mut ShardServerState {
        unsafe { &mut *self.state }
    }
}

// SAFETY: NpcScriptContext is only used within a single synchronous resume() call.
// The pointers are guaranteed valid for that duration.
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

            luau_method!(methods, "id" -> "number", |_, this, ()| Ok(this.npc().id));

            luau_method!(methods, "ty" -> "number", |_, this, ()| Ok(this.npc().ty));

            luau_method!(methods, "spawn_position" -> "Position", |_, this, ()| {
                Ok(this.npc().spawn_position)
            });

            luau_method!(methods, "retreating" -> "boolean", |_, this, ()| Ok(this.npc().retreating));

            luau_method!(methods, "is_moving" -> "boolean", |_, this, ()| Ok(this.npc().path.is_some()));

            // Stats

            luau_method!(methods, "walk_speed" -> "number", |_, this, ()| {
                let stats = tdata_get()
                    .get_npc_stats(this.npc().ty)
                    .map_err(|e| LuaError::runtime(e.to_string()))?;

                Ok(stats.walk_speed)
            });

            luau_method!(methods, "run_speed" -> "number", |_, this, ()| {
                let stats = tdata_get()
                    .get_npc_stats(this.npc().ty)
                    .map_err(|e| LuaError::runtime(e.to_string()))?;

                Ok(stats.run_speed)
            });

            luau_method!(methods, "sight_range" -> "number", |_, this, ()| {
                let stats = tdata_get()
                    .get_npc_stats(this.npc().ty)
                    .map_err(|e| LuaError::runtime(e.to_string()))?;

                Ok(stats.sight_range)
            });

            luau_method!(methods, "idle_range" -> "number", |_, this, ()| {
                let stats = tdata_get()
                    .get_npc_stats(this.npc().ty)
                    .map_err(|e| LuaError::runtime(e.to_string()))?;

                Ok(stats.idle_range)
            });

            luau_method!(methods, "combat_range" -> "number", |_, this, ()| {
                let stats = tdata_get()
                    .get_npc_stats(this.npc().ty)
                    .map_err(|e| LuaError::runtime(e.to_string()))?;

                Ok(stats.combat_range)
            });

            luau_method!(methods, "attack_range" -> "number", |_, this, ()| {
                let stats = tdata_get()
                    .get_npc_stats(this.npc().ty)
                    .map_err(|e| LuaError::runtime(e.to_string()))?;

                Ok(stats.attack_range)
            });

            luau_method!(methods, "regen_time" -> "number", |_, this, ()| {
                let stats = tdata_get()
                    .get_npc_stats(this.npc().ty)
                    .map_err(|e| LuaError::runtime(e.to_string()))?;

                // Convert from 100ms units to seconds
                Ok(stats.regen_time as f64 / 10.0)
            });

            luau_method!(methods, "delay_time" -> "number", |_, this, ()| {
                let stats = tdata_get()
                    .get_npc_stats(this.npc().ty)
                    .map_err(|e| LuaError::runtime(e.to_string()))?;

                // Convert from 100ms units to seconds
                Ok(stats.delay_time as f64 / 10.0)
            });

            // Actions

            luau_method!(methods, "clear_target" -> "()", |_, this, ()| {
                this.npc_mut().target_id = None;
                Ok(())
            });

            luau_method!(methods, "set_target" -> "()", |_, this, target: EntityScriptContext| {
                this.npc_mut().target_id = Some(target.id());
                Ok(())
            });

            luau_method!(methods, "attack" -> "()", |_, this, ()| {
                let npc = this.npc();
                let target_id = match npc.target_id {
                    Some(id) => id,
                    None => return Err(LuaError::runtime("No target to attack")),
                };
                let state = this.state_mut();
                skills::do_basic_attack(npc.get_id(), &[target_id], false, state)
                    .map_err(|e| LuaError::runtime(e.to_string()))
            });

            luau_method!(methods, "move_to" -> "()",
                |_, this, (x, y, z, speed): (i32, i32, i32, Option<i32>)| {
                    let npc = this.npc_mut();
                    let state = this.state_mut();
                    let target_pos = Position { x, y, z };
                    let speed = speed.unwrap_or_else(|| {
                        let dist = npc.get_position().distance_to(&target_pos);
                        let walk_speed = npc.get_speed(false);
                        if dist > walk_speed as u32 {
                            npc.get_speed(true)
                        } else {
                            walk_speed
                        }
                    });

                    let mut path = NpcPath::new_single(target_pos, speed);
                    path.start();
                    npc.tick_movement_along_path(&mut path, state);
                    // Store path so is_moving() works across ticks
                    npc.path = Some(path);
                    Ok(())
                }
            );

            luau_method!(methods, "move_toward_entity" -> "()", |_, this, (target, speed): (EntityScriptContext, Option<i32>)| {
                let npc = this.npc_mut();
                let state = this.state_mut();
                let target_pos = match state.entity_map.get_entity_raw(target.id()) {
                    Some(entity) => entity.get_position(),
                    None => return Err(LuaError::runtime("Entity not found")),
                };

                let stats = tdata_get()
                    .get_npc_stats(npc.ty)
                    .map_err(|e| LuaError::runtime(e.to_string()))?;

                let following_distance = stats.radius;
                let (target_pos, too_close) =
                    target_pos.interpolate(&npc.get_position(), following_distance as f32);

                if too_close {
                    return Ok(());
                }

                let speed = speed.unwrap_or_else(|| {
                    let dist = npc.get_position().distance_to(&target_pos);
                    let walk_speed = npc.get_speed(false);
                    if dist > walk_speed as u32 {
                        npc.get_speed(true)
                    } else {
                        walk_speed
                    }
                });

                let mut path = NpcPath::new_single(target_pos, speed);
                path.start();
                npc.tick_movement_along_path(&mut path, state);
                npc.path = Some(path);
                Ok(())
            });

            luau_method!(methods, "stop" -> "()", |_, this, ()| {
                this.npc_mut().path = None;
                Ok(())
            });

            luau_method!(methods, "set_retreating" -> "()", |_, this, retreating: bool| {
                this.npc_mut().retreating = retreating;
                Ok(())
            });

            luau_method!(methods, "begin_death" -> "()", |_, this, ()| {
                let npc = this.npc();
                let state = this.state_mut();
                if let Some(defeater_id) = npc.last_attacked_by {
                    let mut rng = thread_rng();
                    log_if_failed(helpers::on_mob_defeated(
                        npc.id,
                        defeater_id,
                        state,
                        &mut rng,
                    ));
                }
                Ok(())
            });

            luau_method!(methods, "despawn" -> "()", |_, this, ()| {
                let npc = this.npc();
                let state = this.state_mut();
                state.entity_map.update(npc.get_id(), None, true);
                if npc.summoned {
                    state.entity_map.mark_for_cleanup(npc.get_id());
                }
                Ok(())
            });

            luau_method!(methods, "respawn" -> "()", |_, this, ()| {
                let npc = this.npc_mut();
                let state = this.state_mut();
                npc.reset();
                npc.set_position(npc.spawn_position);
                let chunk_pos = npc.get_chunk_coords();
                state.entity_map.update(npc.get_id(), Some(chunk_pos), true);
                Ok(())
            });

            luau_method!(methods, "set_spawn_position" -> "()",
                |_, this, (x, y, z): (i32, i32, i32)| {
                    this.npc_mut().spawn_position = Position { x, y, z };
                    Ok(())
                }
            );

            // Helpers

            luau_method!(methods, "in_attack_range" -> "boolean", |_, this, ()| {
                let npc = this.npc();
                let state = this.state_mut();
                let target_id = match npc.target_id {
                    Some(id) => id,
                    None => return Ok(false),
                };
                let stats = tdata_get()
                    .get_npc_stats(npc.ty)
                    .map_err(|e| LuaError::runtime(e.to_string()))?;
                let attack_range = stats.attack_range + stats.radius;
                let target = match state.entity_map.get_entity_raw(target_id) {
                    Some(entity) => entity,
                    None => return Ok(false),
                };
                Ok(npc.get_position().distance_to(&target.get_position()) <= attack_range)
            });

            luau_method!(methods, "find_nearest_enemy" -> "Entity?", |_, this, range: u32| {
                let npc = this.npc();
                let state = this.state_mut();
                let npc_team = npc.get_team();
                let npc_pos = npc.get_position();

                let mut nearest_id: Option<EntityID> = None;
                let mut nearest_dist = u32::MAX;

                for eid in state.entity_map.get_around_entity(npc.get_id()) {
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
                    Some(eid) => Ok(Some(EntityScriptContext::new(eid, this.state_mut()))),
                    None => Ok(None),
                }
            });

            luau_method!(methods, "get_follow_target" -> "Entity?", |_, this, ()| {
                Ok(this.npc().loose_follow.map(|eid| EntityScriptContext::new(eid, this.state_mut())))
            });

            luau_method!(methods, "get_pack_leader" -> "Npc?", |_, this, ()| {
                let leader_id = match this.npc().tight_follow {
                    Some((leader_id, _)) => leader_id,
                    None => return Ok(None),
                };

                let leader_npc_id = match leader_id {
                    EntityID::NPC(npc_id) => npc_id,
                    _ => return Ok(None),
                };

                // If the leader is the NPC being ticked, use the existing raw
                // pointer to avoid creating a second &mut reference.
                if leader_npc_id == this.npc().id {
                    return Ok(Some(*this));
                }

                let state = this.state_mut();
                let leader = match state.get_npc_mut(leader_npc_id) {
                    Ok(npc) => npc,
                    Err(_) => return Ok(None),
                };

                Ok(Some(NpcScriptContext::new(leader, this.state_mut())))
            });

            luau_method!(methods, "get_pack_offset" -> "Position", |lua, this, ()| {
                let offset = this.npc().tight_follow
                    .map(|(_, off)| off)
                    .unwrap_or_default();

                let table = lua.create_table()?;
                table.set("x", offset.x)?;
                table.set("y", offset.y)?;
                table.set("z", offset.z)?;
                Ok(table)
            });

            luau_method!(methods, "find_enemies_in_range" -> "{Entity}", |lua, this, range: u32| {
                let npc = this.npc();
                let state = this.state_mut();
                let npc_team = npc.get_team();
                let npc_pos = npc.get_position();

                let result = lua.create_table()?;
                let mut idx = 1;

                for eid in state.entity_map.get_around_entity(npc.get_id()) {
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
                    if npc_pos.distance_to(&cb.get_position()) > range {
                        continue;
                    }
                    result.set(idx, EntityScriptContext::new(eid, this.state_mut()))?;
                    idx += 1;
                }

                Ok(result)
            });

            methods.add_meta_method(LuaMetaMethod::Eq, |_, this, other: NpcScriptContext| {
                Ok(this.npc().get_id() == other.npc().get_id())
            });

            methods.add_meta_method(LuaMetaMethod::ToString, |_, this, ()| {
                let npc = this.npc();
                Ok(format!("{}", npc))
            });

            methods.add_meta_method(LuaMetaMethod::Index, |lua, this, key: String| {
                let entity = EntityScriptContext::new(
                    EntityID::NPC(this.npc().id),
                    this.state_mut(),
                );

                // Entity methods expect EntityScriptContext, but the metamethod passes self as the first arg
                // even with __index, so we need to wrap the method to insert self as EntityScriptContext as the first arg.
                let entity_ud = lua.create_userdata(entity)?;
                let method: LuaFunction = entity_ud.get(key)?;
                let wrapper = lua.create_function(move |_, args: LuaMultiValue| {
                    let mut new_args = Vec::with_capacity(args.len());
                    new_args.push(LuaValue::UserData(entity_ud.clone()));
                    new_args.extend(args.into_iter().skip(1));
                    method.call::<LuaMultiValue>(LuaMultiValue::from_vec(new_args))
                })?;

                Ok(LuaValue::Function(wrapper))
            });
        }); // luau_class
    }
}
