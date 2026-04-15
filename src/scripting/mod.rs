use std::{collections::HashMap, fs, path::Path, sync::OnceLock};

use mlua::prelude::*;
use parking_lot::Mutex;

use crate::{
    defines::SHARD_TICKS_PER_SECOND,
    entity::NPC,
    error::{log, log_error, FFError, FFResult, Severity},
    state::ShardServerState,
    Position,
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
/// build.rs derives parameter types from the closure signature.
macro_rules! luau_method {
    ($methods:ident, $name:literal -> $ret:literal, $($body:tt)+) => {
        $methods.add_method($name, $($body)+);
    };
}

mod entity;
use entity::*;

mod npc;
use npc::*;

luau_type!("Position", "{ x: number, y: number, z: number }");
impl IntoLua for Position {
    fn into_lua(self, lua: &Lua) -> LuaResult<LuaValue> {
        let table = lua.create_table()?;
        table.set("x", self.x)?;
        table.set("y", self.y)?;
        table.set("z", self.z)?;
        Ok(LuaValue::Table(table))
    }
}
impl FromLua for Position {
    fn from_lua(value: LuaValue, _: &Lua) -> LuaResult<Self> {
        match value {
            LuaValue::Table(table) => {
                let x = table.get("x")?;
                let y = table.get("y")?;
                let z = table.get("z")?;
                Ok(Position { x, y, z })
            }
            _ => Err(LuaError::runtime("expected Position")),
        }
    }
}

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

struct NpcCoroutine {
    co_key: LuaRegistryKey,
    wait_ticks: u32,
}

pub struct ScriptingEngine {
    vm: Lua,
    scripts: HashMap<String, LuaRegistryKey>,
    coroutines: HashMap<i32, NpcCoroutine>,
}
impl ScriptingEngine {
    pub fn new() -> FFResult<Self> {
        let vm = Lua::new();
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
        luau_function!("wait", "(seconds: number, predicate: (() -> boolean)?): ()");
        luau_function!("log", "(message: string): ()");
        luau_function!(
            "random_point_in_range",
            "(from: Position, range: number): Position"
        );

        vm.load(format!(
            r#"
            local TICKS_PER_SECOND = {}
            function yield()
                coroutine.yield(nil)
            end
            function wait(seconds, predicate)
                if predicate then
                    local ticks = math.ceil(seconds * TICKS_PER_SECOND)
                    for i = 1, ticks do
                        if predicate() then return end
                        coroutine.yield(nil)
                    end
                else
                    coroutine.yield(seconds)
                end
            end
        "#,
            SHARD_TICKS_PER_SECOND
        ))
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
            .unwrap();

        // random_point_in_range(from, range)
        let rand_pt_fn = vm
            .create_function(|_, (from, range): (Position, f64)| {
                let new_pos = from.get_random_around(range as u32, range as u32, 0);
                Ok(new_pos)
            })
            .unwrap();

        vm.globals().set("log", log_fn).unwrap();
        vm.globals()
            .set("random_point_in_range", rand_pt_fn)
            .unwrap();

        Ok(())
    }

    fn load_scripts(&mut self) -> FFResult<()> {
        const SCRIPTS_PATH: &str = "scripts";

        let scripts_path = Path::new(SCRIPTS_PATH);

        // Load lib modules (available via require("@lib/<name>"))
        let lib_dir = scripts_path.join("lib");
        if lib_dir.exists() {
            self.load_lib_modules(&lib_dir).map_err(|e| {
                FFError::build(
                    Severity::Warning,
                    format!("Failed to load Lua lib modules from {}", lib_dir.display()),
                )
                .with_parent(e)
            })?;
        }

        // Load AI scripts
        let ai_dir = scripts_path.join("ai");
        if ai_dir.exists() {
            self.load_scripts_from_dir(&ai_dir).map_err(|e| {
                FFError::build(
                    Severity::Warning,
                    format!("Failed to load AI scripts from {}", ai_dir.display()),
                )
                .with_parent(e)
            })?;
        }

        log(
            Severity::Info,
            &format!("Loaded {} AI script(s)", self.scripts.len()),
        );

        Ok(())
    }

    fn load_lib_module(&mut self, path: &Path) -> FFResult<()> {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        if stem.is_empty() {
            return Ok(());
        }

        let source = fs::read_to_string(path)?;
        let module_name = format!("@lib/{}", stem);
        let value = self
            .vm
            .load(&source)
            .set_name(&module_name)
            .eval::<LuaValue>()
            .map_err(|e| {
                FFError::build(
                    Severity::Warning,
                    format!("Failed to load module '{}'", module_name),
                )
                .with_parent(e.into())
            })?;

        self.vm.register_module(&module_name, value).map_err(|e| {
            FFError::build(
                Severity::Warning,
                format!("Failed to register module '{}'", module_name),
            )
            .with_parent(e.into())
        })?;

        log(
            Severity::Info,
            &format!(
                "Registered Lua module '{}' from {}",
                module_name,
                path.display()
            ),
        );

        Ok(())
    }

    fn load_lib_modules(&mut self, lib_dir: &Path) -> FFResult<()> {
        let mut pending: Vec<_> = fs::read_dir(lib_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("luau"))
            .collect();

        let mut retry = Vec::with_capacity(pending.len());

        // Retry loop: modules may depend on each other, so keep retrying
        // until either all succeed or no progress is made.
        loop {
            let mut failed = Vec::new();
            let prev_count = pending.len();

            for path in pending.drain(..) {
                if let Err(e) = self.load_lib_module(&path) {
                    // Probably a dependency not yet loaded; retry later
                    failed.push(e);
                    retry.push(path);
                }
            }

            if failed.is_empty() {
                break;
            }

            if failed.len() == prev_count {
                // No progress — report the first failure
                let e = failed.remove(0);
                return Err(e);
            }

            pending.append(&mut retry);
        }

        Ok(())
    }

    fn load_scripts_from_dir(&mut self, dir: &Path) -> FFResult<()> {
        let entries = fs::read_dir(dir)?;
        for entry in entries {
            let Ok(entry) = entry else {
                continue;
            };

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

            let source = fs::read_to_string(&path)?;

            let func = self
                .vm
                .load(&source)
                .set_name(&stem)
                .eval::<LuaFunction>()
                .map_err(FFError::from)?;

            let key = self.vm.create_registry_value(func).map_err(FFError::from)?;

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

        // Check wait timer (skip if dead unless uninterruptible)
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
                log_error(FFError::from(e));
                self.coroutines.remove(&npc_id);
                return;
            }
        };

        let status = co.status();
        if status == LuaThreadStatus::Finished || status == LuaThreadStatus::Error {
            npc.ai = None;
            return;
        }

        let handle = NpcScriptContext::new(npc, state);
        match co.resume::<LuaValue>(handle) {
            Ok(value) => {
                let co_state = self.coroutines.get_mut(&npc_id).unwrap();
                let wait_seconds = match &value {
                    LuaValue::Number(n) => Some(*n),
                    LuaValue::Integer(n) => Some(*n as f64),
                    LuaValue::Nil => None, // yield()
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
                    let ticks = (seconds * SHARD_TICKS_PER_SECOND as f64).ceil() as u32;
                    co_state.wait_ticks = ticks;
                }
            }
            Err(e) => {
                log_error(
                    FFError::build(
                        Severity::Warning,
                        format!("AI script error for NPC {}", npc),
                    )
                    .with_parent(e.into()),
                );
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
                format!("Failed to get script function for NPC {}", npc),
            )
            .with_parent(e.into())
        })?;

        let co = self.vm.create_thread(func).map_err(|e| {
            FFError::build(
                Severity::Warning,
                format!("Failed to create coroutine for NPC {}", npc),
            )
            .with_parent(e.into())
        })?;

        let co_key = self.vm.create_registry_value(co).map_err(|e| {
            FFError::build(
                Severity::Warning,
                format!("Failed to register coroutine for NPC {}", npc),
            )
            .with_parent(e.into())
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
