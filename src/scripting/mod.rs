use std::{collections::HashMap, fs, path::Path, sync::OnceLock};

use mlua::prelude::*;
use parking_lot::Mutex;

use crate::{
    entity::{Combatant as _, EntityID, NPC},
    error::{log, log_error, FFError, FFResult, Severity},
    state::ShardServerState,
};

/// Emits `export type <Name> = <Definition>` in `scripts/globals.d.luau`.
macro_rules! luau_type {
    ($name:literal, $def:literal) => {};
}

/// Emits `declare function <name>(<params>): <ret>` in `scripts/globals.d.luau`.
macro_rules! luau_function {
    ($name:literal, $sig:literal) => {};
}

/// Marks a block of `luau_method!` calls as belonging to a Luau class.
/// build.rs emits `declare class <Name>` ... `end` around the methods.
macro_rules! luau_class {
    ($class:literal, { $($body:tt)* }) => {
        $($body)*
    };
}

/// Registers a Lua method and encodes its Luau return type for build.rs.
/// build.rs scans for invocations to auto-generate `scripts/globals.d.luau`,
/// deriving parameter types from the closure signature.
macro_rules! luau_method {
    ($methods:ident, $name:literal -> $ret:literal, $($body:tt)+) => {
        $methods.add_method($name, $($body)+);
    };
}

mod npc;
use npc::*;

luau_type!("Position", "{ x: number, y: number, z: number }");

static SCRIPTING: OnceLock<Mutex<ScriptingEngine>> = OnceLock::new();

pub fn scripting_init() -> FFResult<()> {
    let engine = ScriptingEngine::new()?;
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
            _ => Err(LuaError::runtime("expected Entity")),
        }
    }
}
impl LuaUserData for LuaEntityID {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        luau_class!("Entity", {
            luau_method!(methods, "is_player" -> "boolean", |_, this, ()| {
                Ok(matches!(this.0, EntityID::Player(_)))
            });

            luau_method!(methods, "is_npc" -> "boolean", |_, this, ()| {
                Ok(matches!(this.0, EntityID::NPC(_)))
            });

            luau_method!(methods, "is_slider" -> "boolean", |_, this, ()| {
                Ok(matches!(this.0, EntityID::Slider(_)))
            });

            luau_method!(methods, "is_egg" -> "boolean", |_, this, ()| {
                Ok(matches!(this.0, EntityID::Egg(_)))
            });
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
    pub fn new() -> FFResult<Self> {
        let vm = Lua::new();

        // Register global yield/wait functions
        Self::register_globals(&vm)?;

        let mut engine = Self {
            vm,
            scripts: HashMap::new(),
            coroutines: HashMap::new(),
        };

        engine.load_scripts()?;
        Ok(engine)
    }

    pub fn reload(&mut self) -> FFResult<()> {
        self.scripts.clear();
        self.coroutines.clear();
        self.vm.expire_registry_values();
        self.load_scripts()
    }

    pub fn get_script_count(&self) -> usize {
        self.scripts.len()
    }

    pub fn get_coroutine_count(&self) -> usize {
        self.coroutines.len()
    }

    fn register_globals(vm: &Lua) -> FFResult<()> {
        luau_function!("yield", "(): ()");
        luau_function!("wait", "(seconds: number): ()");
        luau_function!("log", "(message: string): ()");

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

    fn load_scripts(&mut self) -> FFResult<()> {
        const SCRIPTS_PATH: &str = "scripts";
        let scripts_path = Path::new(SCRIPTS_PATH);

        let ai_dir = scripts_path.join("ai");
        if !ai_dir.exists() {
            return Err(FFError::build(
                Severity::Warning,
                format!("AI scripts directory not found: {}", ai_dir.display()),
            ));
        }

        // Load lib modules (available via require("@lib/<name>"))
        let lib_dir = scripts_path.join("lib");
        if lib_dir.exists() {
            self.load_lib_modules(&lib_dir)?;
        }

        // Load AI scripts
        self.load_scripts_from_dir(&ai_dir)?;

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
                Severity::Info,
                &format!(
                    "Registered Lua module '{}' from {}",
                    module_name,
                    path.display()
                ),
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
            } else {
                log(
                    Severity::Info,
                    &format!("Loaded AI script '{}' from {}", stem, path.display()),
                );
            }

            self.scripts.insert(stem, key);
        }

        Ok(())
    }

    pub fn tick_npc(&mut self, npc: &mut NPC, state: &mut ShardServerState) {
        let npc_id = npc.id;

        // Check wait timer (skip if dead so death handling runs immediately)
        if let Some(co_state) = self.coroutines.get_mut(&npc_id) {
            if co_state.wait_ticks > 0 && !npc.is_dead() {
                co_state.wait_ticks -= 1;
                return;
            }
            co_state.wait_ticks = 0;
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
                // Extract wait duration from yield value
                let wait_seconds = match &value {
                    LuaValue::Number(n) => Some(*n),
                    LuaValue::Integer(n) => Some(*n as f64),
                    LuaValue::Nil => None, // yield() with no wait
                    other => {
                        log(
                            Severity::Warning,
                            &format!(
                                "Unexpected yield value from NPC {} coroutine: {:?}",
                                npc_id, other
                            ),
                        );
                        None
                    }
                };

                if let Some(seconds) = wait_seconds {
                    let ticks =
                        (seconds * crate::defines::SHARD_TICKS_PER_SECOND as f64).ceil() as u32;
                    co_state.wait_ticks = ticks;
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
        let Some(script_name) = &npc.ai else {
            return Err(FFError::build(
                Severity::Warning,
                format!("NPC {} has no AI script", npc),
            ));
        };

        let script_key = self.scripts.get(script_name).ok_or_else(|| {
            FFError::build(
                Severity::Warning,
                format!(
                    "No AI script '{}' exists (assigned to NPC {})",
                    script_name, npc
                ),
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
