//! WASM import wiring: the `kerabit` extern module declared by the prelude
//! plus the subset of Juno `env` builtins (math / strings / print) that make
//! sense outside a browser.
//!
//! Every host function reads the per-frame [`Host`] snapshot or pushes a
//! [`WorldOp`] / effect; none touches engine state directly.

use wasmtime::{Caller, Linker};

use crate::{
    parse_key, parse_mouse, CameraCmd, MovePlanar, ParticleCmd, PlaySound, PrimitiveKind,
    ScriptData, SpawnPrefab, SpawnPrimitive, UiRectCmd, UiTextCmd, WorldOp,
};

type C<'a> = Caller<'a, ScriptData>;

/// `env` builtins Kerabit implements. Anything else a script pulls in
/// (canvas, WebGPU, the Juno ECS) is rejected at load with a hint.
pub(crate) fn supports_builtin(name: &str) -> bool {
    matches!(
        name,
        "sqrt_f32"
            | "webgpu_stub"
            | "print_str"
            | "print_i32"
            | "print_f32"
            | "sin_f32"
            | "cos_f32"
            | "tan_f32"
            | "abs_f32"
            | "floor_f32"
            | "ceil_f32"
            | "min_f32"
            | "max_f32"
            | "rand_f32"
            | "now_f32"
            | "str_len"
            | "str_eq"
            | "clamp_f32"
            | "lerp_f32"
            | "pow_f32"
            | "sign_f32"
            | "fmod_f32"
            | "smoothstep_f32"
            | "deg_to_rad_f32"
            | "rad_to_deg_f32"
            | "dist2_f32"
            | "pi_f32"
            | "abs_i32"
            | "min_i32"
            | "max_i32"
            | "clamp_i32"
            | "len2_f32"
            | "dot2_f32"
    )
}

/// Read a Juni `str` (`[len: i32][utf8]`) out of the instance memory.
fn read_str(caller: &mut C<'_>, ptr: i32) -> String {
    let Some(mem) = caller.data().memory else {
        return String::new();
    };
    let data = mem.data(&*caller);
    let p = ptr.max(0) as usize;
    if p + 4 > data.len() {
        return String::new();
    }
    let len = i32::from_le_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]]).max(0) as usize;
    let end = (p + 4).saturating_add(len).min(data.len());
    String::from_utf8_lossy(&data[p + 4..end]).into_owned()
}

fn name_of(caller: &C<'_>, handle: i32) -> Option<String> {
    caller.data().host.name_of(handle).map(str::to_owned)
}

fn push_op(caller: &mut C<'_>, handle: i32, op: impl FnOnce(String) -> WorldOp) {
    if let Some(name) = name_of(caller, handle) {
        caller.data_mut().host.world_ops.push(op(name));
    }
}

fn pos_component(caller: &C<'_>, handle: i32, axis: usize) -> f32 {
    let host = &caller.data().host;
    host.name_of(handle)
        .and_then(|n| host.positions.get(n))
        .map(|p| p[axis])
        .unwrap_or(0.0)
}

fn scale_component(caller: &C<'_>, handle: i32, axis: usize) -> f32 {
    let host = &caller.data().host;
    host.name_of(handle)
        .and_then(|n| host.scales.get(n))
        .map(|s| s[axis])
        .unwrap_or(1.0)
}

fn log(caller: &mut C<'_>, text: String) {
    let label = caller.data().script_label.clone();
    println!("[juni] {label}: {text}");
    caller.data_mut().host.effects.logs.push(text);
}

fn b(v: bool) -> i32 {
    v as i32
}

/// Register every import the prelude declares plus the supported `env` builtins.
pub(crate) fn link(l: &mut Linker<ScriptData>) -> wasmtime::Result<()> {
    link_env(l)?;
    let m = "kerabit";

    // --- frame / app ---
    l.func_wrap(m, "dt", |c: C<'_>| -> f32 { c.data().host.dt })?;
    l.func_wrap(m, "quit", |mut c: C<'_>| {
        c.data_mut().host.world_ops.push(WorldOp::Quit);
    })?;
    l.func_wrap(m, "reload_scripts", |mut c: C<'_>| {
        c.data_mut().host.effects.reload = true;
    })?;
    l.func_wrap(m, "log", |mut c: C<'_>, text: i32| {
        let text = read_str(&mut c, text);
        log(&mut c, text);
    })?;

    // --- input ---
    l.func_wrap(m, "key_down", |mut c: C<'_>, name: i32| -> i32 {
        let name = read_str(&mut c, name);
        b(parse_key(&name).is_some_and(|k| c.data().host.keys_down.contains(&k)))
    })?;
    l.func_wrap(m, "key_pressed", |mut c: C<'_>, name: i32| -> i32 {
        let name = read_str(&mut c, name);
        b(parse_key(&name).is_some_and(|k| c.data().host.keys_pressed.contains(&k)))
    })?;
    l.func_wrap(m, "mouse_x", |c: C<'_>| -> f32 { c.data().host.mouse_x })?;
    l.func_wrap(m, "mouse_y", |c: C<'_>| -> f32 { c.data().host.mouse_y })?;
    l.func_wrap(m, "mouse_down", |mut c: C<'_>, name: i32| -> i32 {
        let name = read_str(&mut c, name);
        b(parse_mouse(&name).is_some_and(|k| c.data().host.mouse_down.contains(&k)))
    })?;
    l.func_wrap(m, "mouse_pressed", |mut c: C<'_>, name: i32| -> i32 {
        let name = read_str(&mut c, name);
        b(parse_mouse(&name).is_some_and(|k| c.data().host.mouse_pressed.contains(&k)))
    })?;

    // --- entity reads ---
    l.func_wrap(m, "entity", |mut c: C<'_>, name: i32| -> i32 {
        let name = read_str(&mut c, name);
        let host = &mut c.data_mut().host;
        if host.known_names.contains(&name) {
            host.intern(&name)
        } else {
            0
        }
    })?;
    l.func_wrap(m, "self_entity", |mut c: C<'_>| -> i32 {
        let Some(name) = c.data().self_entity.clone() else {
            return 0;
        };
        c.data_mut().host.intern(&name)
    })?;
    l.func_wrap(m, "exists", |c: C<'_>, e: i32| -> i32 {
        let host = &c.data().host;
        b(host.name_of(e).is_some_and(|n| host.known_names.contains(n)))
    })?;
    l.func_wrap(m, "enabled", |c: C<'_>, e: i32| -> i32 {
        let host = &c.data().host;
        b(host
            .name_of(e)
            .and_then(|n| host.enabled.get(n).copied())
            .unwrap_or(false))
    })?;
    l.func_wrap(m, "pos_x", |c: C<'_>, e: i32| -> f32 { pos_component(&c, e, 0) })?;
    l.func_wrap(m, "pos_y", |c: C<'_>, e: i32| -> f32 { pos_component(&c, e, 1) })?;
    l.func_wrap(m, "pos_z", |c: C<'_>, e: i32| -> f32 { pos_component(&c, e, 2) })?;
    l.func_wrap(m, "scale_x", |c: C<'_>, e: i32| -> f32 { scale_component(&c, e, 0) })?;
    l.func_wrap(m, "scale_y", |c: C<'_>, e: i32| -> f32 { scale_component(&c, e, 1) })?;
    l.func_wrap(m, "scale_z", |c: C<'_>, e: i32| -> f32 { scale_component(&c, e, 2) })?;
    l.func_wrap(m, "has_tag", |mut c: C<'_>, e: i32, tag: i32| -> i32 {
        let tag = read_str(&mut c, tag);
        let host = &c.data().host;
        b(host
            .name_of(e)
            .and_then(|n| host.tags.get(n))
            .is_some_and(|t| t.contains(&tag)))
    })?;
    l.func_wrap(m, "tag_count", |mut c: C<'_>, tag: i32| -> i32 {
        let tag = read_str(&mut c, tag);
        c.data().host.tag_index.get(&tag).map_or(0, |v| v.len() as i32)
    })?;
    l.func_wrap(m, "tag_at", |mut c: C<'_>, tag: i32, index: i32| -> i32 {
        let tag = read_str(&mut c, tag);
        let name = c
            .data()
            .host
            .tag_index
            .get(&tag)
            .and_then(|v| v.get(index.max(0) as usize))
            .cloned();
        match name {
            Some(n) => c.data_mut().host.intern(&n),
            None => 0,
        }
    })?;

    // --- entity writes ---
    l.func_wrap(m, "set_pos", |mut c: C<'_>, e: i32, x: f32, y: f32, z: f32| {
        push_op(&mut c, e, |name| WorldOp::SetPos { name, x, y, z });
    })?;
    l.func_wrap(m, "translate", |mut c: C<'_>, e: i32, x: f32, y: f32, z: f32| {
        push_op(&mut c, e, |name| WorldOp::Translate { name, x, y, z });
    })?;
    l.func_wrap(m, "rotate_x", |mut c: C<'_>, e: i32, rad: f32| {
        push_op(&mut c, e, |name| WorldOp::RotateX { name, rad });
    })?;
    l.func_wrap(m, "rotate_y", |mut c: C<'_>, e: i32, rad: f32| {
        push_op(&mut c, e, |name| WorldOp::RotateY { name, rad });
    })?;
    l.func_wrap(m, "rotate_z", |mut c: C<'_>, e: i32, rad: f32| {
        push_op(&mut c, e, |name| WorldOp::RotateZ { name, rad });
    })?;
    l.func_wrap(m, "set_scale", |mut c: C<'_>, e: i32, x: f32, y: f32, z: f32| {
        push_op(&mut c, e, |name| WorldOp::SetScale { name, x, y, z });
    })?;
    l.func_wrap(m, "set_enabled", |mut c: C<'_>, e: i32, on: i32| {
        push_op(&mut c, e, |name| WorldOp::SetEnabled {
            name,
            enabled: on != 0,
        });
    })?;
    l.func_wrap(m, "add_tag", |mut c: C<'_>, e: i32, tag: i32| {
        let tag = read_str(&mut c, tag);
        push_op(&mut c, e, |name| WorldOp::AddTag { name, tag });
    })?;
    l.func_wrap(m, "remove_tag", |mut c: C<'_>, e: i32, tag: i32| {
        let tag = read_str(&mut c, tag);
        push_op(&mut c, e, |name| WorldOp::RemoveTag { name, tag });
    })?;

    // --- authorship ---
    l.func_wrap(m, "despawn", |mut c: C<'_>, e: i32| {
        if let Some(name) = name_of(&c, e) {
            c.data_mut().host.effects.despawns.push(name);
        }
    })?;
    l.func_wrap(
        m,
        "spawn_cube",
        |mut c: C<'_>, name: i32, x: f32, y: f32, z: f32, r: f32, g: f32, bl: f32| -> i32 {
            let name = read_str(&mut c, name);
            let host = &mut c.data_mut().host;
            host.effects.spawns.push(SpawnPrimitive {
                name: name.clone(),
                kind: PrimitiveKind::Cube,
                color: [r, g, bl],
                at: [x, y, z],
                scale: [1.0, 1.0, 1.0],
            });
            host.intern(&name)
        },
    )?;
    l.func_wrap(
        m,
        "spawn_cube_ex",
        |mut c: C<'_>,
         name: i32,
         x: f32,
         y: f32,
         z: f32,
         sx: f32,
         sy: f32,
         sz: f32,
         r: f32,
         g: f32,
         bl: f32|
         -> i32 {
            let name = read_str(&mut c, name);
            let host = &mut c.data_mut().host;
            host.effects.spawns.push(SpawnPrimitive {
                name: name.clone(),
                kind: PrimitiveKind::Cube,
                color: [r, g, bl],
                at: [x, y, z],
                scale: [sx, sy, sz],
            });
            host.intern(&name)
        },
    )?;
    l.func_wrap(
        m,
        "spawn_plane",
        |mut c: C<'_>, name: i32, x: f32, y: f32, z: f32, size: f32, r: f32, g: f32, bl: f32|
         -> i32 {
            let name = read_str(&mut c, name);
            let host = &mut c.data_mut().host;
            host.effects.spawns.push(SpawnPrimitive {
                name: name.clone(),
                kind: PrimitiveKind::Plane,
                color: [r, g, bl],
                at: [x, y, z],
                scale: [size, size, size],
            });
            host.intern(&name)
        },
    )?;
    l.func_wrap(m, "spawn_prefab", |mut c: C<'_>, path: i32, x: f32, y: f32, z: f32| {
        let path = read_str(&mut c, path);
        c.data_mut().host.effects.prefabs.push(SpawnPrefab {
            path,
            offset: [x, y, z],
        });
    })?;

    // --- physics ---
    l.func_wrap(m, "move_planar", |mut c: C<'_>, e: i32, dx: f32, dz: f32| {
        if let Some(name) = name_of(&c, e) {
            c.data_mut()
                .host
                .effects
                .moves
                .push(MovePlanar { name, dx, dz });
        }
    })?;
    l.func_wrap(m, "register_box", |mut c: C<'_>, e: i32| {
        if let Some(name) = name_of(&c, e) {
            c.data_mut().host.effects.register_boxes.push(name);
        }
    })?;

    // --- juice / view ---
    l.func_wrap(m, "play", |mut c: C<'_>, path: i32| {
        let path = read_str(&mut c, path);
        c.data_mut()
            .host
            .effects
            .plays
            .push(PlaySound { path, at: None });
    })?;
    l.func_wrap(m, "play_at", |mut c: C<'_>, path: i32, x: f32, y: f32, z: f32| {
        let path = read_str(&mut c, path);
        c.data_mut().host.effects.plays.push(PlaySound {
            path,
            at: Some([x, y, z]),
        });
    })?;
    l.func_wrap(
        m,
        "spawn_particles",
        |mut c: C<'_>, x: f32, y: f32, z: f32, count: i32, r: f32, g: f32, bl: f32| {
            c.data_mut().host.effects.particles.push(ParticleCmd {
                origin: [x, y, z],
                count: count.max(1) as u32,
                color: [r, g, bl],
            });
        },
    )?;
    l.func_wrap(
        m,
        "set_camera",
        |mut c: C<'_>, ex: f32, ey: f32, ez: f32, tx: f32, ty: f32, tz: f32| {
            c.data_mut().host.effects.camera = Some(CameraCmd {
                eye: [ex, ey, ez],
                target: [tx, ty, tz],
            });
        },
    )?;

    // --- overlay ---
    l.func_wrap(
        m,
        "ui_text",
        |mut c: C<'_>, x: f32, y: f32, size: f32, r: f32, g: f32, bl: f32, text: i32| {
            let text = read_str(&mut c, text);
            c.data_mut().host.effects.ui_texts.push(UiTextCmd {
                x,
                y,
                size,
                color: [r, g, bl],
                text,
            });
        },
    )?;
    l.func_wrap(
        m,
        "ui_rect",
        |mut c: C<'_>, x: f32, y: f32, w: f32, h: f32, r: f32, g: f32, bl: f32, a: f32| {
            c.data_mut().host.effects.ui_rects.push(UiRectCmd {
                x,
                y,
                w,
                h,
                color: [r, g, bl, a],
            });
        },
    )?;

    Ok(())
}

/// Juno `env` builtins that are pure or host-agnostic.
fn link_env(l: &mut Linker<ScriptData>) -> wasmtime::Result<()> {
    let m = "env";
    l.func_wrap(m, "sqrt_f32", |x: f32| -> f32 { x.sqrt() })?;
    l.func_wrap(m, "sin_f32", |x: f32| -> f32 { x.sin() })?;
    l.func_wrap(m, "cos_f32", |x: f32| -> f32 { x.cos() })?;
    l.func_wrap(m, "tan_f32", |x: f32| -> f32 { x.tan() })?;
    l.func_wrap(m, "abs_f32", |x: f32| -> f32 { x.abs() })?;
    l.func_wrap(m, "floor_f32", |x: f32| -> f32 { x.floor() })?;
    l.func_wrap(m, "ceil_f32", |x: f32| -> f32 { x.ceil() })?;
    l.func_wrap(m, "min_f32", |a: f32, b: f32| -> f32 { a.min(b) })?;
    l.func_wrap(m, "max_f32", |a: f32, b: f32| -> f32 { a.max(b) })?;
    l.func_wrap(m, "clamp_f32", |x: f32, lo: f32, hi: f32| -> f32 { x.clamp(lo, hi.max(lo)) })?;
    l.func_wrap(m, "lerp_f32", |a: f32, b: f32, t: f32| -> f32 { a + (b - a) * t })?;
    l.func_wrap(m, "pow_f32", |x: f32, y: f32| -> f32 { x.powf(y) })?;
    l.func_wrap(m, "sign_f32", |x: f32| -> f32 {
        if x > 0.0 {
            1.0
        } else if x < 0.0 {
            -1.0
        } else {
            0.0
        }
    })?;
    l.func_wrap(m, "fmod_f32", |x: f32, y: f32| -> f32 { if y == 0.0 { 0.0 } else { x % y } })?;
    l.func_wrap(m, "smoothstep_f32", |e0: f32, e1: f32, x: f32| -> f32 {
        let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    })?;
    l.func_wrap(m, "deg_to_rad_f32", |x: f32| -> f32 { x.to_radians() })?;
    l.func_wrap(m, "rad_to_deg_f32", |x: f32| -> f32 { x.to_degrees() })?;
    l.func_wrap(m, "dist2_f32", |x1: f32, y1: f32, x2: f32, y2: f32| -> f32 {
        ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt()
    })?;
    l.func_wrap(m, "pi_f32", || -> f32 { std::f32::consts::PI })?;
    l.func_wrap(m, "abs_i32", |x: i32| -> i32 { x.wrapping_abs() })?;
    l.func_wrap(m, "min_i32", |a: i32, b: i32| -> i32 { a.min(b) })?;
    l.func_wrap(m, "max_i32", |a: i32, b: i32| -> i32 { a.max(b) })?;
    l.func_wrap(m, "clamp_i32", |x: i32, lo: i32, hi: i32| -> i32 { x.clamp(lo, hi.max(lo)) })?;
    l.func_wrap(m, "len2_f32", |x: f32, y: f32| -> f32 { (x * x + y * y).sqrt() })?;
    l.func_wrap(m, "dot2_f32", |x1: f32, y1: f32, x2: f32, y2: f32| -> f32 { x1 * x2 + y1 * y2 })?;
    l.func_wrap(m, "rand_f32", || -> f32 {
        // xorshift on a thread-local seed; deterministic enough for juice.
        use std::cell::Cell;
        thread_local! { static SEED: Cell<u32> = const { Cell::new(0x9E37_79B9) }; }
        SEED.with(|s| {
            let mut x = s.get();
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            s.set(x);
            (x >> 8) as f32 / (1u32 << 24) as f32
        })
    })?;
    l.func_wrap(m, "now_f32", || -> f32 {
        use std::sync::OnceLock;
        use std::time::Instant;
        static START: OnceLock<Instant> = OnceLock::new();
        START.get_or_init(Instant::now).elapsed().as_secs_f32() * 1000.0
    })?;
    l.func_wrap(m, "webgpu_stub", |_code: i32| {})?;
    l.func_wrap(m, "str_len", |mut c: C<'_>, s: i32| -> i32 {
        read_str(&mut c, s).len() as i32
    })?;
    l.func_wrap(m, "str_eq", |mut c: C<'_>, a: i32, bp: i32| -> i32 {
        let a = read_str(&mut c, a);
        let bs = read_str(&mut c, bp);
        b(a == bs)
    })?;
    l.func_wrap(m, "print_str", |mut c: C<'_>, s: i32| {
        let s = read_str(&mut c, s);
        log(&mut c, s);
    })?;
    l.func_wrap(m, "print_i32", |mut c: C<'_>, v: i32| {
        log(&mut c, v.to_string());
    })?;
    l.func_wrap(m, "print_f32", |mut c: C<'_>, v: f32| {
        log(&mut c, v.to_string());
    })?;
    Ok(())
}
