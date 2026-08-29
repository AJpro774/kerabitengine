//! Rhai scripting for Kerabit games (2.0 host API).
//!
//! Scripts tick after the Rust `run` closure. Prefer `fn init()` / `fn update()`;
//! bare top-level statements still re-run each frame (1.1 compat).
//!
//! Persistent values: `set(key, value)` / `get(key)` / `has(key)`.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use kerabit_input::{InputState, Key, MouseButton};
use kerabit_math::Vec3;
use kerabit_world::World;
use rhai::{Array, Dynamic, Engine, ImmutableString, Scope, AST};

/// Errors from compiling or running a Rhai script.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Rhai(String),
}

impl ScriptError {
    fn rhai(err: impl std::fmt::Display) -> Self {
        Self::Rhai(err.to_string())
    }
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

#[derive(Clone, Copy, Debug)]
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
    pub reload: bool,
}

struct LoadedScript {
    path: PathBuf,
    ast: AST,
    entity: Option<String>,
    scope: Scope<'static>,
    inited: bool,
    has_update: bool,
    has_init: bool,
    mtime: Option<SystemTime>,
}

#[derive(Default)]
struct Host {
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
    store: HashMap<String, Dynamic>,
    world_ops: Vec<WorldOp>,
    effects: ScriptEffects,
}

enum WorldOp {
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

/// Compiled scripts + Rhai engine. Tick once per frame against a live world.
pub struct ScriptRuntime {
    engine: Engine,
    host: Rc<RefCell<Host>>,
    scripts: Vec<LoadedScript>,
    last_error: Option<String>,
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
        let host = Rc::new(RefCell::new(Host::default()));
        let engine = build_engine(host.clone());
        Self {
            engine,
            host,
            scripts: Vec::new(),
            last_error: None,
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

    /// Parse `source` for syntax errors (no host needed).
    pub fn check_source(source: &str) -> Result<(), ScriptError> {
        let engine = Engine::new();
        engine.compile(source).map(|_| ()).map_err(ScriptError::rhai)
    }

    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn clear(&mut self) {
        self.scripts.clear();
        self.last_error = None;
        self.host.borrow_mut().store.clear();
    }

    /// Compile and keep `path`. `entity` binds `self` to that name.
    pub fn load_file(
        &mut self,
        path: impl AsRef<Path>,
        entity: Option<String>,
    ) -> Result<(), ScriptError> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path)?;
        let mtime = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
        self.load_source_with_mtime(path, &source, entity, mtime)
    }

    /// Compile `source` as if it came from `name` (used in tests / editor).
    pub fn load_source(
        &mut self,
        name: impl AsRef<Path>,
        source: &str,
        entity: Option<String>,
    ) -> Result<(), ScriptError> {
        self.load_source_with_mtime(name, source, entity, None)
    }

    fn load_source_with_mtime(
        &mut self,
        name: impl AsRef<Path>,
        source: &str,
        entity: Option<String>,
        mtime: Option<SystemTime>,
    ) -> Result<(), ScriptError> {
        let ast = self
            .engine
            .compile(source)
            .map_err(ScriptError::rhai)?;
        let has_update = ast_has_fn(&ast, "update");
        let has_init = ast_has_fn(&ast, "init");
        let mut scope = Scope::new();
        if let Some(ref n) = entity {
            scope.push("self", n.clone());
        }
        self.scripts.push(LoadedScript {
            path: name.as_ref().to_path_buf(),
            ast,
            entity,
            scope,
            inited: false,
            has_update,
            has_init,
            mtime,
        });
        Ok(())
    }

    /// Re-read every loaded script file from disk (editor Reload).
    pub fn reload_from_disk(&mut self) -> Result<(), ScriptError> {
        let snapshot: Vec<(PathBuf, Option<String>)> = self
            .scripts
            .iter()
            .map(|s| (s.path.clone(), s.entity.clone()))
            .collect();
        self.scripts.clear();
        self.last_error = None;
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
            match self.engine.compile(&source) {
                Ok(ast) => {
                    let has_update = ast_has_fn(&ast, "update");
                    let has_init = ast_has_fn(&ast, "init");
                    let mut scope = Scope::new();
                    if let Some(ref n) = entity {
                        scope.push("self", n.clone());
                    }
                    self.scripts[i] = LoadedScript {
                        path,
                        ast,
                        entity,
                        scope,
                        inited: false,
                        has_update,
                        has_init,
                        mtime: Some(mtime),
                    };
                }
                Err(err) => {
                    self.last_error = Some(format!("{}: {err}", path.display()));
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

        {
            let mut host = self.host.borrow_mut();
            host.dt = dt;
            host.world_ops.clear();
            host.effects = ScriptEffects::default();
            fill_keys(&mut host, input);
            fill_mouse(&mut host, input);
            fill_world(&mut host, world);
        }

        let mut first_err: Option<String> = None;
        for script in &mut self.scripts {
            let result = if script.has_update {
                if !script.inited {
                    if script.has_init {
                        let r = self.engine.call_fn::<()>(
                            &mut script.scope,
                            &script.ast,
                            "init",
                            (),
                        );
                        if let Err(err) = r {
                            if first_err.is_none() {
                                first_err =
                                    Some(format!("{}: {err}", script.path.display()));
                            }
                        }
                    }
                    script.inited = true;
                }
                self.engine
                    .call_fn::<()>(&mut script.scope, &script.ast, "update", ())
                    .map(|_| ())
            } else {
                self.engine
                    .eval_ast_with_scope::<Dynamic>(&mut script.scope, &script.ast)
                    .map(|_| ())
            };
            if let Err(err) = result {
                if first_err.is_none() {
                    first_err = Some(format!("{}: {err}", script.path.display()));
                }
            }
        }
        self.last_error = first_err;

        let (world_ops, mut effects) = {
            let mut host = self.host.borrow_mut();
            (
                std::mem::take(&mut host.world_ops),
                std::mem::take(&mut host.effects),
            )
        };
        apply_world_ops(world, quit, world_ops);

        if effects.reload {
            if let Err(err) = self.reload_from_disk() {
                self.last_error = Some(err.to_string());
            }
            effects.reload = false;
        }

        effects
    }
}

fn ast_has_fn(ast: &AST, name: &str) -> bool {
    ast.iter_functions().any(|f| f.name == name)
}

fn build_engine(host: Rc<RefCell<Host>>) -> Engine {
    let mut engine = Engine::new();
    engine.set_max_operations(100_000);
    engine.set_max_call_levels(64);
    engine.set_max_expr_depths(64, 32);

    {
        let h = host.clone();
        engine.register_fn("dt", move || h.borrow().dt as f64);
    }
    {
        let h = host.clone();
        engine.register_fn("key_down", move |name: ImmutableString| -> bool {
            parse_key(name.as_str())
                .map(|k| h.borrow().keys_down.contains(&k))
                .unwrap_or(false)
        });
    }
    {
        let h = host.clone();
        engine.register_fn("key_pressed", move |name: ImmutableString| -> bool {
            parse_key(name.as_str())
                .map(|k| h.borrow().keys_pressed.contains(&k))
                .unwrap_or(false)
        });
    }
    {
        let h = host.clone();
        engine.register_fn("mouse_pos", move || -> Array {
            let host = h.borrow();
            vec![
                Dynamic::from(host.mouse_x as f64),
                Dynamic::from(host.mouse_y as f64),
            ]
        });
    }
    {
        let h = host.clone();
        engine.register_fn("mouse_button_down", move |name: ImmutableString| -> bool {
            parse_mouse(name.as_str())
                .map(|b| h.borrow().mouse_down.contains(&b))
                .unwrap_or(false)
        });
    }
    {
        let h = host.clone();
        engine.register_fn(
            "mouse_button_pressed",
            move |name: ImmutableString| -> bool {
                parse_mouse(name.as_str())
                    .map(|b| h.borrow().mouse_pressed.contains(&b))
                    .unwrap_or(false)
            },
        );
    }
    {
        let h = host.clone();
        engine.register_fn("quit", move || {
            h.borrow_mut().world_ops.push(WorldOp::Quit);
        });
    }
    {
        let h = host.clone();
        engine.register_fn("exists", move |name: ImmutableString| -> bool {
            h.borrow().known_names.contains(name.as_str())
        });
    }
    {
        let h = host.clone();
        engine.register_fn("names_with_tag", move |tag: ImmutableString| -> Array {
            h.borrow()
                .tag_index
                .get(tag.as_str())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(Dynamic::from)
                .collect()
        });
    }
    {
        let h = host.clone();
        engine.register_fn("pos", move |name: ImmutableString| -> Array {
            arr3(h.borrow().positions.get(name.as_str()).copied())
        });
    }
    {
        let h = host.clone();
        engine.register_fn("get_pos", move |name: ImmutableString| -> Array {
            arr3(h.borrow().positions.get(name.as_str()).copied())
        });
    }
    {
        let h = host.clone();
        engine.register_fn("scale", move |name: ImmutableString| -> Array {
            arr3(h.borrow().scales.get(name.as_str()).copied())
        });
    }
    {
        let h = host.clone();
        engine.register_fn("enabled", move |name: ImmutableString| -> bool {
            h.borrow()
                .enabled
                .get(name.as_str())
                .copied()
                .unwrap_or(false)
        });
    }
    {
        let h = host.clone();
        engine.register_fn(
            "has_tag",
            move |name: ImmutableString, tag: ImmutableString| -> bool {
                h.borrow()
                    .tags
                    .get(name.as_str())
                    .map(|t| t.contains(tag.as_str()))
                    .unwrap_or(false)
            },
        );
    }
    {
        let h = host.clone();
        engine.register_fn("has", move |key: ImmutableString| -> bool {
            h.borrow().store.contains_key(key.as_str())
        });
    }
    {
        let h = host.clone();
        engine.register_fn("get", move |key: ImmutableString| -> Dynamic {
            h.borrow()
                .store
                .get(key.as_str())
                .cloned()
                .unwrap_or(Dynamic::UNIT)
        });
    }
    {
        let h = host.clone();
        engine.register_fn("set", move |key: ImmutableString, value: Dynamic| {
            h.borrow_mut().store.insert(key.to_string(), value);
        });
    }
    {
        let h = host.clone();
        engine.register_fn("rotate_x", move |name: ImmutableString, rad: f64| {
            h.borrow_mut().world_ops.push(WorldOp::RotateX {
                name: name.to_string(),
                rad: rad as f32,
            });
        });
    }
    {
        let h = host.clone();
        engine.register_fn("rotate_y", move |name: ImmutableString, rad: f64| {
            h.borrow_mut().world_ops.push(WorldOp::RotateY {
                name: name.to_string(),
                rad: rad as f32,
            });
        });
    }
    {
        let h = host.clone();
        engine.register_fn("rotate_z", move |name: ImmutableString, rad: f64| {
            h.borrow_mut().world_ops.push(WorldOp::RotateZ {
                name: name.to_string(),
                rad: rad as f32,
            });
        });
    }
    {
        let h = host.clone();
        engine.register_fn(
            "translate",
            move |name: ImmutableString, x: f64, y: f64, z: f64| {
                h.borrow_mut().world_ops.push(WorldOp::Translate {
                    name: name.to_string(),
                    x: x as f32,
                    y: y as f32,
                    z: z as f32,
                });
            },
        );
    }
    {
        let h = host.clone();
        engine.register_fn(
            "set_pos",
            move |name: ImmutableString, x: f64, y: f64, z: f64| {
                h.borrow_mut().world_ops.push(WorldOp::SetPos {
                    name: name.to_string(),
                    x: x as f32,
                    y: y as f32,
                    z: z as f32,
                });
            },
        );
    }
    {
        let h = host.clone();
        engine.register_fn(
            "set_scale",
            move |name: ImmutableString, x: f64, y: f64, z: f64| {
                h.borrow_mut().world_ops.push(WorldOp::SetScale {
                    name: name.to_string(),
                    x: x as f32,
                    y: y as f32,
                    z: z as f32,
                });
            },
        );
    }
    {
        let h = host.clone();
        engine.register_fn("set_enabled", move |name: ImmutableString, on: bool| {
            h.borrow_mut().world_ops.push(WorldOp::SetEnabled {
                name: name.to_string(),
                enabled: on,
            });
        });
    }
    {
        let h = host.clone();
        engine.register_fn(
            "add_tag",
            move |name: ImmutableString, tag: ImmutableString| {
                h.borrow_mut().world_ops.push(WorldOp::AddTag {
                    name: name.to_string(),
                    tag: tag.to_string(),
                });
            },
        );
    }
    {
        let h = host.clone();
        engine.register_fn(
            "remove_tag",
            move |name: ImmutableString, tag: ImmutableString| {
                h.borrow_mut().world_ops.push(WorldOp::RemoveTag {
                    name: name.to_string(),
                    tag: tag.to_string(),
                });
            },
        );
    }
    {
        let h = host.clone();
        engine.register_fn("despawn", move |name: ImmutableString| {
            h.borrow_mut().effects.despawns.push(name.to_string());
        });
    }
    {
        let h = host.clone();
        engine.register_fn(
            "spawn_cube",
            move |name: ImmutableString,
                  r: f64,
                  g: f64,
                  b: f64,
                  x: f64,
                  y: f64,
                  z: f64| {
                h.borrow_mut().effects.spawns.push(SpawnPrimitive {
                    name: name.to_string(),
                    kind: PrimitiveKind::Cube,
                    color: [r as f32, g as f32, b as f32],
                    at: [x as f32, y as f32, z as f32],
                    scale: [1.0, 1.0, 1.0],
                });
            },
        );
    }
    {
        let h = host.clone();
        engine.register_fn(
            "spawn_cube_ex",
            move |name: ImmutableString,
                  r: f64,
                  g: f64,
                  b: f64,
                  x: f64,
                  y: f64,
                  z: f64,
                  sx: f64,
                  sy: f64,
                  sz: f64| {
                h.borrow_mut().effects.spawns.push(SpawnPrimitive {
                    name: name.to_string(),
                    kind: PrimitiveKind::Cube,
                    color: [r as f32, g as f32, b as f32],
                    at: [x as f32, y as f32, z as f32],
                    scale: [sx as f32, sy as f32, sz as f32],
                });
            },
        );
    }
    {
        let h = host.clone();
        engine.register_fn(
            "spawn_plane",
            move |name: ImmutableString,
                  r: f64,
                  g: f64,
                  b: f64,
                  x: f64,
                  y: f64,
                  z: f64,
                  size: f64| {
                let s = size as f32;
                h.borrow_mut().effects.spawns.push(SpawnPrimitive {
                    name: name.to_string(),
                    kind: PrimitiveKind::Plane,
                    color: [r as f32, g as f32, b as f32],
                    at: [x as f32, y as f32, z as f32],
                    scale: [s, 1.0, s],
                });
            },
        );
    }
    {
        let h = host.clone();
        engine.register_fn(
            "spawn_prefab",
            move |path: ImmutableString, x: f64, y: f64, z: f64| {
                h.borrow_mut().effects.prefabs.push(SpawnPrefab {
                    path: path.to_string(),
                    offset: [x as f32, y as f32, z as f32],
                });
            },
        );
    }
    {
        let h = host.clone();
        engine.register_fn(
            "move_planar",
            move |name: ImmutableString, dx: f64, dz: f64| {
                h.borrow_mut().effects.moves.push(MovePlanar {
                    name: name.to_string(),
                    dx: dx as f32,
                    dz: dz as f32,
                });
            },
        );
    }
    {
        let h = host.clone();
        engine.register_fn("register_box", move |name: ImmutableString| {
            h.borrow_mut()
                .effects
                .register_boxes
                .push(name.to_string());
        });
    }
    {
        let h = host.clone();
        engine.register_fn("play", move |path: ImmutableString| {
            h.borrow_mut().effects.plays.push(PlaySound {
                path: path.to_string(),
                at: None,
            });
        });
    }
    {
        let h = host.clone();
        engine.register_fn(
            "play_at",
            move |path: ImmutableString, x: f64, y: f64, z: f64| {
                h.borrow_mut().effects.plays.push(PlaySound {
                    path: path.to_string(),
                    at: Some([x as f32, y as f32, z as f32]),
                });
            },
        );
    }
    {
        let h = host.clone();
        engine.register_fn(
            "spawn_particles",
            move |x: f64, y: f64, z: f64, count: i64, r: f64, g: f64, b: f64| {
                h.borrow_mut().effects.particles.push(ParticleCmd {
                    origin: [x as f32, y as f32, z as f32],
                    count: count.max(0) as u32,
                    color: [r as f32, g as f32, b as f32],
                });
            },
        );
    }
    {
        let h = host.clone();
        engine.register_fn(
            "set_camera",
            move |ex: f64, ey: f64, ez: f64, tx: f64, ty: f64, tz: f64| {
                h.borrow_mut().effects.camera = Some(CameraCmd {
                    eye: [ex as f32, ey as f32, ez as f32],
                    target: [tx as f32, ty as f32, tz as f32],
                });
            },
        );
    }
    {
        let host = host.clone();
        engine.register_fn(
            "ui_rect",
            move |x: f64, y: f64, w: f64, height: f64, r: f64, g: f64, b: f64, a: f64| {
                host.borrow_mut().effects.ui_rects.push(UiRectCmd {
                    x: x as f32,
                    y: y as f32,
                    w: w as f32,
                    h: height as f32,
                    color: [r as f32, g as f32, b as f32, a as f32],
                });
            },
        );
    }
    {
        let h = host.clone();
        engine.register_fn(
            "ui_text",
            move |x: f64, y: f64, size: f64, r: f64, g: f64, b: f64, text: ImmutableString| {
                h.borrow_mut().effects.ui_texts.push(UiTextCmd {
                    x: x as f32,
                    y: y as f32,
                    size: size as f32,
                    color: [r as f32, g as f32, b as f32],
                    text: text.to_string(),
                });
            },
        );
    }
    {
        let h = host.clone();
        engine.register_fn("reload_scripts", move || {
            h.borrow_mut().effects.reload = true;
        });
    }

    engine
}

fn arr3(v: Option<[f32; 3]>) -> Array {
    let p = v.unwrap_or([0.0, 0.0, 0.0]);
    vec![
        Dynamic::from(p[0] as f64),
        Dynamic::from(p[1] as f64),
        Dynamic::from(p[2] as f64),
    ]
}

fn fill_keys(host: &mut Host, input: &InputState) {
    host.keys_down.clear();
    host.keys_pressed.clear();
    for k in ALL_KEYS {
        if input.key_down(*k) {
            host.keys_down.insert(*k);
        }
        if input.key_pressed(*k) {
            host.keys_pressed.insert(*k);
        }
    }
}

fn fill_mouse(host: &mut Host, input: &InputState) {
    let (x, y) = input.mouse_position();
    host.mouse_x = x;
    host.mouse_y = y;
    host.mouse_down.clear();
    host.mouse_pressed.clear();
    for b in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
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

fn parse_mouse(name: &str) -> Option<MouseButton> {
    match name.trim().to_ascii_lowercase().as_str() {
        "left" | "lmb" => Some(MouseButton::Left),
        "right" | "rmb" => Some(MouseButton::Right),
        "middle" | "mmb" => Some(MouseButton::Middle),
        _ => None,
    }
}

fn parse_key(name: &str) -> Option<Key> {
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

    #[test]
    fn check_source_rejects_garbage() {
        assert!(ScriptRuntime::check_source("rotate_y(").is_err());
        assert!(ScriptRuntime::check_source("rotate_y(\"cube\", 1);").is_ok());
    }

    #[test]
    fn rotate_y_moves_named_entity() {
        let mut world = World::new();
        world.spawn_named("cube", Transform::IDENTITY);
        let mut rt = ScriptRuntime::new();
        rt.load_source("t.rhai", "rotate_y(\"cube\", 1.0);", None)
            .unwrap();
        let input = InputState::new();
        let mut quit = false;
        let _ = rt.tick(0.016, &input, &mut world, &mut quit);
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
            "t.rhai",
            "if key_pressed(\"Escape\") { quit(); }",
            None,
        )
        .unwrap();
        let mut input = InputState::new();
        input.set_key(Key::Escape, true);
        let mut quit = false;
        let _ = rt.tick(0.016, &input, &mut world, &mut quit);
        assert!(quit);
    }

    #[test]
    fn entity_self_binding() {
        let mut world = World::new();
        world.spawn_named("spinner", Transform::IDENTITY);
        let mut rt = ScriptRuntime::new();
        rt.load_source("t.rhai", "rotate_y(self, 0.5);", Some("spinner".into()))
            .unwrap();
        let input = InputState::new();
        let mut quit = false;
        let _ = rt.tick(0.016, &input, &mut world, &mut quit);
        let q = world.get("spinner").unwrap().transform.rotation();
        assert!((q.w - 1.0).abs() > 0.01);
    }

    #[test]
    fn getters_and_store_persist() {
        let mut world = World::new();
        let mut t = Transform::IDENTITY;
        t.set_translation(Vec3::new(1.0, 2.0, 3.0));
        world.spawn_named("cube", t);
        let mut rt = ScriptRuntime::new();
        rt.load_source(
            "t.rhai",
            r#"
            fn init() { set("n", 0); }
            fn update() {
                set("n", get("n") + 1);
                let p = pos("cube");
                if p[0] != 1.0 || p[1] != 2.0 { quit(); }
                if get("n") == 3 {
                    spawn_particles(0.0, 0.0, 0.0, 1, 1.0, 1.0, 1.0);
                }
            }
            "#,
            None,
        )
        .unwrap();
        let input = InputState::new();
        let mut quit = false;
        let _ = rt.tick(0.016, &input, &mut world, &mut quit);
        let _ = rt.tick(0.016, &input, &mut world, &mut quit);
        let fx = rt.tick(0.016, &input, &mut world, &mut quit);
        assert!(!quit);
        assert_eq!(fx.particles.len(), 1);
    }

    #[test]
    fn despawn_effect_queued() {
        let mut world = World::new();
        world.spawn_named("cube", Transform::IDENTITY);
        let mut rt = ScriptRuntime::new();
        rt.load_source("t.rhai", "despawn(\"cube\");", None).unwrap();
        let input = InputState::new();
        let mut quit = false;
        let fx = rt.tick(0.016, &input, &mut world, &mut quit);
        assert_eq!(fx.despawns, vec!["cube".to_string()]);
    }

    #[test]
    fn spark_script_compiles_and_ticks() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../games/spark/scenes/spark.rhai"
        );
        let src = std::fs::read_to_string(path).expect("spark.rhai");
        ScriptRuntime::check_source(&src).expect("spark.rhai syntax");
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
        let mut quit = false;
        let _ = rt.tick(0.016, &input, &mut world, &mut quit);
        assert!(
            rt.last_error().is_none(),
            "spark tick error: {:?}",
            rt.last_error()
        );
    }
}
