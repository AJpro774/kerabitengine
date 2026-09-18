"""Join Sketchfab glTFs into Kerabit-lite single-mesh GLBs (headless Blender)."""

import math
import os

import bpy

from mathutils import Matrix

ROOT = os.path.dirname(os.path.abspath(__file__))
RAW = os.path.join(ROOT, "assets", "raw")
OUT = os.path.join(ROOT, "assets")

# name, target_size (largest dim after align), sit_on_floor, align mode
JOBS = [
    ("crate", 1.0, True, "upright"),
    ("barrel", 1.05, True, "upright"),
    ("dummy", 1.7, True, "upright"),
    ("pistol", 0.32, False, "pistol"),
]


def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def import_gltf(path):
    bpy.ops.import_scene.gltf(filepath=path)


def mesh_objects():
    return [o for o in bpy.context.scene.objects if o.type == "MESH"]


def flatten_and_join():
    for o in list(bpy.context.scene.objects):
        mw = o.matrix_world.copy()
        o.parent = None
        o.matrix_world = mw
    for o in list(bpy.context.scene.objects):
        if o.type != "MESH":
            bpy.data.objects.remove(o, do_unlink=True)
    objs = mesh_objects()
    if not objs:
        raise RuntimeError("no meshes")
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.hide_set(False)
        o.hide_viewport = False
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    if len(objs) > 1:
        bpy.ops.object.join()
    return bpy.context.view_layer.objects.active


def apply_trs(obj):
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    obj.rotation_mode = "XYZ"
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)


def rotate_world(obj, axis, degrees):
    obj.matrix_world = Matrix.Rotation(math.radians(degrees), 4, axis) @ obj.matrix_world
    apply_trs(obj)


def make_upright(obj):
    """Rotate so the longest axis becomes Blender Z (glTF Y / Kerabit up)."""
    dims = obj.dimensions
    if dims.z >= dims.x - 1e-4 and dims.z >= dims.y - 1e-4:
        return
    if dims.y >= dims.x:
        rotate_world(obj, "X", 90.0)
    else:
        rotate_world(obj, "Y", -90.0)


def make_pistol_forward(obj):
    """Longest axis (barrel) → Blender +Y, which glTF export maps to Kerabit -Z."""
    dims = obj.dimensions
    if dims.y >= dims.x and dims.y >= dims.z:
        return
    if dims.x >= dims.z:
        rotate_world(obj, "Z", -90.0)
    else:
        rotate_world(obj, "X", 90.0)


def single_primitive_material(obj):
    """Kerabit load_gltf only keeps the first primitive (one material)."""
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.mode_set(mode="OBJECT")
    for poly in obj.data.polygons:
        poly.material_index = 0
    while len(obj.material_slots) > 1:
        obj.active_material_index = len(obj.material_slots) - 1
        bpy.ops.object.material_slot_remove()


def pack(name, target_size, sit_on_floor, align):
    reset()
    src = os.path.join(RAW, name, "scene.gltf")
    dst = os.path.join(OUT, f"{name}.glb")
    import_gltf(src)
    obj = flatten_and_join()
    apply_trs(obj)

    if align == "upright":
        make_upright(obj)
    elif align == "pistol":
        make_pistol_forward(obj)

    bpy.ops.object.origin_set(type="ORIGIN_GEOMETRY", center="BOUNDS")
    dims = obj.dimensions
    largest = max(dims.x, dims.y, dims.z)
    if largest > 1e-8:
        s = target_size / largest
        obj.scale = (s, s, s)
        apply_trs(obj)

    if sit_on_floor:
        zs = [(obj.matrix_world @ v.co).z for v in obj.data.vertices]
        obj.location.z -= min(zs)
        apply_trs(obj)
        bpy.context.scene.cursor.location = (0.0, 0.0, 0.0)
        bpy.ops.object.origin_set(type="ORIGIN_CURSOR")
        obj.location = (0.0, 0.0, 0.0)
        apply_trs(obj)

    single_primitive_material(obj)

    for img in bpy.data.images:
        if img.size[0] > 1024 or img.size[1] > 1024:
            img.scale(min(img.size[0], 1024), min(img.size[1], 1024))

    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.export_scene.gltf(
        filepath=dst,
        export_format="GLB",
        use_selection=True,
        export_apply=True,
        export_texcoords=True,
        export_normals=True,
        export_materials="EXPORT",
        export_image_format="JPEG",
        export_jpeg_quality=80,
        export_animations=False,
        export_skins=False,
        export_morph=False,
    )
    nverts = len(obj.data.vertices)
    print(f"packed {name}: verts={nverts} dims={tuple(round(d,3) for d in obj.dimensions)} -> {dst}")


def main():
    os.makedirs(OUT, exist_ok=True)
    for name, size, floor, align in JOBS:
        pack(name, size, floor, align)


main()
