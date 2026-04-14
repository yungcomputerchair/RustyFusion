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

// ==================== NPC Handle (Lua UserData) ====================

/// Lightweight proxy passed to Lua scripts during a single tick.
/// This holds raw pointers because mlua's scope API requires 'static,
/// and we guarantee the references are valid for the duration of resume().
pub(super) struct NpcHandle {
    npc: *mut NPC,
    state: *mut ShardServerState,
}

// SAFETY: NpcHandle is only used within a single synchronous resume() call.
// The pointers are guaranteed valid for that duration.
unsafe impl Send for NpcHandle {}

impl NpcHandle {
    pub(super) fn new(npc: &mut NPC, state: &mut ShardServerState) -> Self {
        Self {
            npc: npc as *mut NPC,
            state: state as *mut ShardServerState,
        }
    }

    // SAFETY for all three accessors: NpcHandle is only created from valid
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

impl LuaUserData for NpcHandle {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        // ---- Properties (read-only) ----

        methods.add_method("id", |_, this, ()| Ok(this.npc().id));

        methods.add_method("type_id", |_, this, ()| Ok(this.npc().ty));

        methods.add_method("position", |lua, this, ()| {
            let pos = this.npc().get_position();
            let table = lua.create_table()?;
            table.set("x", pos.x)?;
            table.set("y", pos.y)?;
            table.set("z", pos.z)?;
            Ok(table)
        });

        methods.add_method("hp", |_, this, ()| Ok(this.npc().get_hp()));

        methods.add_method("max_hp", |_, this, ()| {
            let stats = tdata_get()
                .get_npc_stats(this.npc().ty)
                .map_err(|e| LuaError::runtime(e.to_string()))?;
            Ok(stats.max_hp as i32)
        });

        methods.add_method("level", |_, this, ()| {
            let stats = tdata_get()
                .get_npc_stats(this.npc().ty)
                .map_err(|e| LuaError::runtime(e.to_string()))?;
            Ok(stats.level)
        });

        methods.add_method("is_dead", |_, this, ()| Ok(this.npc().get_hp() <= 0));

        methods.add_method("has_target", |_, this, ()| {
            Ok(this.npc().target_id.is_some())
        });

        methods.add_method("is_target_alive", |_, this, ()| {
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

        methods.add_method("is_moving", |_, this, ()| Ok(this.npc().path.is_some()));

        // ---- Stats ----

        methods.add_method("walk_speed", |_, this, ()| {
            let stats = tdata_get()
                .get_npc_stats(this.npc().ty)
                .map_err(|e| LuaError::runtime(e.to_string()))?;
            Ok(stats.walk_speed)
        });

        methods.add_method("run_speed", |_, this, ()| {
            let stats = tdata_get()
                .get_npc_stats(this.npc().ty)
                .map_err(|e| LuaError::runtime(e.to_string()))?;
            Ok(stats.run_speed)
        });

        methods.add_method("sight_range", |_, this, ()| {
            let stats = tdata_get()
                .get_npc_stats(this.npc().ty)
                .map_err(|e| LuaError::runtime(e.to_string()))?;
            Ok(stats.sight_range)
        });

        methods.add_method("idle_range", |_, this, ()| {
            let stats = tdata_get()
                .get_npc_stats(this.npc().ty)
                .map_err(|e| LuaError::runtime(e.to_string()))?;
            Ok(stats.idle_range)
        });

        methods.add_method("combat_range", |_, this, ()| {
            let stats = tdata_get()
                .get_npc_stats(this.npc().ty)
                .map_err(|e| LuaError::runtime(e.to_string()))?;
            Ok(stats.combat_range)
        });

        methods.add_method("attack_range", |_, this, ()| {
            let stats = tdata_get()
                .get_npc_stats(this.npc().ty)
                .map_err(|e| LuaError::runtime(e.to_string()))?;
            Ok(stats.attack_range)
        });

        methods.add_method("regen_time", |_, this, ()| {
            let stats = tdata_get()
                .get_npc_stats(this.npc().ty)
                .map_err(|e| LuaError::runtime(e.to_string()))?;
            // Convert from 100ms units to seconds
            Ok(stats.regen_time as f64 / 10.0)
        });

        methods.add_method("delay_time", |_, this, ()| {
            let stats = tdata_get()
                .get_npc_stats(this.npc().ty)
                .map_err(|e| LuaError::runtime(e.to_string()))?;
            // Convert from 100ms units to seconds
            Ok(stats.delay_time as f64 / 10.0)
        });

        // ---- Actions ----

        methods.add_method("clear_target", |_, this, ()| {
            this.npc_mut().target_id = None;
            Ok(())
        });

        methods.add_method("set_target", |_, this, target: LuaEntityID| {
            this.npc_mut().target_id = Some(target.0);
            Ok(())
        });

        methods.add_method("attack", |_, this, ()| {
            let npc = this.npc();
            let target_id = match npc.target_id {
                Some(id) => id,
                None => return Err(LuaError::runtime("No target to attack")),
            };
            let state = this.state_mut();
            skills::do_basic_attack(npc.get_id(), &[target_id], false, state)
                .map_err(|e| LuaError::runtime(e.to_string()))
        });

        methods.add_method(
            "move_to",
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
            },
        );

        methods.add_method("move_toward_target", |_, this, speed: i32| {
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
            let (target_pos, _too_close) =
                target_pos.interpolate(&npc.get_position(), following_distance as f32);
            let mut path = NpcPath::new_single(target_pos, speed);
            path.start();
            npc.tick_movement_along_path(&mut path, state);
            Ok(())
        });

        methods.add_method("stop", |_, this, ()| {
            this.npc_mut().path = None;
            Ok(())
        });

        methods.add_method("begin_death", |_, this, ()| {
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

        methods.add_method("despawn", |_, this, ()| {
            let npc = this.npc();
            let state = this.state_mut();
            state.entity_map.update(npc.get_id(), None, true);
            if npc.summoned {
                state.entity_map.mark_for_cleanup(npc.get_id());
            }
            Ok(())
        });

        methods.add_method("respawn", |_, this, ()| {
            let npc = this.npc_mut();
            let state = this.state_mut();
            npc.reset();
            let chunk_pos = npc.get_chunk_coords();
            state.entity_map.update(npc.get_id(), Some(chunk_pos), true);
            Ok(())
        });

        methods.add_method("set_position", |_, this, (x, y, z): (i32, i32, i32)| {
            this.npc_mut().set_position(Position { x, y, z });
            Ok(())
        });

        methods.add_method(
            "set_spawn_position",
            |_, this, (x, y, z): (i32, i32, i32)| {
                this.npc_mut().spawn_position = Position { x, y, z };
                Ok(())
            },
        );

        // ---- World Queries ----

        methods.add_method("spawn_position", |lua, this, ()| {
            // The spawn position is stored per-NPC in tabledata;
            // for now we expose the current position as a fallback.
            // The actual spawn position will be set when the coroutine is created.
            let npc = this.npc();
            let pos = npc.spawn_position;
            let table = lua.create_table()?;
            table.set("x", pos.x)?;
            table.set("y", pos.y)?;
            table.set("z", pos.z)?;
            Ok(table)
        });

        methods.add_method("distance_to", |_, this, (x, y, z): (i32, i32, i32)| {
            let npc = this.npc();
            let target = Position { x, y, z };
            Ok(npc.get_position().distance_to(&target))
        });

        methods.add_method("distance_to_entity", |_, this, target: LuaEntityID| {
            let npc = this.npc();
            let state = this.state_mut();
            let target = state
                .entity_map
                .get_entity_raw(target.0)
                .ok_or_else(|| LuaError::runtime("Entity not found"))?;
            Ok(npc.get_position().distance_to(&target.get_position()))
        });

        methods.add_method("in_attack_range", |_, this, ()| {
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

        methods.add_method("find_nearest_enemy", |_, this, range: u32| {
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

        methods.add_method("random_point_in_range", |lua, this, range: u32| {
            let npc = this.npc();
            let pos = npc.get_position();
            let target = pos.get_random_around(range, range, 0);
            let table = lua.create_table()?;
            table.set("x", target.x)?;
            table.set("y", target.y)?;
            table.set("z", target.z)?;
            Ok(table)
        });
    }
}
