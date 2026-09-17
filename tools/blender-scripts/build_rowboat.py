"""Build the rowboat mount and its inventory icon in Blender.

    /Applications/Blender.app/Contents/MacOS/Blender -b \
        -P tools/blender-scripts/build_rowboat.py

The origin sits at the waterline, not the keel, so the client can place the
boat straight onto the sampled water surface. Bow points -Y (Blender forward).
"""
import math
import sys
from pathlib import Path

import bpy

sys.path.insert(0, str(Path(__file__).resolve().parent))
from icon_render import add_light, principled, render_icon

ROOT = Path(__file__).resolve().parents[2]

LENGTH = 3.4
HALF_BEAM = 0.62
DEPTH = 0.55
DRAFT = 0.15
PLANK = 0.05
STATIONS = 37
RIB_POINTS = 19
# Floorboards, just clear of the waterline. Without them the game's water
# plane cuts through the hull and pools inside the boat.
SOLE_Z = 0.06


def smoothstep(t):
    t = min(max(t, 0.0), 1.0)
    return t * t * (3.0 - 2.0 * t)


def half_beam(t):
    """Stem, widest amidships, flat transom. Full in the ends or it reads
    as a canoe rather than a dinghy."""
    if t < 0.5:
        return HALF_BEAM * (0.10 + 0.90 * smoothstep(t / 0.5) ** 0.6)
    return HALF_BEAM * (1.0 - 0.38 * smoothstep((t - 0.5) / 0.5) ** 1.4)


def keel_z(t):
    return -DRAFT + 0.22 * abs(2.0 * t - 1.0) ** 2.6


def rim_z(t):
    return DEPTH - DRAFT + 0.13 * (2.0 * t - 1.0) ** 2


def interior_half_width(t, z):
    """Half the inner hull width at height `z`, or None if the sole would
    sit below the keel here (the rockered ends)."""
    hb = max(half_beam(t) - PLANK, 0.004)
    bottom = keel_z(t) + PLANK
    top = rim_z(t)
    if z <= bottom or z >= top or top - bottom < 1e-6:
        return None
    ratio = (z - bottom) / (top - bottom)
    a = math.acos(min(max(1.0 - ratio ** (1.0 / 0.85), -1.0), 1.0))
    return hb * math.sin(a) ** 0.8


def rib(t, inset):
    """One cross section, port rim over the keel to starboard rim."""
    hb = max(half_beam(t) - inset, 0.004)
    bottom = keel_z(t) + (inset if inset else 0.0)
    top = rim_z(t)
    points = []
    for j in range(RIB_POINTS):
        a = (j / (RIB_POINTS - 1) - 0.5) * math.pi
        x = hb * math.copysign(abs(math.sin(a)) ** 0.8, a)
        z = bottom + (top - bottom) * (1.0 - math.cos(a)) ** 0.85
        points.append((x, LENGTH * (t - 0.5), z))
    return points


def build_hull(material):
    verts = []
    for inset in (0.0, PLANK):
        for i in range(STATIONS):
            verts.extend(rib(i / (STATIONS - 1), inset))
    shell = STATIONS * RIB_POINTS

    def outer(i, j):
        return i * RIB_POINTS + j

    def inner(i, j):
        return shell + i * RIB_POINTS + j

    faces = []
    for i in range(STATIONS - 1):
        for j in range(RIB_POINTS - 1):
            faces.append((outer(i, j), outer(i, j + 1), outer(i + 1, j + 1), outer(i + 1, j)))
            faces.append((inner(i, j), inner(i, j + 1), inner(i + 1, j + 1), inner(i + 1, j)))
        for j in (0, RIB_POINTS - 1):
            faces.append((outer(i, j), outer(i + 1, j), inner(i + 1, j), inner(i, j)))
    for i in (0, STATIONS - 1):
        for j in range(RIB_POINTS - 1):
            faces.append((outer(i, j), inner(i, j), inner(i, j + 1), outer(i, j + 1)))

    mesh = bpy.data.meshes.new('Hull')
    mesh.from_pydata(verts, [], faces)
    mesh.materials.append(material)
    obj = bpy.data.objects.new('Hull', mesh)
    bpy.context.collection.objects.link(obj)

    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    bpy.ops.object.mode_set(mode='EDIT')
    bpy.ops.mesh.select_all(action='SELECT')
    bpy.ops.mesh.normals_make_consistent(inside=False)
    bpy.ops.object.mode_set(mode='OBJECT')
    bpy.ops.object.shade_smooth()
    obj.select_set(False)
    return obj


def build_sole(material):
    """Planking laid across the ribs, hiding the water the hull sits in."""
    verts = []
    spans = []
    for i in range(STATIONS):
        t = i / (STATIONS - 1)
        half = interior_half_width(t, SOLE_Z)
        if half is None:
            spans.append(None)
            continue
        y = LENGTH * (t - 0.5)
        spans.append(len(verts))
        verts.append((-half, y, SOLE_Z))
        verts.append((half, y, SOLE_Z))

    faces = []
    for a, b in zip(spans, spans[1:]):
        if a is None or b is None:
            continue
        faces.append((a, a + 1, b + 1, b))

    mesh = bpy.data.meshes.new('Sole')
    mesh.from_pydata(verts, [], faces)
    mesh.materials.append(material)
    obj = bpy.data.objects.new('Sole', mesh)
    bpy.context.collection.objects.link(obj)
    return obj


def add_box(name, location, scale, material):
    bpy.ops.mesh.primitive_cube_add(size=2.0, location=location)
    obj = bpy.context.object
    obj.name = name
    obj.data.name = name
    obj.scale = scale
    obj.data.materials.append(material)
    return obj


def add_thwart(name, t, material):
    y = LENGTH * (t - 0.5)
    return add_box(
        name,
        (0.0, y, keel_z(t) + 0.30),
        (half_beam(t) - PLANK * 0.5, 0.075, 0.022),
        material,
    )


def add_oar(name, side, material):
    """Shipped along the gunwale — the client rows it from its own node.
    Built along +Z (the cylinder's own axis), then laid down onto -Y."""
    oar_len = 2.2
    bpy.ops.mesh.primitive_cylinder_add(radius=0.026, depth=oar_len, location=(0, 0, 0))
    shaft = bpy.context.object
    shaft.data.materials.append(material)
    blade = add_box('OarBlade', (0.0, 0.0, oar_len * 0.42), (0.075, 0.012, 0.30), material)
    bpy.ops.object.select_all(action='DESELECT')
    shaft.select_set(True)
    blade.select_set(True)
    bpy.context.view_layer.objects.active = shaft
    bpy.ops.object.join()
    shaft.name = name
    shaft.data.name = name
    shaft.rotation_euler = (math.radians(90.0), 0.0, side * math.radians(6.0))
    shaft.location = (side * (HALF_BEAM - 0.14), -0.25, DEPTH - DRAFT - 0.14)
    return shaft


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    oak = principled('BoatOak', (0.16, 0.086, 0.043, 1), 0.78)
    trim = principled('BoatTrim', (0.28, 0.17, 0.09, 1), 0.62)

    hull = build_hull(oak)
    parts = [hull]
    parts.append(build_sole(trim))
    parts.append(add_thwart('ThwartBow', 0.30, trim))
    parts.append(add_thwart('ThwartMid', 0.50, trim))
    parts.append(add_thwart('ThwartStern', 0.72, trim))
    oars = [add_oar('OarPort', -1.0, trim), add_oar('OarStarboard', 1.0, trim)]

    seat = bpy.data.objects.new('RideSeat', None)
    seat.empty_display_size = 0.15
    seat.location = (0.0, LENGTH * (0.72 - 0.5), keel_z(0.72) + 0.32)
    bpy.context.collection.objects.link(seat)

    exported = parts + oars
    bpy.ops.object.select_all(action='DESELECT')
    for obj in exported:
        obj.select_set(True)
    seat.select_set(True)
    bpy.context.view_layer.objects.active = hull
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)

    source = ROOT / 'assets/rowboat'
    source.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=str(source / 'rowboat.blend'))
    bpy.ops.export_scene.gltf(
        filepath=str(ROOT / 'client/public/models/mounts/rowboat.glb'),
        export_format='GLB',
        use_selection=True,
        export_animations=False,
    )

    world = bpy.data.worlds.new('World')
    world.use_nodes = True
    world.node_tree.nodes['Background'].inputs[0].default_value = (0.3, 0.3, 0.3, 1)
    bpy.context.scene.world = world
    add_light((0.8, -1.4, 2.0), 260, 2.0)
    add_light((-1.6, -0.8, 1.2), 110, 2.4)
    add_light((0.0, 1.8, 1.2), 90, 2.4)
    render_icon(
        exported,
        str(ROOT / 'client/public/items/objects/rowboat.png'),
        (math.radians(-32), 0.0, math.radians(-40)),
        1.06,
    )


main()
