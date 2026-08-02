#!/usr/bin/env python3
"""CPU soft-render of the Kerabit showcase scene (same layout/orbit as games/showcase).

Used when Metal/wgpu is unavailable (e.g. restricted agent hosts). Prefer:
  KERABIT_SHOWCASE_RECORD=1 cargo run -p showcase --release
when a GPU adapter is present — that path dumps real-engine frames.
"""

from __future__ import annotations

import math
import struct
import zlib
from pathlib import Path

WIDTH, HEIGHT = 640, 360
FPS = 20
LOOP_SECS = 20.0
FRAMES = int(LOOP_SECS * FPS)


def clamp(x: float, lo: float = 0.0, hi: float = 1.0) -> float:
    return lo if x < lo else hi if x > hi else x


def v_add(a, b):
    return (a[0] + b[0], a[1] + b[1], a[2] + b[2])


def v_sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def v_mul(a, s):
    return (a[0] * s, a[1] * s, a[2] * s)


def v_dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def v_len(a):
    return math.sqrt(max(1e-12, v_dot(a, a)))


def v_norm(a):
    l = v_len(a)
    return (a[0] / l, a[1] / l, a[2] / l)


def v_cross(a, b):
    return (
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    )


def look_at(eye, target, up=(0.0, 1.0, 0.0)):
    f = v_norm(v_sub(target, eye))
    s = v_norm(v_cross(f, up))
    u = v_cross(s, f)
    # world -> camera
    return s, u, f, eye


def project(p, cam, fov_deg=52.0):
    s, u, f, eye = cam
    w = v_sub(p, eye)
    x = v_dot(w, s)
    y = v_dot(w, u)
    z = v_dot(w, f)
    if z < 0.15:
        return None
    fovy = math.radians(fov_deg)
    ay = 1.0 / math.tan(fovy * 0.5)
    ax = ay * HEIGHT / WIDTH
    ndc_x = (x / z) * ax
    ndc_y = (y / z) * ay
    sx = (ndc_x * 0.5 + 0.5) * WIDTH
    sy = (1.0 - (ndc_y * 0.5 + 0.5)) * HEIGHT
    return sx, sy, z


def write_png(path: Path, rgba: bytearray, w: int, h: int) -> None:
    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    raw = bytearray()
    stride = w * 4
    for y in range(h):
        raw.append(0)
        raw.extend(rgba[y * stride : (y + 1) * stride])
    compressed = zlib.compress(bytes(raw), 6)
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", compressed) + chunk(
        b"IEND", b""
    )
    path.write_bytes(png)


def shade(albedo, metallic, roughness, n, p, eye, lights):
    v = v_norm(v_sub(eye, p))
    col = [albedo[0] * 0.07, albedo[1] * 0.08, albedo[2] * 0.1]
    for Ldir, Lcol, Lint, kind, origin, rng in lights:
        if kind == "sun":
            l = v_norm(v_mul(Ldir, -1.0))
            atten = 1.0
        else:
            to_l = v_sub(origin, p)
            dist = v_len(to_l)
            l = v_norm(to_l)
            atten = clamp(1.0 - dist / rng) ** 2
        ndotl = max(0.0, v_dot(n, l))
        h = v_norm(v_add(l, v))
        ndoth = max(0.0, v_dot(n, h))
        spec = (ndoth ** (max(4.0, 64.0 * (1.0 - roughness)))) * (0.15 + 0.85 * metallic)
        diff = ndotl * (1.0 - metallic * 0.85)
        col[0] += (albedo[0] * diff + spec) * Lcol[0] * Lint * atten
        col[1] += (albedo[1] * diff + spec) * Lcol[1] * Lint * atten
        col[2] += (albedo[2] * diff + spec) * Lcol[2] * Lint * atten
    return (clamp(col[0]), clamp(col[1]), clamp(col[2]))


def draw_quad(buf, zbuf, cam, corners, albedo, metallic, roughness, lights, eye):
    n = v_norm(v_cross(v_sub(corners[1], corners[0]), v_sub(corners[2], corners[0])))
    center = v_mul(
        v_add(v_add(corners[0], corners[1]), v_add(corners[2], corners[3])), 0.25
    )
    if v_dot(n, v_sub(eye, center)) <= 0:
        return
    pts = []
    for c in corners:
        pr = project(c, cam)
        if pr is None:
            return
        pts.append(pr)
    # Flat shade once per face (CPU trailer path).
    r, g, b = shade(albedo, metallic, roughness, n, center, eye, lights)
    bloom = max(0.0, (r + g + b) / 3.0 - 0.75) * 0.35
    color = (
        int(clamp(r + bloom) * 255),
        int(clamp(g + bloom * 0.9) * 255),
        int(clamp(b + bloom * 0.7) * 255),
    )
    for tri in ((0, 1, 2), (0, 2, 3)):
        raster_tri_flat(buf, zbuf, [pts[i] for i in tri], color)


def raster_tri_flat(buf, zbuf, pts, color):
    (x0, y0, z0), (x1, y1, z1), (x2, y2, z2) = pts
    minx = max(0, int(min(x0, x1, x2)))
    maxx = min(WIDTH - 1, int(max(x0, x1, x2)))
    miny = max(0, int(min(y0, y1, y2)))
    maxy = min(HEIGHT - 1, int(max(y0, y1, y2)))
    cr, cg, cb = color
    for y in range(miny, maxy + 1):
        for x in range(minx, maxx + 1):
            bc = barycentric(x0, y0, x1, y1, x2, y2, x + 0.5, y + 0.5)
            if bc is None:
                continue
            w, u, v = bc
            if w < 0 or u < 0 or v < 0:
                continue
            z = w * z0 + u * z1 + v * z2
            i = y * WIDTH + x
            if z >= zbuf[i]:
                continue
            zbuf[i] = z
            o = i * 4
            buf[o] = cr
            buf[o + 1] = cg
            buf[o + 2] = cb
            buf[o + 3] = 255


def barycentric(ax, ay, bx, by, cx, cy, px, py):
    v0x, v0y = bx - ax, by - ay
    v1x, v1y = cx - ax, cy - ay
    v2x, v2y = px - ax, py - ay
    den = v0x * v1y - v1x * v0y
    if abs(den) < 1e-8:
        return None
    inv = 1.0 / den
    u = (v2x * v1y - v1x * v2y) * inv
    v = (v0x * v2y - v2x * v0y) * inv
    w = 1.0 - u - v
    return w, u, v

def cube_faces(center, scale, yaw=0.0):
    hx, hy, hz = scale[0] * 0.5, scale[1] * 0.5, scale[2] * 0.5
    cy, sy = math.cos(yaw), math.sin(yaw)

    def rot(p):
        x, y, z = p
        return (x * cy - z * sy, y, x * sy + z * cy)

    def t(p):
        r = rot(p)
        return (r[0] + center[0], r[1] + center[1], r[2] + center[2])

    # 8 corners
    c = [
        t((-hx, -hy, -hz)),
        t((hx, -hy, -hz)),
        t((hx, hy, -hz)),
        t((-hx, hy, -hz)),
        t((-hx, -hy, hz)),
        t((hx, -hy, hz)),
        t((hx, hy, hz)),
        t((-hx, hy, hz)),
    ]
    faces = [
        [c[0], c[1], c[2], c[3]],  # -Z
        [c[5], c[4], c[7], c[6]],  # +Z
        [c[4], c[0], c[3], c[7]],  # -X
        [c[1], c[5], c[6], c[2]],  # +X
        [c[3], c[2], c[6], c[7]],  # +Y
        [c[4], c[5], c[1], c[0]],  # -Y
    ]
    return faces


def draw_particles(buf, zbuf, cam, origin, count, color, phase, spread):
    # deterministic pseudo-particles
    for i in range(count):
        seed = i * 17.13 + phase * 3.1
        ang = seed * 1.7
        elev = math.sin(seed * 2.3) * 0.5 + 0.5
        rad = (0.2 + (seed % 1.0) * spread) * (0.4 + elev)
        p = (
            origin[0] + math.cos(ang) * rad,
            origin[1] + elev * 1.2 + math.sin(phase + i) * 0.05,
            origin[2] + math.sin(ang) * rad,
        )
        pr = project(p, cam)
        if pr is None:
            continue
        sx, sy, z = pr
        size = max(1, int(3.5 / z))
        r, g, b = color
        for dy in range(-size, size + 1):
            for dx in range(-size, size + 1):
                if dx * dx + dy * dy > size * size:
                    continue
                x = int(sx) + dx
                y = int(sy) + dy
                if x < 0 or y < 0 or x >= WIDTH or y >= HEIGHT:
                    continue
                i = y * WIDTH + x
                if z >= zbuf[i]:
                    continue
                zbuf[i] = z
                o = i * 4
                buf[o] = int(r * 255)
                buf[o + 1] = int(g * 255)
                buf[o + 2] = int(b * 255)
                buf[o + 3] = 255


def render_frame(t: float) -> bytearray:
    phase = (t % LOOP_SECS) / LOOP_SECS
    angle = phase * math.tau
    radius = 6.4
    height = 2.6 + math.sin(angle) * 0.25
    eye = (math.cos(angle) * radius, height, math.sin(angle) * radius)
    target = (0.0, 0.75, 0.0)
    cam = look_at(eye, target)

    pulse = 1.0 + math.sin(angle) * 0.25
    lights = [
        (("sun"), (-0.45, -1.0, -0.25), (1.0, 0.96, 0.9), 1.4, None, None),
    ]
    # rewrite lights as tuples (dir, col, intensity, kind, origin, range)
    lights = [
        ((-0.45, -1.0, -0.25), (1.0, 0.96, 0.9), 1.4, "sun", None, 0.0),
        ((0, 0, 0), (1.0, 0.55, 0.22), 2.4, "point", (1.8, 2.0, 0.6), 9.0),
        ((0, 0, 0), (0.3, 0.5, 1.0), 1.8, "point", (-2.0, 1.6, -0.8), 8.0),
        ((0, 0, 0), (0.55, 1.0, 0.7), 1.2 * pulse, "point", (0.2, 1.2, 2.2), 6.0),
    ]

    buf = bytearray(WIDTH * HEIGHT * 4)
    # Atmospheric charcoal gradient (matches site / showcase clear color).
    for y in range(HEIGHT):
        v = y / max(1, HEIGHT - 1)
        r = int(9 + v * 18)
        g = int(10 + v * 16)
        b = int(14 + v * 22)
        row = y * WIDTH * 4
        for x in range(WIDTH):
            o = row + x * 4
            buf[o] = r
            buf[o + 1] = g
            buf[o + 2] = b
            buf[o + 3] = 255
            # subtle warm key light falloff top-left
            fall = max(0.0, 1.0 - ((x / WIDTH - 0.25) ** 2 + (y / HEIGHT - 0.2) ** 2) * 2.2)
            buf[o] = min(255, buf[o] + int(28 * fall))
            buf[o + 1] = min(255, buf[o + 1] + int(22 * fall))
            buf[o + 2] = min(255, buf[o + 2] + int(8 * fall))
    zbuf = [1e9] * (WIDTH * HEIGHT)

    yaw = 0.4 * t
    solids = [
        # back / side walls + plinth + material cubes (floor = gradient backdrop)
        ((0.0, 1.6, -3.4), (7.0, 3.2, 0.18), 0.0, (0.14, 0.15, 0.18), 0.0, 0.95),
        ((-3.5, 1.4, -0.8), (0.18, 2.8, 5.0), 0.0, (0.16, 0.17, 0.2), 0.0, 0.92),
        ((0.0, 0.2, 0.0), (4.2, 0.4, 2.4), 0.0, (0.28, 0.29, 0.32), 0.15, 0.55),
        ((-1.5, 0.85, 0.0), (1.0, 1.0, 1.0), yaw, (0.88, 0.18, 0.14), 0.0, 0.22),
        ((0.0, 0.85, 0.0), (1.0, 1.0, 1.0), yaw, (0.92, 0.9, 0.85), 1.0, 0.32),
        ((1.5, 0.85, 0.0), (1.0, 1.0, 1.0), yaw, (0.95, 0.96, 0.98), 1.0, 0.06),
        ((0.0, 0.85, 1.35), (1.0, 1.0, 1.0), yaw, (0.12, 0.52, 0.95), 0.0, 0.1),
    ]
    bob = 2.0 + math.sin(angle * 2.0) * 0.15
    orb_z = 0.6 + math.cos(angle) * 0.1
    solids.append(
        ((1.8, bob, orb_z), (0.28, 0.28, 0.28), yaw * 3.0, (1.0, 0.75, 0.25), 0.4, 0.18)
    )

    # painter's-ish: draw larger/farther first by sorting centers by depth
    decorated = []
    for center, scale, y, albedo, metal, rough in solids:
        pr = project(center, cam)
        depth = pr[2] if pr else 1e9
        decorated.append((depth, center, scale, y, albedo, metal, rough))
    decorated.sort(key=lambda x: -x[0])

    for _, center, scale, y, albedo, metal, rough in decorated:
        for face in cube_faces(center, scale, y):
            draw_quad(buf, zbuf, cam, face, albedo, metal, rough, lights, eye)

    # particle beats matching showcase cadence
    draw_particles(buf, zbuf, cam, (1.8, 1.9, 0.6), 22, (1.0, 0.65, 0.25), t * 3.5, 1.1)
    draw_particles(buf, zbuf, cam, (-2.0, 1.5, -0.8), 14, (0.4, 0.6, 1.0), t * 2.2 + 0.12, 0.9)
    if int(t / 4.0) >= 0:
        draw_particles(buf, zbuf, cam, (0.0, 0.15, 0.0), 28, (0.7, 0.75, 0.85), t * 0.5, 1.4)

    return buf


def main():
    out = Path(__file__).resolve().parents[1] / "target" / "showcase-frames"
    out.mkdir(parents=True, exist_ok=True)
    print(f"soft-rendering {FRAMES} frames → {out}", flush=True)
    for i in range(FRAMES):
        t = i / FPS
        buf = render_frame(t)
        write_png(out / f"frame_{i:05d}.png", buf, WIDTH, HEIGHT)
        if (i + 1) % 40 == 0 or i + 1 == FRAMES:
            print(f"  {i + 1}/{FRAMES}", flush=True)
    print("done", flush=True)


if __name__ == "__main__":
    main()
