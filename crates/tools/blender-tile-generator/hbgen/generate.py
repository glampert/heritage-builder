"""Entry points: generate() for the legacy fortress studies, generate_engine()
for game tiles in the engine's sprite format."""

import os

import bpy

from .builder import Builder
from .materials import Materials
from .palettes import PALETTES
from .recipes import RECIPES
from .scene import (DEFAULT_DENSITY, clear_scene, setup_camera, setup_camera_engine, setup_grade,
                    setup_lighting, setup_outlines, setup_render, trim_top)

# Outline thickness per logical px for engine renders. The legacy 1.6 px was tuned
# for 1x fortress renders, where a 15-cell tile shrinks a lot on screen; game tiles
# are drawn near 1:1, so their ink is thinner per logical px.
ENGINE_OUTLINE_SCALE = 0.4


def _render_to(out):
    sc = bpy.context.scene
    sc.render.filepath = os.path.abspath(os.path.expanduser(out))
    os.makedirs(os.path.dirname(sc.render.filepath), exist_ok=True)
    bpy.ops.render.render(write_still=True)
    return sc.render.filepath


def generate_engine(building, palette="viet", density=DEFAULT_DENSITY, out=None, blend=None, render=True,
                    params=None, seed=0, samples=None, height_px=None):
    """Render a tile in the engine's sprite format. Returns the PNG size, the
    logical_size and the draw_size for tile_set.json (PNG size / density).
    params go to the recipe; seed seeds B.rng; samples overrides Eevee's (for --fast);
    height_px forces the frame's height (see setup_camera_engine)."""
    clear_scene()
    mats = Materials(PALETTES[palette])
    B = Builder(mats, collection_name=building, seed=seed)
    G = RECIPES[building](B, **(params or {}))
    # Ink: thickness scales with density, but the wobble stays small so the lines stay crisp.
    setup_lighting(); setup_render(samples); setup_outlines(density * ENGINE_OUTLINE_SCALE, wobble_scale=1.0); setup_grade()
    _, w, h = setup_camera_engine(G, density, B.col, height_px)
    if blend:
        bpy.ops.wm.save_as_mainfile(filepath=os.path.abspath(os.path.expanduser(blend)))
    if out and render:
        _render_to(out)
    return dict(building=building, cells=G, density=density, png_size=[w, h],
                logical_size=dict(width=G * 64, height=G * 32), draw_size=dict(width=w // density, height=h // density), out=out)


def generate(building="fortress", palette="viet", cells=15, scale=1.0, height_factor=1.0, out=None, blend=None, render=True, trim=True):
    """Legacy camera (the fortress studies): a square frame of cells x 64 x scale px,
    the earth slab's front corner 6 px above the bottom, empty rows trimmed off the top."""
    clear_scene()
    mats = Materials(PALETTES[palette])
    B = Builder(mats, collection_name=building)
    G = RECIPES[building](B)
    setup_lighting(); setup_render(); setup_outlines(); setup_grade()
    setup_camera(G, cells, scale, height_factor)
    sc = bpy.context.scene
    if blend:
        bpy.ops.wm.save_as_mainfile(filepath=os.path.abspath(os.path.expanduser(blend)))
    if out and render:
        path = _render_to(out)
        if trim: final = trim_top(path)
        else: final = (sc.render.resolution_x, sc.render.resolution_y)
    else:
        final = (sc.render.resolution_x, sc.render.resolution_y)
    return dict(objects=len(B.col.objects), render_res=(sc.render.resolution_x, sc.render.resolution_y), output_size=final, ortho=sc.camera.data.ortho_scale, out=out)
