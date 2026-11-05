"""Scene setup: cameras (legacy and engine), lights, render settings, outlines,
compositor grade, and the legacy top trim."""

import math
from math import radians, sqrt

import bpy

from .palettes import EEVEE, GRADE, LIGHTING, OUTLINE


def clear_scene():
    bpy.ops.object.select_all(action='SELECT'); bpy.ops.object.delete()
    for coll in (bpy.data.meshes, bpy.data.materials, bpy.data.lights, bpy.data.cameras, bpy.data.images):
        for d in list(coll):
            if d.users == 0: coll.remove(d)
    for c in list(bpy.data.collections):
        if not c.objects and not c.children: bpy.data.collections.remove(c)


def setup_camera(G, cells, scale, height_factor=1.0):
    sc = bpy.context.scene
    cam_d = bpy.data.cameras.new("IsoCam"); cam_d.type = 'ORTHO'
    cam = bpy.data.objects.new("IsoCam", cam_d); sc.collection.objects.link(cam); sc.camera = cam
    cam.rotation_euler = (radians(60), 0, radians(45))
    cam.location = (18.4, -18.4, 15.0)
    sc.render.resolution_x = int(cells * 64 * scale)
    sc.render.resolution_y = int(cells * 64 * scale * height_factor)
    sc.render.resolution_percentage = 100
    diamond_w = G * sqrt(2)                      # world width of the ground diamond on screen
    cam_d.ortho_scale = diamond_w * max(1.0, height_factor)  # ortho_scale spans the larger dimension
    # vertical placement: put the lowest point of the tile a few px above the bottom edge
    bpy.context.view_layer.update()
    from bpy_extras.object_utils import world_to_camera_view
    from mathutils import Vector
    lowest = world_to_camera_view(sc, cam, Vector((G / 2, -G / 2, -0.45))).y  # front corner, bottom of earth slab
    pad = 6.0 / sc.render.resolution_y
    cam_d.shift_y = (lowest - pad) / max(1.0, height_factor)    # positive shift moves the scene down in frame
    return cam


# Engine sprite format (see calc_object_iso_coords in crates/game/src/tile/mod.rs):
# - the sprite is centred on the footprint using its logical width, so the PNG is
#   exactly G * 64 * density px wide and the diamond's left and right corners touch
#   the frame's sides;
# - its bottom edge sits on the footprint's bottom vertex, so the front corner of the
#   footprint at z = 0, world (G/2, -G/2, 0), is the bottom-centre of the frame.
# draw_size is the PNG size / density. Anything below z = 0 is cut off.
DEFAULT_DENSITY = 8          # PNG px per logical px (placeholder-art plan, Q1): about the hand-made 2x2 art's 7.6
TOP_MARGIN_PX = 2            # empty logical px kept above the model


def model_extents(G, collection):
    """Screen-space extents of the model, in world units, relative to the anchor:
    (up, half-width) as seen by the isometric camera."""
    from mathutils import Euler, Vector
    R = Euler((radians(60), 0, radians(45))).to_matrix()
    up, right = R @ Vector((0, 1, 0)), R @ Vector((1, 0, 0))
    anchor = Vector((G / 2, -G / 2, 0))
    bpy.context.view_layer.update()
    max_up, max_side = 0.0, 0.0
    for o in collection.all_objects:
        if o.type != 'MESH': continue
        mw = o.matrix_world
        for v in o.data.vertices:
            d = mw @ v.co - anchor
            max_up = max(max_up, d.dot(up)); max_side = max(max_side, abs(d.dot(right)))
    return max_up, max_side


def setup_camera_engine(G, density, collection, height_px=None):
    """Orthographic camera whose frame matches the engine's sprite anchor exactly.
    The height is measured from the model and rounded up to whole logical px;
    height_px forces a taller frame (so a tile's variations share one size).
    Returns (camera, width_px, height_px)."""
    from mathutils import Euler, Vector
    sc = bpy.context.scene
    cam_d = bpy.data.cameras.new("IsoCam"); cam_d.type = 'ORTHO'; cam_d.sensor_fit = 'HORIZONTAL'
    cam = bpy.data.objects.new("IsoCam", cam_d); sc.collection.objects.link(cam); sc.camera = cam
    rot = Euler((radians(60), 0, radians(45))); cam.rotation_euler = rot
    R = rot.to_matrix(); up, fwd = R @ Vector((0, 1, 0)), R @ Vector((0, 0, -1))
    width_px = int(round(G * 64 * density))
    cam_d.ortho_scale = G * sqrt(2)                          # the diamond spans the full width
    world_per_px = cam_d.ortho_scale / width_px
    max_up, max_side = model_extents(G, collection)
    if max_side > cam_d.ortho_scale / 2 + 1e-4:
        print(f"WARNING: model is {max_side:.3f} wide of the anchor; the frame holds {cam_d.ortho_scale / 2:.3f}. The sides get clipped.")
    needed_px = (math.ceil(max_up / world_per_px / density) + TOP_MARGIN_PX) * density
    if height_px is None:
        height_px = needed_px
    elif height_px < needed_px or height_px % density:
        raise ValueError(f"height_px {height_px} must be a multiple of {density} and at least {needed_px}")
    sc.render.resolution_x, sc.render.resolution_y, sc.render.resolution_percentage = width_px, height_px, 100
    sc.render.pixel_aspect_x = sc.render.pixel_aspect_y = 1.0
    cam_d.shift_x = cam_d.shift_y = 0.0
    dist = 40.0 + 4.0 * G
    anchor = Vector((G / 2, -G / 2, 0))
    cam.location = anchor + up * (height_px * world_per_px / 2) - fwd * dist   # anchor on the bottom edge's centre
    cam_d.clip_start, cam_d.clip_end = 0.1, dist * 2 + 20
    return cam, width_px, height_px


def setup_lighting():
    L = LIGHTING; sc = bpy.context.scene
    sun_d = bpy.data.lights.new("Sun", 'SUN'); sun_d.energy = L["sun_energy"]; sun_d.color = L["sun_color"]; sun_d.angle = radians(L["sun_angle"])
    sun = bpy.data.objects.new("Sun", sun_d); sc.collection.objects.link(sun); sun.rotation_euler = [radians(a) for a in L["sun_rot"]]
    fill_d = bpy.data.lights.new("Fill", 'SUN'); fill_d.energy = L["fill_energy"]; fill_d.color = L["fill_color"]
    fill = bpy.data.objects.new("Fill", fill_d); sc.collection.objects.link(fill); fill.rotation_euler = [radians(a) for a in L["fill_rot"]]
    world = sc.world or bpy.data.worlds.new("World"); sc.world = world; world.use_nodes = True
    bg = world.node_tree.nodes.get("Background"); bg.inputs[0].default_value = (*L["world_color"], 1); bg.inputs[1].default_value = L["world_strength"]


def setup_render(samples=None):
    sc = bpy.context.scene
    for eng in ("BLENDER_EEVEE", "BLENDER_EEVEE_NEXT"):
        try: sc.render.engine = eng; break
        except TypeError: pass
    sc.render.film_transparent = True
    sc.render.image_settings.file_format = 'PNG'; sc.render.image_settings.color_mode = 'RGBA'; sc.render.image_settings.color_depth = '8'
    sc.view_settings.view_transform = 'Standard'; sc.view_settings.look = 'None'
    ev = sc.eevee
    for k, v in (("taa_render_samples", samples or EEVEE["samples"]), ("use_shadows", True), ("use_fast_gi", True), ("fast_gi_distance", EEVEE["fast_gi_distance"]), ("shadow_ray_count", 2), ("shadow_step_count", 4)):
        if hasattr(ev, k): setattr(ev, k, v)


def setup_outlines(px_scale=1.0, wobble_scale=None):
    """px_scale multiplies the line thickness, for renders at more than one pixel
    per logical pixel; wobble_scale (default px_scale) multiplies the noise and jitter."""
    O = dict(OUTLINE); sc = bpy.context.scene
    # The scene's line thickness multiplies the line style's, so only the style's
    # measures scale: the legacy renders get 1.6 x 1.6 = 2.56 px lines.
    sc.render.use_freestyle = True; sc.render.line_thickness_mode = 'ABSOLUTE'; sc.render.line_thickness = O["thickness"]
    w = px_scale if wobble_scale is None else wobble_scale
    O["thickness"] *= px_scale
    for k in ("noise_amp", "jitter_amp", "noise_period"): O[k] *= w
    fs = bpy.context.view_layer.freestyle_settings; fs.use_culling = True; fs.crease_angle = radians(120)
    for ls in list(fs.linesets): fs.linesets.remove(ls)
    ls = fs.linesets.new("Ink"); ls.select_silhouette = True; ls.select_border = True; ls.select_crease = True; ls.select_contour = False
    st = ls.linestyle; st.color = O["color"]; st.thickness = O["thickness"]; st.alpha = O["alpha"]
    m = st.thickness_modifiers.new(name="Noise", type='NOISE'); m.amplitude = O["noise_amp"]; m.period = O["noise_period"]; m.seed = 3
    g = st.geometry_modifiers.new(name="Jitter", type='PERLIN_NOISE_1D'); g.amplitude = O["jitter_amp"]; g.frequency = O["jitter_freq"]; g.octaves = 2


def setup_grade():
    # Blender 5.x compositor: tree lives on scene.compositing_node_group and
    # outputs through NodeGroupOutput (the old CompositorNodeComposite is gone).
    sc = bpy.context.scene
    ct = getattr(sc, "compositing_node_group", None)
    if ct is None:
        ct = bpy.data.node_groups.new("Comp", "CompositorNodeTree"); sc.compositing_node_group = ct
    for n in list(ct.nodes): ct.nodes.remove(n)
    if not ct.interface.items_tree: ct.interface.new_socket("Image", in_out='OUTPUT', socket_type='NodeSocketColor')
    rl = ct.nodes.new("CompositorNodeRLayers"); outn = ct.nodes.new("NodeGroupOutput")
    hs = ct.nodes.new("CompositorNodeHueSat"); hs.inputs["Saturation"].default_value = GRADE["saturation"]
    bc = ct.nodes.new("CompositorNodeBrightContrast"); bc.inputs["Contrast"].default_value = GRADE["contrast"]
    ct.links.new(rl.outputs["Image"], hs.inputs["Image"]); ct.links.new(hs.outputs[0], bc.inputs["Image"]); ct.links.new(bc.outputs[0], outn.inputs[0])
    sc.render.use_compositing = True


def trim_top(path, keep_px=4):
    """Crop fully-transparent rows from the top of a rendered PNG, leaving keep_px of margin.
    Width and the bottom anchor are untouched, so the diamond still maps to the grid."""
    import numpy as np
    im = bpy.data.images.load(path); W, H = im.size
    px = np.array(im.pixels[:], dtype=np.float32).reshape(H, W, 4)      # row 0 = bottom
    rows = np.where((px[:, :, 3] > 0.002).any(axis=1))[0]
    top = min(H, int(rows.max()) + 1 + keep_px) if len(rows) else H
    cropped = px[:top]
    out = bpy.data.images.new("trimmed", W, top, alpha=True, float_buffer=False)
    out.colorspace_settings.name = im.colorspace_settings.name
    out.pixels.foreach_set(cropped.ravel())
    out.file_format = 'PNG'; out.filepath_raw = path; out.save()
    bpy.data.images.remove(im); bpy.data.images.remove(out)
    return W, top
