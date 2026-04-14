use std::{collections::HashMap, fs, path::Path, sync::OnceLock};

use mlua::prelude::*;
use parking_lot::Mutex;
use rand::thread_rng;

use crate::{
    ai::AI,
    config::config_get,
    entity::{Combatant, Entity, EntityID, NPC},
    error::{log, log_error, log_if_failed, FFError, FFResult, Severity},
    helpers,
    path::Path as NpcPath,
    skills,
    state::ShardServerState,
    tabledata::tdata_get,
    Position,
};

static SCRIPTING: OnceLock<Mutex<ScriptingEngine>> = OnceLock::new();

pub fn scripting_init() -> FFResult<()> {
    let scripts_dir = config_get().shard.scripts_path.get();
    let scripts_path = Path::new(&scripts_dir);
    let engine = ScriptingEngine::new(scripts_path)?;
    SCRIPTING.set(Mutex::new(engine)).map_err(|_| {
        FFError::build(
            Severity::Warning,
            "Scripting engine already initialized".to_string(),
        )
    })
}

pub fn scripting_get() -> &'static Mutex<ScriptingEngine> {
    SCRIPTING.get().expect("Scripting engine not initialized")
}

#[derive(Debug, Clone, Copy)]
struct LuaEntityID(EntityID);
impl FromLua for LuaEntityID {
    fn from_lua(value: LuaValue, _: &Lua) -> LuaResult<Self> {
        match value {
            LuaValue::UserData(ud) => {
                let eid = ud.borrow::<Self>()?;
                Ok(*eid)
            }
            _ => Err(LuaError::runtime("expected EntityID")),
        }
    }
}
impl LuaUserData for LuaEntityID {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("is_player", |_, this, ()| {
            Ok(matches!(this.0, EntityID::Player(_)))
        });

        methods.add_method("is_npc", |_, this, ()| {
            Ok(matches!(this.0, EntityID::NPC(_)))
        });

        methods.add_method("is_slider", |_, this, ()| {
            Ok(matches!(this.0, EntityID::Slider(_)))
        });

        methods.add_method("is_egg", |_, this, ()| {
            Ok(matches!(this.0, EntityID::Egg(_)))
        });

        methods.add_meta_method(LuaMetaMethod::Eq, |_, this, other: LuaEntityID| {
            Ok(this.0 == other.0)
        });

        methods.add_meta_method(LuaMetaMethod::ToString, |_, this, ()| {
            Ok(format!("{:?}", this.0))
        });
    }
}

struct NpcCoroutine {
    co_key: LuaRegistryKey,
    wait_ticks: u32,
}

pub struct ScriptingEngine {
    vm: Lua,
    /// script name -> compiled script function
    scripts: HashMap<String, LuaRegistryKey>,
    /// npc_id -> live coroutine
    coroutines: HashMap<i32, NpcCoroutine>,
}
impl ScriptingEngine {
    pub fn new(scripts_dir: &Path) -> FFResult<Self> {
        let vm = Lua::new();

        // Register global yield/wait functions
        Self::register_globals(&vm)?;

        let mut engine = Self {
            vm,
            scripts: HashMap::new(),
            coroutines: HashMap::new(),
        };

        engine.load_scripts(scripts_dir)?;
        Ok(engine)
    }

    fn register_globals(vm: &Lua) -> FFResult<()> {
        // yield() - suspend for exactly one tick (wraps coroutine.yield)
        vm.load(
            r#"
            function yield()
                coroutine.yield(nil)
            end
            function wait(seconds)
                coroutine.yield(seconds)
            end
        "#,
        )
        .exec()
        .map_err(|e| {
            FFError::build(
                Severity::Fatal,
                format!("Failed to register globals: {}", e),
            )
        })?;

        // log(message)
        let log_fn = vm
            .create_function(|_, msg: String| {
                log(Severity::Info, &format!("[Lua] {}", msg));
                Ok(())
            })
            .map_err(|e| {
                FFError::build(
                    Severity::Fatal,
                    format!("Failed to create log function: {}", e),
                )
            })?;

        vm.globals()
            .set("log", log_fn)
            .map_err(|e| FFError::build(Severity::Fatal, format!("Failed to set log: {}", e)))?;

        Ok(())
    }

    fn load_scripts(&mut self, scripts_dir: &Path) -> FFResult<()> {
        let ai_dir = scripts_dir.join("ai");
        if !ai_dir.exists() {
            return Err(FFError::build(
                Severity::Warning,
                format!("AI scripts directory not found: {}", ai_dir.display()),
            ));
        }

        // Load lib modules (available via require("@lib/<name>"))
        let lib_dir = ai_dir.join("lib");
        if lib_dir.exists() {
            self.load_lib_modules(&lib_dir)?;
        }

        // Load base scripts
        self.load_scripts_from_dir(&ai_dir)?;

        // Load custom overrides (these replace base scripts with the same name)
        let custom_dir = ai_dir.join("custom");
        if custom_dir.exists() {
            self.load_scripts_from_dir(&custom_dir)?;
        }

        log(
            Severity::Info,
            &format!("Loaded {} AI script(s)", self.scripts.len()),
        );

        Ok(())
    }

    fn load_lib_modules(&mut self, lib_dir: &Path) -> FFResult<()> {
        let entries = fs::read_dir(lib_dir).map_err(|e| {
            FFError::build(
                Severity::Warning,
                format!("Failed to read {}: {}", lib_dir.display(), e),
            )
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                FFError::build(
                    Severity::Warning,
                    format!("Failed to read dir entry: {}", e),
                )
            })?;

            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("luau") {
                continue;
            }

            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if stem.is_empty() {
                continue;
            }

            let source = fs::read_to_string(&path).map_err(|e| {
                FFError::build(
                    Severity::Warning,
                    format!("Failed to read {}: {}", path.display(), e),
                )
            })?;

            let module_name = format!("@lib/{}", stem);
            let value = self
                .vm
                .load(&source)
                .set_name(&module_name)
                .eval::<LuaValue>()
                .map_err(|e| {
                    FFError::build(
                        Severity::Warning,
                        format!("Failed to load module {}: {}", path.display(), e),
                    )
                })?;

            self.vm.register_module(&module_name, value).map_err(|e| {
                FFError::build(
                    Severity::Warning,
                    format!("Failed to register module {}: {}", module_name, e),
                )
            })?;

            log(
                Severity::Debug,
                &format!("Registered Lua module '{}'", module_name),
            );
        }

        Ok(())
    }

    fn load_scripts_from_dir(&mut self, dir: &Path) -> FFResult<()> {
        let entries = fs::read_dir(dir).map_err(|e| {
            FFError::build(
                Severity::Warning,
                format!("Failed to read {}: {}", dir.display(), e),
            )
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                FFError::build(
                    Severity::Warning,
                    format!("Failed to read dir entry: {}", e),
                )
            })?;

            let path = entry.path();
            if path.is_dir() {
                continue; // skip subdirectories (lib, custom handled separately)
            }

            if path.extension().and_then(|e| e.to_str()) != Some("luau") {
                continue;
            }

            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            if stem.is_empty() {
                continue;
            }

            let source = fs::read_to_string(&path).map_err(|e| {
                FFError::build(
                    Severity::Warning,
                    format!("Failed to read {}: {}", path.display(), e),
                )
            })?;

            let func = self
                .vm
                .load(&source)
                .set_name(&stem)
                .eval::<LuaFunction>()
                .map_err(|e| {
                    FFError::build(
                        Severity::Warning,
                        format!("Failed to compile {}: {}", path.display(), e),
                    )
                })?;

            let key = self.vm.create_registry_value(func).map_err(|e| {
                FFError::build(
                    Severity::Warning,
                    format!("Failed to register {}: {}", path.display(), e),
                )
            })?;

            if self.scripts.contains_key(&stem) {
                log(
                    Severity::Info,
                    &format!("Overriding AI script '{}' with {}", stem, path.display()),
                );
            }

            self.scripts.insert(stem, key);
        }

        Ok(())
    }

    pub fn tick_npc(&mut self, npc: &mut NPC, state: &mut ShardServerState) {
        let npc_id = npc.id;

        // Tick stored path (movement initiated by move_to on previous ticks)
        if let Some(mut path) = npc.path.take() {
            if !path.is_done() {
                npc.tick_movement_along_path(&mut path, state);
            }
            if path.is_done() {
                // Path complete; don't store it back
            } else {
                npc.path = Some(path);
            }
        }

        // Check wait timer
        if let Some(co_state) = self.coroutines.get_mut(&npc_id) {
            if co_state.wait_ticks > 0 {
                co_state.wait_ticks -= 1;
                return;
            }
        }

        // Get or create coroutine
        if !self.coroutines.contains_key(&npc_id) {
            if let Err(e) = self.create_coroutine(npc) {
                log_error(e);
                return;
            }
        }

        let co_state = self.coroutines.get(&npc_id).unwrap();
        let co: LuaThread = match self.vm.registry_value(&co_state.co_key) {
            Ok(co) => co,
            Err(e) => {
                log_error(FFError::build(
                    Severity::Warning,
                    format!("Failed to get coroutine for NPC {}: {}", npc_id, e),
                ));
                self.coroutines.remove(&npc_id);
                return;
            }
        };

        // Create NPC handle for this tick
        let handle = NpcHandle::new(npc, state);

        match co.resume::<LuaValue>(handle) {
            Ok(value) => {
                let co_state = self.coroutines.get_mut(&npc_id).unwrap();
                match value {
                    // wait(seconds) yields with a number
                    LuaValue::Number(seconds) => {
                        let ticks =
                            (seconds * crate::defines::SHARD_TICKS_PER_SECOND as f64).ceil() as u32;
                        co_state.wait_ticks = ticks;
                    }
                    // yield() yields with nil
                    _ => {
                        co_state.wait_ticks = 0;
                    }
                }

                // If coroutine finished, restart it
                if co.status() == LuaThreadStatus::Finished {
                    self.coroutines.remove(&npc_id);
                }
            }
            Err(e) => {
                log_error(FFError::build(
                    Severity::Warning,
                    format!("AI script error for NPC {}: {}", npc_id, e),
                ));
                // Remove broken coroutine; it will be recreated next tick
                self.coroutines.remove(&npc_id);
            }
        }
    }

    fn create_coroutine(&mut self, npc: &NPC) -> FFResult<()> {
        let script_name = AI::get_script_name(npc);

        let script_key = self.scripts.get(script_name).ok_or_else(|| {
            FFError::build(
                Severity::Warning,
                format!("No AI script '{}' (NPC type {})", script_name, npc.ty),
            )
        })?;

        let func: LuaFunction = self.vm.registry_value(script_key).map_err(|e| {
            FFError::build(
                Severity::Warning,
                format!("Failed to get script function: {}", e),
            )
        })?;

        let co = self.vm.create_thread(func).map_err(|e| {
            FFError::build(
                Severity::Warning,
                format!("Failed to create coroutine: {}", e),
            )
        })?;

        let co_key = self.vm.create_registry_value(co).map_err(|e| {
            FFError::build(
                Severity::Warning,
                format!("Failed to register coroutine: {}", e),
            )
        })?;

        self.coroutines.insert(
            npc.id,
            NpcCoroutine {
                co_key,
                wait_ticks: 0,
            },
        );

        Ok(())
    }

    pub fn remove_npc(&mut self, npc_id: i32) {
        self.coroutines.remove(&npc_id);
    }
}

// ==================== NPC Handle (Lua UserData) ====================

/// Lightweight proxy passed to Lua scripts during a single tick.
/// This holds raw pointers because mlua's scope API requires 'static,
/// and we guarantee the references are valid for the duration of resume().
struct NpcHandle {
    npc: *mut NPC,
    state: *mut ShardServerState,
}

// SAFETY: NpcHandle is only used within a single synchronous resume() call.
// The pointers are guaranteed valid for that duration.
unsafe impl Send for NpcHandle {}

impl NpcHandle {
    fn new(npc: &mut NPC, state: &mut ShardServerState) -> Self {
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
