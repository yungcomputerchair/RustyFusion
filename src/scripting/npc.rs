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

use super::LuaEntityID;

/// NPC context used in Lua script. Provides bindings to Lua.
pub(super) struct NpcScriptContext {
    npc: *mut NPC,
    state: *mut ShardServerState,
}

// SAFETY: NpcScriptContext is only used within a single synchronous resume() call.
// The pointers are guaranteed valid for that duration.
unsafe impl Send for NpcScriptContext {}

impl NpcScriptContext {
    pub(super) fn new(npc: &mut NPC, state: &mut ShardServerState) -> Self {
        Self {
            npc: npc as *mut NPC,
            state: state as *mut ShardServerState,
        }
    }

    // SAFETY for all three accessors: NpcScriptContext is only created from valid
    // &mut references in tick_npc() and only used within a single synchronous
    // resume() call. No aliasing is possible during that window.

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

impl LuaUserData for NpcScriptContext {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        luau_class!("Npc", {
            // Properties

            luau_method!(methods, "id" -> "number", |_, this, ()| Ok(this.npc().id));

            luau_method!(methods, "ty" -> "number", |_, this, ()| Ok(this.npc().ty));

            luau_method!(methods, "position" -> "Position", |lua, this, ()| {
                let pos = this.npc().get_position();
                let table = lua.create_table()?;
                table.set("x", pos.x)?;
                table.set("y", pos.y)?;
                table.set("z", pos.z)?;
                Ok(table)
            });

            luau_method!(methods, "spawn_position" -> "Position", |lua, this, ()| {
                let pos = this.npc().spawn_position;
                let table = lua.create_table()?;
                table.set("x", pos.x)?;
                table.set("y", pos.y)?;
                table.set("z", pos.z)?;
                Ok(table)
            });

            luau_method!(methods, "hp" -> "number", |_, this, ()| Ok(this.npc().get_hp()));

            luau_method!(methods, "max_hp" -> "number", |_, this, ()| {
                let stats = tdata_get()
                    .get_npc_stats(this.npc().ty)
                    .map_err(|e| LuaError::runtime(e.to_string()))?;
                Ok(stats.max_hp as i32)
            });

            luau_method!(methods, "level" -> "number", |_, this, ()| {
                let stats = tdata_get()
                    .get_npc_stats(this.npc().ty)
                    .map_err(|e| LuaError::runtime(e.to_string()))?;
                Ok(stats.level)
            });

            luau_method!(methods, "is_dead" -> "boolean", |_, this, ()| Ok(this.npc().get_hp() <= 0));

            luau_method!(methods, "has_target" -> "boolean", |_, this, ()| {
                Ok(this.npc().target_id.is_some())
            });

            luau_method!(methods, "is_target_alive" -> "boolean", |_, this, ()| {
                let npc = this.npc();
                let target_id = match npc.target_id {
                    Some(id) => id,
                    None => return Ok(false),
                };
                let state = this.state_mut();
                match state.get_combatant(target_id) {
                    Ok(target) => Ok(!target.is_dead()),
                    Err(_) => Ok(false),
                }
            });

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

            luau_method!(methods, "set_target" -> "()", |_, this, target: LuaEntityID| {
                this.npc_mut().target_id = Some(target.0);
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
                |_, this, (x, y, z, speed): (i32, i32, i32, i32)| {
                    let npc = this.npc_mut();
                    let state = this.state_mut();
                    let target_pos = Position { x, y, z };
                    let mut path = NpcPath::new_single(target_pos, speed);
                    path.start();
                    npc.tick_movement_along_path(&mut path, state);
                    // Store path so is_moving() works across ticks
                    npc.path = Some(path);
                    Ok(())
                }
            );

            luau_method!(methods, "move_toward_target" -> "()", |_, this, speed: i32| {
                let npc = this.npc_mut();
                let state = this.state_mut();
                let target_id = match npc.target_id {
                    Some(id) => id,
                    None => return Err(LuaError::runtime("No target to move toward")),
                };

                let target_pos = match state.entity_map.get_entity_raw(target_id) {
                    Some(entity) => entity.get_position(),
                    None => return Err(LuaError::runtime("Target entity not found")),
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

                let mut path = NpcPath::new_single(target_pos, speed);
                path.start();
                npc.tick_movement_along_path(&mut path, state);
                Ok(())
            });

            luau_method!(methods, "move_toward_entity" -> "()", |_, this, (target, speed): (LuaEntityID, i32)| {
                let npc = this.npc_mut();
                let state = this.state_mut();
                let target_pos = match state.entity_map.get_entity_raw(target.0) {
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

                let mut path = NpcPath::new_single(target_pos, speed);
                path.start();
                npc.tick_movement_along_path(&mut path, state);
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
                let chunk_pos = npc.get_chunk_coords();
                state.entity_map.update(npc.get_id(), Some(chunk_pos), true);
                Ok(())
            });

            luau_method!(methods, "set_position" -> "()", |_, this, (x, y, z): (i32, i32, i32)| {
                this.npc_mut().set_position(Position { x, y, z });
                Ok(())
            });

            luau_method!(methods, "set_spawn_position" -> "()",
                |_, this, (x, y, z): (i32, i32, i32)| {
                    this.npc_mut().spawn_position = Position { x, y, z };
                    Ok(())
                }
            );

            // Helpers

            luau_method!(methods, "distance_to" -> "number", |_, this, (x, y, z): (i32, i32, i32)| {
                let npc = this.npc();
                let target = Position { x, y, z };
                Ok(npc.get_position().distance_to(&target))
            });

            luau_method!(methods, "distance_to_entity" -> "number", |_, this, target: LuaEntityID| {
                let npc = this.npc();
                let state = this.state_mut();
                let target = state
                    .entity_map
                    .get_entity_raw(target.0)
                    .ok_or_else(|| LuaError::runtime("Entity not found"))?;
                Ok(npc.get_position().distance_to(&target.get_position()))
            });

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
                    Some(eid) => Ok(Some(LuaEntityID(eid))),
                    None => Ok(None),
                }
            });

            luau_method!(methods, "get_follow_target" -> "Entity?", |_, this, ()| {
                Ok(this.npc().loose_follow.map(LuaEntityID))
            });

            luau_method!(methods, "get_entity_target" -> "Entity?", |_, this, target: LuaEntityID| {
                let state = this.state_mut();
                match target.0 {
                    EntityID::NPC(npc_id) => {
                        let npc = state.get_npc(npc_id)
                            .map_err(|e| LuaError::runtime(e.to_string()))?;
                        Ok(npc.target_id.map(LuaEntityID))
                    }
                    EntityID::Player(pc_id) => {
                        let player = state.get_player(pc_id)
                            .map_err(|e| LuaError::runtime(e.to_string()))?;
                        Ok(player.last_attacked_by.map(LuaEntityID))
                    }
                    _ => Ok(None),
                }
            });

            luau_method!(methods, "random_point_in_range" -> "Position", |lua, this, range: u32| {
                let npc = this.npc();
                let pos = npc.spawn_position;
                let target = pos.get_random_around(range, range, 0);
                let table = lua.create_table()?;
                table.set("x", target.x)?;
                table.set("y", target.y)?;
                table.set("z", target.z)?;
                Ok(table)
            });
        }); // luau_class
    }
}
