//! Juni scripting for Kerabit games (3.0 host API).
//!
//! Scripts are `.juni` files compiled in-process (via `juni-driver`) against
//! the Kerabit **prelude** ([`PRELUDE`], `juni/kerabit.juni`) and run in an
//! embedded WASM runtime. `fn main() -> i32` runs once at load; `fn frame(dt)`
//! runs every frame after the Rust `run` closure. Persist values in a
//! `state:` block.
//!
//! The host exposes entities as opaque `i32` handles. Reads come from a
//! per-frame snapshot of the [`World`]; writes are queued as [`WorldOp`]s and
//! [`ScriptEffects`] and applied by the engine after every script has run, so
//! scripts never hold references into engine state.

mod host;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub use juni_check::diag::{Diagnostic, Severity};
use juni_driver::{check_single_with_prelude, compile_single_with_prelude, PreludeSource};
use kerabit_input::{InputState, Key, MouseButton};
use kerabit_math::Vec3;
use kerabit_world::World;
use wasmtime::{Engine, Instance, Linker, Module, Store, TypedFunc};

/// Source of the Kerabit prelude module (`juni/kerabit.juni`): the frozen 3.0
/// host table plus small helpers, auto-imported into every script.
pub const PRELUDE: &str = include_str!("../juni/kerabit.juni");

/// Logical module name of the prelude (also `import kerabit` in scripts).
pub const PRELUDE_NAME: &str = "kerabit";

/// Fuel budget per script call; exhausting it traps instead of hanging a frame.
const FUEL_PER_CALL: u64 = 20_000_000;

/// Errors from compiling, linking, or running a Juni script.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Type / syntax errors, already formatted as `file:line:col: error: msg` lines.
    #[error("{0}")]
    Compile(String),
    /// The script uses a Juno browser builtin Kerabit does not provide.
    #[error("{0}")]
    Unsupported(String),
    /// WASM instantiation failed (unexpected import / memory shape).
    #[error("link error: {0}")]
    Link(String),
    /// A trap while running `main` / `frame` (out of fuel, unreachable, ...).
    #[error("{0}")]
    Runtime(String),
}

/// Cube / plane spawn requested by a script (applied by `Context`).
#[derive(Clone, Debug)]
pub struct SpawnPrimitive {
    pub name: String,
    pub kind: PrimitiveKind,
    pub color: [f32; 3],
    pub at: [f32; 3],
    pub scale: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveKind {
    Cube,
    Plane,
}

/// Prefab spawn request.
#[derive(Clone, Debug)]
pub struct SpawnPrefab {
    pub path: String,
    pub offset: [f32; 3],
}

/// Planar kinematic move (XZ delta this frame).
#[derive(Clone, Debug)]
pub struct MovePlanar {
    pub name: String,
    pub dx: f32,
    pub dz: f32,
}

/// Audio one-shot.
#[derive(Clone, Debug)]
pub struct PlaySound {
    pub path: String,
    pub at: Option<[f32; 3]>,
}

/// Particle burst.
#[derive(Clone, Debug)]
pub struct ParticleCmd {
    pub origin: [f32; 3],
    pub count: u32,
    pub color: [f32; 3],
}

/// Camera look-at.
#[derive(Clone, Debug)]
pub struct CameraCmd {
    pub eye: [f32; 3],
    pub target: [f32; 3],
}

/// Overlay rect / text (ASCII text only).
#[derive(Clone, Debug)]
pub struct UiRectCmd {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct UiTextCmd {
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub color: [f32; 3],
    pub text: String,
}

/// Side effects that need the full Kerabit `Context` (GPU / audio / physics / UI).
#[derive(Default, Debug)]
pub struct ScriptEffects {
    pub despawns: Vec<String>,
    pub spawns: Vec<SpawnPrimitive>,
    pub prefabs: Vec<SpawnPrefab>,
    pub moves: Vec<MovePlanar>,
    pub plays: Vec<PlaySound>,
    pub particles: Vec<ParticleCmd>,
    pub camera: Option<CameraCmd>,
    pub ui_rects: Vec<UiRectCmd>,
    pub ui_texts: Vec<UiTextCmd>,
    pub register_boxes: Vec<String>,
    /// `print(...)` / `log(...)` output, one entry per call.
    pub logs: Vec<String>,
    pub reload: bool,
}

/// World mutation queued by a script; applied to the [`World`] after all
/// scripts ran this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum WorldOp {
    Quit,
    RotateX { name: String, rad: f32 },
    RotateY { name: String, rad: f32 },
    RotateZ { name: String, rad: f32 },
    Translate { name: String, x: f32, y: f32, z: f32 },
    SetPos { name: String, x: f32, y: f32, z: f32 },
    SetScale { name: String, x: f32, y: f32, z: f32 },
    SetEnabled { name: String, enabled: bool },
    AddTag { name: String, tag: String },
    RemoveTag { name: String, tag: String },
}

/// Per-frame host snapshot + queued output, shared by every script.
#[derive(Default)]
pub(crate) struct Host {
    dt: f32,
    keys_down: HashSet<Key>,
    keys_pressed: HashSet<Key>,
    mouse_x: f32,
    mouse_y: f32,
    mouse_down: HashSet<MouseButton>,
    mouse_pressed: HashSet<MouseButton>,
    known_names: HashSet<String>,
    tag_index: HashMap<String, Vec<String>>,
    positions: HashMap<String, [f32; 3]>,
    scales: HashMap<String, [f32; 3]>,
    enabled: HashMap<String, bool>,
    tags: HashMap<String, HashSet<String>>,
    /// Handle table: `handles[h - 1]` is the entity name for handle `h`.
    handles: Vec<String>,
    handle_of: HashMap<String, i32>,
    world_ops: Vec<WorldOp>,
    effects: ScriptEffects,
}

impl Host {
    /// Intern `name` and return its stable handle (never 0).
    fn intern(&mut self, name: &str) -> i32 {
        if let Some(&h) = self.handle_of.get(name) {
            return h;
        }
        self.handles.push(name.to_string());
        let h = self.handles.len() as i32;
        self.handle_of.insert(name.to_string(), h);
        h
    }

    fn name_of(&self, handle: i32) -> Option<&str> {
        if handle <= 0 {
            return None;
        }
        self.handles.get(handle as usize - 1).map(String::as_str)
    }
}

/// Store data for one script instance.
pub(crate) struct ScriptData {
    /// Moved in for the duration of a call, then taken back by the runtime.
    host: Host,
    self_entity: Option<String>,
    memory: Option<wasmtime::Memory>,
    script_label: String,
}

struct LoadedScript {
    path: PathBuf,
    entity: Option<String>,
    store: Store<ScriptData>,
    main: Option<TypedFunc<(), i32>>,
    frame: Option<TypedFunc<f32, i32>>,
    inited: bool,
    /// Trap message; while set the script is skipped (until reloaded).
    error: Option<String>,
    mtime: Option<SystemTime>,
}

/// Compiled scripts + WASM engine. Tick once per frame against a live world.
pub struct ScriptRuntime {
    engine: Engine,
    linker: Linker<ScriptData>,
    host: Host,
    scripts: Vec<LoadedScript>,
    /// Compile error from the last hot reload; cleared when the file compiles again.
    reload_error: Option<String>,
    base_dir: Option<PathBuf>,
    auto_reload: bool,
}

impl Default for ScriptRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptRuntime {
    pub fn new() -> Self {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config).expect("wasmtime engine");
        let mut linker = Linker::new(&engine);
        host::link(&mut linker).expect("kerabit host linker");
        Self {
            engine,
            linker,
            host: Host::default(),
            scripts: Vec::new(),
            reload_error: None,
            base_dir: None,
            auto_reload: true,
        }
    }

    /// Directory used to resolve relative script / prefab / audio paths.
    pub fn base_dir(&self) -> Option<&Path> {
        self.base_dir.as_deref()
    }

    pub fn set_base_dir(&mut self, dir: Option<PathBuf>) {
        self.base_dir = dir;
    }

    /// When true (default), recompile scripts whose file mtime changed.
    pub fn set_auto_reload(&mut self, enabled: bool) {
        self.auto_reload = enabled;
    }

    /// Type-check `source` against the Kerabit prelude (no runtime needed).
    pub fn check_source(source: &str) -> Result<(), ScriptError> {
        let diags = Self::check_source_diagnostics(source, None);
        if diags.iter().any(|d| d.severity == Severity::Error) {
            Err(ScriptError::Compile(format_diagnostics(&diags, "<script>")))
        } else {
            Ok(())
        }
    }

    /// Structured diagnostics (errors + warnings) for editors and tools.
    pub fn check_source_diagnostics(source: &str, file: Option<&str>) -> Vec<Diagnostic> {
        match check_single_with_prelude("main", file, source, &[prelude()]) {
            Ok(warnings) => warnings,
            Err(diags) => diags,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty()
    }

    /// Current script problem, if any: a hot-reload compile error, else the
    /// first trapped script's message. Stays set until the file is reloaded
    /// so games can keep it on screen.
    pub fn last_error(&self) -> Option<&str> {
        self.reload_error
            .as_deref()
            .or_else(|| self.scripts.iter().find_map(|s| s.error.as_deref()))
    }

    pub fn clear(&mut self) {
        self.scripts.clear();
        self.reload_error = None;
        self.host.handles.clear();
        self.host.handle_of.clear();
    }

    /// Compile and keep `path`. `entity` binds `self_entity()` to that name.
    pub fn load_file(
        &mut self,
        path: impl AsRef<Path>,
        entity: Option<String>,
    ) -> Result<(), ScriptError> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path)?;
        let mtime = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
        let script = self.compile(path, &source, entity, mtime)?;
        self.scripts.push(script);
        Ok(())
    }

    /// Compile `source` as if it came from `name` (used in tests / editor).
    pub fn load_source(
        &mut self,
        name: impl AsRef<Path>,
        source: &str,
        entity: Option<String>,
    ) -> Result<(), ScriptError> {
        let script = self.compile(name.as_ref(), source, entity, None)?;
        self.scripts.push(script);
        Ok(())
    }

    fn compile(
        &mut self,
        path: &Path,
        source: &str,
        entity: Option<String>,
        mtime: Option<SystemTime>,
    ) -> Result<LoadedScript, ScriptError> {
        let label = path.display().to_string();
        let out = compile_single_with_prelude("main", Some(&label), source, &[prelude()])
            .map_err(|diags| ScriptError::Compile(format_diagnostics(&diags, &label)))?;
        if let Some(unsupported) = out
            .builtins
            .iter()
            .find(|b| !host::supports_builtin(b))
        {
            return Err(ScriptError::Unsupported(format!(
                "{label}: `{unsupported}` is a Juno browser builtin and is not available in Kerabit; use the kerabit host API (see API.md)"
            )));
        }

        let module = Module::new(&self.engine, &out.wasm)
            .map_err(|e| ScriptError::Link(format!("{label}: {e:#}")))?;
        let mut store = Store::new(
            &self.engine,
            ScriptData {
                host: Host::default(),
                self_entity: entity.clone(),
                memory: None,
                script_label: label.clone(),
            },
        );
        store
            .set_fuel(FUEL_PER_CALL)
            .map_err(|e| ScriptError::Link(format!("{label}: {e:#}")))?;
        let instance: Instance = self
            .linker
            .instantiate(&mut store, &module)
            .map_err(|e| ScriptError::Link(format!("{label}: {e:#}")))?;
        let memory = instance.get_memory(&mut store, "memory");
        store.data_mut().memory = memory;
        let main = instance.get_typed_func::<(), i32>(&mut store, "main").ok();
        let frame = instance.get_typed_func::<f32, i32>(&mut store, "frame").ok();

        Ok(LoadedScript {
            path: path.to_path_buf(),
            entity,
            store,
            main,
            frame,
            inited: false,
            error: None,
            mtime,
        })
    }

    /// Re-read every loaded script file from disk (editor Reload).
    pub fn reload_from_disk(&mut self) -> Result<(), ScriptError> {
        let snapshot: Vec<(PathBuf, Option<String>)> = self
            .scripts
            .iter()
            .map(|s| (s.path.clone(), s.entity.clone()))
            .collect();
        self.scripts.clear();
        self.reload_error = None;
        for (path, entity) in snapshot {
            self.load_file(path, entity)?;
        }
        Ok(())
    }

    fn maybe_hot_reload(&mut self) {
        if !self.auto_reload {
            return;
        }
        for i in 0..self.scripts.len() {
            let path = self.scripts[i].path.clone();
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            let Ok(mtime) = meta.modified() else {
                continue;
            };
            if self.scripts[i].mtime == Some(mtime) {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            let entity = self.scripts[i].entity.clone();
            match self.compile(&path, &source, entity, Some(mtime)) {
                Ok(script) => {
                    self.scripts[i] = script;
                    self.reload_error = None;
                }
                Err(err) => {
                    // Keep the old instance running; remember the mtime so the
                    // file is only recompiled after the next save.
                    self.scripts[i].mtime = Some(mtime);
                    self.reload_error = Some(err.to_string());
                }
            }
        }
    }

    /// Run every loaded script; apply world ops; return context-level effects.
    pub fn tick(
        &mut self,
        dt: f32,
        input: &InputState,
        world: &mut World,
        quit: &mut bool,
    ) -> ScriptEffects {
        if self.scripts.is_empty() {
            return ScriptEffects::default();
        }

        self.maybe_hot_reload();

        self.host.dt = dt;
        self.host.world_ops.clear();
        self.host.effects = ScriptEffects::default();
        fill_keys(&mut self.host, input);
        fill_mouse(&mut self.host, input);
        fill_world(&mut self.host, world);

        for script in &mut self.scripts {
            if script.error.is_some() {
                continue;
            }
            // Hand the shared host to this script's store for the call.
            script.store.data_mut().host = std::mem::take(&mut self.host);
            let result = run_script(script);
            self.host = std::mem::take(&mut script.store.data_mut().host);
            if let Err(err) = result {
                script.error = Some(err.to_string());
            }
        }

        let world_ops = std::mem::take(&mut self.host.world_ops);
        let mut effects = std::mem::take(&mut self.host.effects);
        apply_world_ops(world, quit, world_ops);

        if effects.reload {
            if let Err(err) = self.reload_from_disk() {
                self.reload_error = Some(err.to_string());
            }
            effects.reload = false;
        }

        effects
    }
}

fn run_script(script: &mut LoadedScript) -> Result<(), ScriptError> {
    let label = script.store.data().script_label.clone();
    let dt = script.store.data().host.dt;
    if !script.inited {
        script.inited = true;
        if let Some(main) = script.main.clone() {
            refuel(&mut script.store, &label)?;
            main.call(&mut script.store, ())
                .map_err(|e| ScriptError::Runtime(format!("{label}: main(): {}", trap_message(&e))))?;
        }
    }
    if let Some(frame) = script.frame.clone() {
        refuel(&mut script.store, &label)?;
        frame
            .call(&mut script.store, dt)
            .map_err(|e| ScriptError::Runtime(format!("{label}: frame(): {}", trap_message(&e))))?;
    }
    Ok(())
}

fn refuel(store: &mut Store<ScriptData>, label: &str) -> Result<(), ScriptError> {
    store
        .set_fuel(FUEL_PER_CALL)
        .map_err(|e| ScriptError::Runtime(format!("{label}: {e:#}")))
}

fn trap_message(err: &wasmtime::Error) -> String {
    match err.downcast_ref::<wasmtime::Trap>() {
        Some(wasmtime::Trap::OutOfFuel) => {
            "script ran too long this frame (infinite loop?)".to_string()
        }
        Some(wasmtime::Trap::UnreachableCodeReached) => {
            "runtime check failed (array index or str_substr out of bounds)".to_string()
        }
        Some(trap) => format!("{trap}"),
        None => format!("{err:#}"),
    }
}

fn prelude() -> PreludeSource<'static> {
    PreludeSource {
        name: PRELUDE_NAME,
        source: PRELUDE,
    }
}

/// `file:line:col: severity: message` per diagnostic, one per line.
pub fn format_diagnostics(diags: &[Diagnostic], fallback_file: &str) -> String {
    diags
        .iter()
        .map(|d| d.format(fallback_file))
        .collect::<Vec<_>>()
        .join("\n")
}

fn fill_keys(host: &mut Host, input: &InputState) {
    host.keys_down.clear();
    host.keys_pressed.clear();
    for &k in ALL_KEYS {
        if input.key_down(k) {
            host.keys_down.insert(k);
        }
        if input.key_pressed(k) {
            host.keys_pressed.insert(k);
        }
    }
}

fn fill_mouse(host: &mut Host, input: &InputState) {
    let (x, y) = input.mouse_position();
    host.mouse_x = x;
    host.mouse_y = y;
    host.mouse_down.clear();
    host.mouse_pressed.clear();
    for &b in &[MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
        if input.mouse_button_down(b) {
            host.mouse_down.insert(b);
        }
        if input.mouse_button_pressed(b) {
            host.mouse_pressed.insert(b);
        }
    }
}

fn fill_world(host: &mut Host, world: &World) {
    host.known_names.clear();
    host.tag_index.clear();
    host.positions.clear();
    host.scales.clear();
    host.enabled.clear();
    host.tags.clear();
    for entity in world.iter() {
        let Some(name) = entity.name() else {
            continue;
        };
        let name = name.to_string();
        host.known_names.insert(name.clone());
        let t = entity.transform.translation();
        host.positions.insert(name.clone(), [t.x, t.y, t.z]);
        let s = entity.transform.scale();
        host.scales.insert(name.clone(), [s.x, s.y, s.z]);
        host.enabled.insert(name.clone(), entity.is_enabled());
        let mut tag_set = HashSet::new();
        for tag in entity.tags() {
            tag_set.insert(tag.clone());
            host.tag_index
                .entry(tag.clone())
                .or_default()
                .push(name.clone());
        }
        host.tags.insert(name, tag_set);
    }
    // Stable order so `tag_at(tag, i)` is deterministic across frames.
    for names in host.tag_index.values_mut() {
        names.sort();
    }
}

fn apply_world_ops(world: &mut World, quit: &mut bool, ops: Vec<WorldOp>) {
    for op in ops {
        match op {
            WorldOp::Quit => *quit = true,
            WorldOp::RotateX { name, rad } => {
                if let Some(e) = world.get_mut(&name) {
                    e.transform.rotate_x(rad);
                }
            }
            WorldOp::RotateY { name, rad } => {
                if let Some(e) = world.get_mut(&name) {
                    e.rotate_y(rad);
                }
            }
            WorldOp::RotateZ { name, rad } => {
                if let Some(e) = world.get_mut(&name) {
                    e.transform.rotate_z(rad);
                }
            }
            WorldOp::Translate { name, x, y, z } => {
                if let Some(e) = world.get_mut(&name) {
                    e.transform.translate(Vec3::new(x, y, z));
                }
            }
            WorldOp::SetPos { name, x, y, z } => {
                if let Some(e) = world.get_mut(&name) {
                    e.transform.set_translation(Vec3::new(x, y, z));
                }
            }
            WorldOp::SetScale { name, x, y, z } => {
                if let Some(e) = world.get_mut(&name) {
                    e.transform.set_scale(Vec3::new(x, y, z));
                }
            }
            WorldOp::SetEnabled { name, enabled } => {
                if let Some(e) = world.get_mut(&name) {
                    e.set_enabled(enabled);
                }
            }
            WorldOp::AddTag { name, tag } => {
                if let Some(e) = world.get_mut(&name) {
                    e.add_tag(tag);
                }
            }
            WorldOp::RemoveTag { name, tag } => {
                if let Some(e) = world.get_mut(&name) {
                    e.remove_tag(&tag);
                }
            }
        }
    }
}

pub(crate) fn parse_mouse(name: &str) -> Option<MouseButton> {
    match name.trim().to_ascii_lowercase().as_str() {
        "left" | "lmb" => Some(MouseButton::Left),
        "right" | "rmb" => Some(MouseButton::Right),
        "middle" | "mmb" => Some(MouseButton::Middle),
        _ => None,
    }
}

pub(crate) fn parse_key(name: &str) -> Option<Key> {
    match name.trim().to_ascii_lowercase().as_str() {
        "escape" | "esc" => Some(Key::Escape),
        "space" => Some(Key::Space),
        "enter" | "return" => Some(Key::Enter),
        "tab" => Some(Key::Tab),
        "backspace" => Some(Key::Backspace),
        "shift" => Some(Key::Shift),
        "control" | "ctrl" => Some(Key::Control),
        "alt" => Some(Key::Alt),
        "left" => Some(Key::Left),
        "right" => Some(Key::Right),
        "up" => Some(Key::Up),
        "down" => Some(Key::Down),
        "a" => Some(Key::A),
        "b" => Some(Key::B),
        "c" => Some(Key::C),
        "d" => Some(Key::D),
        "e" => Some(Key::E),
        "f" => Some(Key::F),
        "g" => Some(Key::G),
        "h" => Some(Key::H),
        "i" => Some(Key::I),
        "j" => Some(Key::J),
        "k" => Some(Key::K),
        "l" => Some(Key::L),
        "m" => Some(Key::M),
        "n" => Some(Key::N),
        "o" => Some(Key::O),
        "p" => Some(Key::P),
        "q" => Some(Key::Q),
        "r" => Some(Key::R),
        "s" => Some(Key::S),
        "t" => Some(Key::T),
        "u" => Some(Key::U),
        "v" => Some(Key::V),
        "w" => Some(Key::W),
        "x" => Some(Key::X),
        "y" => Some(Key::Y),
        "z" => Some(Key::Z),
        "0" | "digit0" => Some(Key::Digit0),
        "1" | "digit1" => Some(Key::Digit1),
        "2" | "digit2" => Some(Key::Digit2),
        "3" | "digit3" => Some(Key::Digit3),
        "4" | "digit4" => Some(Key::Digit4),
        "5" | "digit5" => Some(Key::Digit5),
        "6" | "digit6" => Some(Key::Digit6),
        "7" | "digit7" => Some(Key::Digit7),
        "8" | "digit8" => Some(Key::Digit8),
        "9" | "digit9" => Some(Key::Digit9),
        _ => None,
    }
}

const ALL_KEYS: &[Key] = &[
    Key::Escape,
    Key::Space,
    Key::Enter,
    Key::Tab,
    Key::Backspace,
    Key::Shift,
    Key::Control,
    Key::Alt,
    Key::Left,
    Key::Right,
    Key::Up,
    Key::Down,
    Key::A,
    Key::B,
    Key::C,
    Key::D,
    Key::E,
    Key::F,
    Key::G,
    Key::H,
    Key::I,
    Key::J,
    Key::K,
    Key::L,
    Key::M,
    Key::N,
    Key::O,
    Key::P,
    Key::Q,
    Key::R,
    Key::S,
    Key::T,
    Key::U,
    Key::V,
    Key::W,
    Key::X,
    Key::Y,
    Key::Z,
    Key::Digit0,
    Key::Digit1,
    Key::Digit2,
    Key::Digit3,
    Key::Digit4,
    Key::Digit5,
    Key::Digit6,
    Key::Digit7,
    Key::Digit8,
    Key::Digit9,
];

#[cfg(test)]
mod tests {
    use kerabit_world::Transform;

    use super::*;

    fn tick(rt: &mut ScriptRuntime, world: &mut World, input: &InputState) -> (ScriptEffects, bool) {
        let mut quit = false;
        let fx = rt.tick(0.016, input, world, &mut quit);
        (fx, quit)
    }

    #[test]
    fn prelude_itself_checks() {
        let diags = ScriptRuntime::check_source_diagnostics(
            "fn main() -> i32:\n    return 0\n",
            Some("empty.juni"),
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn check_source_rejects_garbage_and_accepts_host_calls() {
        assert!(ScriptRuntime::check_source("rotate_y(").is_err());
        assert!(ScriptRuntime::check_source(
            "fn frame(dt: f32) -> i32:\n    rotate_y(entity(\"cube\"), 1.0)\n    return 0\n"
        )
        .is_ok());
        let err = ScriptRuntime::check_source(
            "fn frame(dt: f32) -> i32:\n    set_pos(entity(\"cube\"), 1.0)\n    return 0\n",
        )
        .unwrap_err();
        assert!(err.to_string().contains("expects 4 args, got 2"), "{err}");
    }

    #[test]
    fn browser_builtins_are_rejected_with_guidance() {
        let mut rt = ScriptRuntime::new();
        let err = rt
            .load_source(
                "t.juni",
                "fn main() -> i32:\n    canvas_init(320, 200)\n    return 0\n",
                None,
            )
            .unwrap_err();
        assert!(matches!(err, ScriptError::Unsupported(_)), "{err}");
        assert!(err.to_string().contains("canvas_init"));
    }

    #[test]
    fn rotate_y_moves_named_entity() {
        let mut world = World::new();
        world.spawn_named("cube", Transform::IDENTITY);
        let mut rt = ScriptRuntime::new();
        rt.load_source(
            "t.juni",
            "fn frame(dt: f32) -> i32:\n    rotate_y(entity(\"cube\"), 1.0)\n    return 0\n",
            None,
        )
        .unwrap();
        let input = InputState::new();
        let (_, quit) = tick(&mut rt, &mut world, &input);
        assert_eq!(rt.last_error(), None);
        let q = world.get("cube").unwrap().transform.rotation();
        assert!(q.length_squared() > 0.5);
        assert!((q.w - 1.0).abs() > 0.01);
        assert!(!quit);
    }

    #[test]
    fn escape_quits() {
        let mut world = World::new();
        let mut rt = ScriptRuntime::new();
        rt.load_source(
            "t.juni",
            "fn frame(dt: f32) -> i32:\n    if key_pressed(\"Escape\"):\n        quit()\n    return 0\n",
            None,
        )
        .unwrap();
        let mut input = InputState::new();
        input.set_key(Key::Escape, true);
        let (_, quit) = tick(&mut rt, &mut world, &input);
        assert_eq!(rt.last_error(), None);
        assert!(quit);
    }

    #[test]
    fn entity_self_binding() {
        let mut world = World::new();
        world.spawn_named("spinner", Transform::IDENTITY);
        let mut rt = ScriptRuntime::new();
        rt.load_source(
            "t.juni",
            "fn frame(dt: f32) -> i32:\n    rotate_y(self_entity(), 0.5)\n    return 0\n",
            Some("spinner".into()),
        )
        .unwrap();
        let input = InputState::new();
        tick(&mut rt, &mut world, &input);
        assert_eq!(rt.last_error(), None);
        let q = world.get("spinner").unwrap().transform.rotation();
        assert!((q.w - 1.0).abs() > 0.01);
    }

    #[test]
    fn state_persists_and_reads_positions() {
        let mut world = World::new();
        let mut t = Transform::IDENTITY;
        t.set_translation(Vec3::new(1.0, 2.0, 3.0));
        world.spawn_named("cube", t);
        let mut rt = ScriptRuntime::new();
        rt.load_source(
            "t.juni",
            r#"state:
    n: i32 = 0
    cube: i32 = 0

fn main() -> i32:
    cube = entity("cube")
    return 0

fn frame(dt: f32) -> i32:
    n = n + 1
    if pos_x(cube) != 1.0 or pos_y(cube) != 2.0 or pos_z(cube) != 3.0:
        quit()
    if n == 3:
        spawn_particles(0.0, 0.0, 0.0, 1, 1.0, 1.0, 1.0)
    return 0
"#,
            None,
        )
        .unwrap();
        let input = InputState::new();
        let (_, q1) = tick(&mut rt, &mut world, &input);
        let (_, q2) = tick(&mut rt, &mut world, &input);
        let (fx, q3) = tick(&mut rt, &mut world, &input);
        assert_eq!(rt.last_error(), None);
        assert!(!q1 && !q2 && !q3);
        assert_eq!(fx.particles.len(), 1);
    }

    #[test]
    fn unknown_entity_is_zero_and_writes_are_ignored() {
        let mut world = World::new();
        let mut rt = ScriptRuntime::new();
        rt.load_source(
            "t.juni",
            r#"fn frame(dt: f32) -> i32:
    let e = entity("ghost")
    if e != 0:
        quit()
    set_pos(e, 1.0, 1.0, 1.0)
    if exists(e):
        quit()
    return 0
"#,
            None,
        )
        .unwrap();
        let input = InputState::new();
        let (_, quit) = tick(&mut rt, &mut world, &input);
        assert_eq!(rt.last_error(), None);
        assert!(!quit);
    }

    #[test]
    fn spawn_returns_handle_and_queues_effect() {
        let mut world = World::new();
        let mut rt = ScriptRuntime::new();
        rt.load_source(
            "t.juni",
            r#"fn main() -> i32:
    let e = spawn_cube("box", 1.0, 2.0, 3.0, 0.5, 0.5, 0.5)
    if e == 0:
        quit()
    despawn(entity("old"))
    return 0
"#,
            None,
        )
        .unwrap();
        world.spawn_named("old", Transform::IDENTITY);
        let input = InputState::new();
        let (fx, quit) = tick(&mut rt, &mut world, &input);
        assert_eq!(rt.last_error(), None);
        assert!(!quit);
        assert_eq!(fx.spawns.len(), 1);
        assert_eq!(fx.spawns[0].name, "box");
        assert_eq!(fx.spawns[0].kind, PrimitiveKind::Cube);
        assert_eq!(fx.spawns[0].at, [1.0, 2.0, 3.0]);
        assert_eq!(fx.despawns, vec!["old".to_string()]);
    }

    #[test]
    fn tags_ui_and_logs() {
        let mut world = World::new();
        world.spawn_named("h1", Transform::IDENTITY);
        world.spawn_named("h2", Transform::IDENTITY);
        world.get_mut("h1").unwrap().add_tag("hazard");
        world.get_mut("h2").unwrap().add_tag("hazard");
        let mut rt = ScriptRuntime::new();
        rt.load_source(
            "t.juni",
            r#"fn frame(dt: f32) -> i32:
    let n = tag_count("hazard")
    let i = 0
    while i < n:
        let h = tag_at("hazard", i)
        if not has_tag(h, "hazard"):
            quit()
        i = i + 1
    ui_text(0.1, 0.2, 0.03, 1.0, 1.0, 1.0, "HUD")
    ui_rect(0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.5)
    print(n)
    log("hello from juni")
    return 0
"#,
            None,
        )
        .unwrap();
        let input = InputState::new();
        let (fx, quit) = tick(&mut rt, &mut world, &input);
        assert_eq!(rt.last_error(), None);
        assert!(!quit);
        assert_eq!(fx.ui_texts.len(), 1);
        assert_eq!(fx.ui_texts[0].text, "HUD");
        assert_eq!(fx.ui_rects.len(), 1);
        assert_eq!(fx.logs, vec!["2".to_string(), "hello from juni".to_string()]);
    }

    #[test]
    fn infinite_loop_traps_instead_of_hanging() {
        let mut world = World::new();
        let mut rt = ScriptRuntime::new();
        rt.load_source(
            "t.juni",
            "fn frame(dt: f32) -> i32:\n    while true:\n        rotate_y(0, 0.0)\n    return 0\n",
            None,
        )
        .unwrap();
        let input = InputState::new();
        tick(&mut rt, &mut world, &input);
        let err = rt.last_error().expect("trap reported").to_string();
        assert!(err.contains("too long"), "{err}");
        // The trapped script is skipped afterwards; the message stays visible
        // until the file is reloaded.
        let started = std::time::Instant::now();
        tick(&mut rt, &mut world, &input);
        assert!(started.elapsed().as_millis() < 500, "broken script must not re-run");
        assert_eq!(rt.last_error(), Some(err.as_str()));
    }

    #[test]
    fn spark_script_compiles_and_ticks() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../games/spark/scenes/spark.juni"
        );
        let src = std::fs::read_to_string(path).expect("spark.juni");
        ScriptRuntime::check_source(&src).expect("spark.juni types");
        let mut rt = ScriptRuntime::new();
        rt.load_source(path, &src, None).unwrap();
        let mut world = World::new();
        world.spawn_named("player", Transform::IDENTITY);
        world.spawn_named("goal", Transform::IDENTITY);
        world.spawn_named("hazard_a", Transform::IDENTITY);
        world.spawn_named("hazard_b", Transform::IDENTITY);
        world.get_mut("hazard_a").unwrap().add_tag("hazard");
        world.get_mut("hazard_b").unwrap().add_tag("hazard");
        world.get_mut("player").unwrap().add_tag("player");
        let input = InputState::new();
        tick(&mut rt, &mut world, &input);
        assert!(
            rt.last_error().is_none(),
            "spark tick error: {:?}",
            rt.last_error()
        );
    }
}
