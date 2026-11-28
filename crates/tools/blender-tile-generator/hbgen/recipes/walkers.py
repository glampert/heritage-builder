"""Walkers (docs/plans/placeholder-art.md, Phase 6: the walker experiment). Low-poly people built
from the Builder's primitives, posed procedurally for a walk cycle, and turned to each of the
directions a unit is drawn in, under the fixed isometric camera.

A walker is a unit (catalog/walkers.json): its variations are anim sets' folders, `walk_<dir>`
with 8 frames (`step` 0 to 7) and `idle_<dir>` with 2 (`breath` 0 and 1), for the five directions
rendered, se, e, ne, n and s; sw, w and nw are those mirrored. A figure is modelled facing +X
(screen lower right, se) and the group is turned about Z to its facing, as the trade ship is
(trade.SHIP_FACINGS: e 45 degrees, ne 90, n 135, s -45).

Sizes follow the hand-made walkers: an adult stands about 0.9 of a cell tall (37 logical px with
its hat), and a group of three spans about 0.7 of a cell across. The frame is a 1x1 footprint's
(64 logical px wide); the model stands on the cell's centre, 16 px above the frame's bottom, so
the unit's iso_offset y is 16.

The walk: each leg swings on a sine about its hip (thigh), bending at the knee as it comes
forward; the arms swing against the legs; the body bobs twice a stride. The idle is the figure
standing, rising a little as it breathes.
"""

import math

from mathutils import Matrix, Vector

# The rotation that turns a figure's front (+X as it's modelled) to each facing.
FACINGS = {"se": 0.0, "e": 45.0, "ne": 90.0, "n": 135.0, "s": -45.0}

WALK_STEPS = 8          # frames in a stride
LEG_SWING = 26.0        # degrees each thigh swings either way
KNEE_BEND = 38.0        # degrees the knee bends at its most, as the leg comes forward
ARM_SWING = 22.0
BOB = 0.012             # how far the body rises and falls, twice a stride


def _turn(B, degrees):
    import bpy
    bpy.context.view_layer.update()     # so matrix_world holds the scales and tilts set since the objects were made
    turn = Matrix.Rotation(math.radians(degrees), 4, 'Z')
    for o in B.col.objects:
        o.matrix_world = turn @ o.matrix_world


def _rot_y(v, degrees):
    """Rotates a vector about the figure's Y axis (its hips' axis): positive swings it forward (+X)."""
    return Matrix.Rotation(math.radians(degrees), 3, 'Y') @ Vector(v)


# The cel look of the hand-made walkers: each colour in two flat tones, lit and in shade, with a
# hard edge between (the light's diffuse term thresholded), so a small figure reads crisply.
CEL_SHADE = 0.6          # the shaded tone, as a share of the lit one
CEL_EDGE = 0.32          # where the diffuse light turns from shade to lit
# Thin, steady, near-black ink: the buildings' wobble would smear a figure a few px wide.
WALKER_INK = {"scale": 0.55, "wobble": 0.2, "color": (0.035, 0.025, 0.02)}


def _cel(B, name, hexc):
    from ..palettes import lin
    nt, bsdf, _ = B.M._new(name)
    out = next(n for n in nt.nodes if n.type == 'OUTPUT_MATERIAL')
    nt.nodes.remove(bsdf)
    diff = nt.nodes.new("ShaderNodeBsdfDiffuse"); diff.inputs["Color"].default_value = (1, 1, 1, 1)
    rgb = nt.nodes.new("ShaderNodeShaderToRGB"); nt.links.new(diff.outputs[0], rgb.inputs[0])
    ramp = B.M._ramp(nt, rgb.outputs[0], [(0.0, (CEL_SHADE,) * 3 + (1,)), (CEL_EDGE, (1, 1, 1, 1))])
    ramp.color_ramp.interpolation = 'CONSTANT'
    mix = nt.nodes.new("ShaderNodeMix"); mix.data_type = 'RGBA'; mix.blend_type = 'MULTIPLY'; mix.inputs["Factor"].default_value = 1.0
    nt.links.new(ramp.outputs[0], mix.inputs[6]); mix.inputs[7].default_value = lin(hexc)
    em = nt.nodes.new("ShaderNodeEmission"); nt.links.new(mix.outputs[2], em.inputs[0])
    nt.links.new(em.outputs[0], out.inputs[0])


def _shadow_material(B, name="shadow", alpha=0.38):
    nt, bsdf, _ = B.M._new(name)
    bsdf.inputs["Base Color"].default_value = (0.02, 0.02, 0.01, 1); bsdf.inputs["Alpha"].default_value = alpha
    bsdf.inputs["Roughness"].default_value = 1.0
    m = B.M[name]
    if hasattr(m, "surface_render_method"): m.surface_render_method = 'BLENDED'
    elif hasattr(m, "blend_method"): m.blend_method = 'BLEND'


# Each walker's colours, sampled from its hand-made art so the new directions match it.
COLOURS = {
    "skin": "c08a62", "hair": "1a1512", "eye": "120c0a", "hat": "dcc690",
    # The settler's family.
    "coatBlue": "4a5a78", "coatCream": "d4c29c", "shirtBrown": "8a5432", "trousersOlive": "6a6444",
    "trousersBlack": "221e1c", "sandal": "4a3020", "basket": "8a6a3a", "bundle": "5a6a44", "sling": "6a4630",
    # The peasant (a harvester): bare chest, a head wrap and a knee-length cloth.
    "wrapRose": "9a6a5c", "clothGrey": "6e6250",
    # The medic: a navy tunic and turban, white trousers, a black bag.
    "coatNavy": "2c3264", "trousersWhite": "e6e0d2", "bagBlack": "1e1c1e",
    # The fire watcher (the guard in red): a red tunic and cap, white trousers, dark leg wraps, a spear.
    "tunicRed": "c03a26", "belt": "3a2a1a", "legWrap": "4a4030", "shaft": "5a3a20", "blade": "a8a8a0",
    # The trader (the vendor in indigo): an indigo top, a black skirt and red sash, baskets on a pole.
    "topIndigo": "3f5f8f", "skirtBlack": "1e1a1a", "sashRed": "b8402a", "yoke": "7a5a30",
    # The animals.
    "dogCream": "e2d2ae", "dogNose": "2a2018", "buffalo": "4c5054", "buffaloDark": "34383c", "horn": "6a645a",
}


def _materials(B):
    for name, hexc in COLOURS.items():
        _cel(B, name, hexc)
    _shadow_material(B)


def shadow(B, half_w, half_d, up=0.0):
    """A soft dark blob on the ground under the walkers, as the hand-made ones stand on: an ellipse
    `half_w` across the screen and `half_d` up and down it (world units on the ground), its centre
    `up` up the screen from the feet, so it stays inside the frame above its bottom edge whichever
    way the walker faces. Made after the walker is turned, so it isn't; left uninked (B.ink)."""
    import bpy
    screen_up = Vector((-1.0, 1.0, 0.0)).normalized()     # +X is the screen's lower right, +Y its upper right
    c = screen_up * up
    bpy.ops.mesh.primitive_circle_add(vertices=28, radius=1.0, fill_type='NGON', location=(c.x, c.y, 0.004))
    o = bpy.context.active_object; o.name = "shadow"
    o.rotation_euler = (0.0, 0.0, math.radians(45.0)); o.scale = (half_w, half_d, 1.0)   # local X across the screen
    B._add(o, "shadow")
    B.ink = dict(WALKER_INK, skip=[o])
    return o


def _limb(B, name, top, joint, end, r_top, r_end, m):
    """A two-segment limb (thigh and shin, upper arm and forearm), with a ball at the joint."""
    B.pole(name, top, joint, r_top, m, verts=8)
    B.pole(name, joint, end, r_end, m, verts=8)
    import bpy
    bpy.ops.mesh.primitive_uv_sphere_add(radius=r_top * 1.02, location=joint, segments=8, ring_count=6)
    B.smooth(B._add(bpy.context.active_object, m))


def _ball(B, name, x, y, z, r, m, sz=1.0, sx=1.0):
    import bpy
    bpy.ops.mesh.primitive_uv_sphere_add(radius=r, location=(x, y, z), segments=14, ring_count=10)
    o = bpy.context.active_object; o.name = name; o.scale = (sx, 1.0, sz)
    B._add(o, m); B.smooth(o)
    return o


def person(B, ox, oy, size=1.0, phase=0.0, walking=True, step=0, steps=WALK_STEPS, breath=0, top="coatBlue",
           sleeves=None, skirt=None, coat_len=0.7, trousers="trousersOlive", feet="sandal", wraps=None, belt=None,
           head="hat", head_m="hair", face=True, carry=None):
    """One person at (ox, oy) facing +X, `size` 1 an adult. `phase` (radians) shifts the stride, so a
    group doesn't step in lockstep.
    Clothes: `top` (the torso; "skin" bare-chested), `sleeves` (the arms, default the top), `skirt`
    (default the top) reaching `coat_len` down the legs (0 a shirt to the hips, 1 to the ankles),
    `trousers` (the legs; "skin" bare), `feet` ("sandal" or "bare"), `wraps` (shins wrapped in that
    colour), `belt` (a band at the waist).
    Head: `head` "hat" (the conical hat), "wrap" (a turban or cap in `head_m`), "bun" (hair in a bun)
    or "hair"; `face` draws the eyes (the settler's family has faces, the other walkers' hand-made
    art leaves them blank).
    `carry`: "basket" on the back, "baby" in a sling in front, "bag" at the hip on a strap, "spear"
    in the right hand, "yoke" (a carrying pole on the right shoulder, a basket at each end)."""
    s = size
    t = 2 * math.pi * step / steps + phase
    lift = s * (BOB * (1 + math.cos(2 * t)) / 2 if walking else 0.006 * breath)
    hip_z = 0.4 * s + lift
    thigh, shin = 0.2 * s, 0.2 * s
    sleeves = sleeves or top
    skirt = skirt or top

    # Legs: the thigh swings on a sine; the knee bends while the leg comes forward (the swing's
    # rising half), so the trailing foot stays on the ground and the leading one reaches out. Under
    # a long coat the stride is shorter, so the legs stay inside its skirt.
    swing = LEG_SWING * (1.0 - 0.4 * min(coat_len, 1.0))
    for side, sgn in (("l", 1), ("r", -1)):
        a = sgn * math.sin(t) * swing if walking else 0.0
        bend = KNEE_BEND * max(0.0, sgn * math.cos(t)) if walking else 0.0
        hip = Vector((ox, oy + sgn * 0.045 * s, hip_z))
        knee = hip + _rot_y((0, 0, -thigh), a)
        foot = knee + _rot_y((0, 0, -shin), a - bend)
        _limb(B, "leg_" + side, hip, knee, foot, 0.032 * s, 0.026 * s, trousers)
        if wraps:
            B.pole("wrap_" + side, knee + (foot - knee) * 0.2, foot, 0.03 * s, wraps, verts=8)
        B.box("foot_" + side, foot.x + 0.02 * s, foot.y, foot.z - 0.012 * s, 0.07 * s, 0.035 * s, 0.018 * s,
              "sandal" if feet == "sandal" else "skin")

    # The torso, round and wider across the shoulders than deep, and the skirt flaring over the
    # legs, wide enough front to back that the thighs swing inside it.
    shoulder_z = hip_z + 0.32 * s
    torso = B.cylinder("torso", ox, oy, hip_z - 0.02 * s, 0.062 * s, 0.34 * s, top, verts=16, r2=0.075 * s)
    torso.scale = (1.0, 1.3, 1.0)
    if coat_len > 0:
        length = coat_len * (thigh + shin) * 0.95
        sk = B.cylinder("skirt", ox, oy, hip_z - length, 0.115 * s, length + 0.02 * s, skirt, verts=16, r2=0.07 * s)
        sk.scale = (1.35, 1.1, 1.0)   # deeper front to back, so a striding thigh stays inside
    if belt:
        band = B.cylinder("belt", ox, oy, hip_z - 0.01 * s, 0.07 * s, 0.035 * s, belt, verts=16)
        band.scale = (1.05, 1.32, 1.0)

    # Arms swing against the legs, bent a little at the elbow; a hand that holds something is raised.
    hands = {}
    for side, sgn in (("l", 1), ("r", -1)):
        a = -sgn * math.sin(t) * ARM_SWING if walking else 3.0
        bend = 18.0
        if side == "r" and carry == "spear":
            a, bend = 10.0, 75.0
        elif side == "r" and carry == "yoke":
            a, bend = 35.0, 115.0
        sh = Vector((ox, oy + sgn * 0.11 * s, shoulder_z - 0.03 * s))
        elbow = sh + _rot_y((0, 0, -0.15 * s), a)
        hand = elbow + _rot_y((0, 0, -0.14 * s), a + bend)
        hands[side] = hand
        _limb(B, "arm_" + side, sh, elbow, hand, 0.026 * s, 0.022 * s, sleeves)
        _ball(B, "hand_" + side, hand.x, hand.y, hand.z, 0.022 * s, "skin")

    # Neck, head, and the hat, wrap or hair.
    head_z = shoulder_z + 0.1 * s
    B.cylinder("neck", ox, oy, shoulder_z - 0.01 * s, 0.03 * s, 0.05 * s, "skin", verts=8)
    _ball(B, "head", ox + 0.006 * s, oy, head_z, 0.064 * s, "skin", sz=1.12)
    _ball(B, "hair", ox - 0.014 * s, oy, head_z + 0.014 * s, 0.062 * s, "hair", sz=1.05)
    if face:
        # Two small dark eyes, so it reads as a face at the hand-made art's size.
        for sgn in (1, -1):
            _ball(B, "eye", ox + 0.066 * s, oy + sgn * 0.024 * s, head_z + 0.006 * s, 0.0075 * s, "eye")
    if head == "hat":
        # Tipped back on the head, as the hand-made walkers wear it, so the face shows under its brim.
        h = B.cone("hat", ox - 0.012 * s, oy, head_z + 0.055 * s, 0.112 * s, 0.005 * s, 0.085 * s, "hat", verts=20)
        h.rotation_euler = (0.0, math.radians(-16.0), 0.0)
    elif head == "wrap":
        _ball(B, "wrap", ox - 0.008 * s, oy, head_z + 0.04 * s, 0.07 * s, head_m, sz=0.62)
    elif head == "bun":
        _ball(B, "bun", ox - 0.06 * s, oy, head_z + 0.035 * s, 0.035 * s, "hair")

    if carry == "basket":
        # A tall carrying basket on the back, a bundle on top, its strap over the shoulders.
        bx = ox - 0.14 * s
        B.cylinder("basket", bx, oy, hip_z + 0.02 * s, 0.085 * s, 0.32 * s, "basket", verts=14, r2=0.1 * s)
        _ball(B, "basket_load", bx, oy, hip_z + 0.36 * s, 0.09 * s, "bundle", sz=0.7)
        for sgn in (1, -1):
            B.pole("strap", (ox + 0.06 * s, oy + sgn * 0.08 * s, shoulder_z), (bx, oy + sgn * 0.07 * s, shoulder_z - 0.05 * s),
                   0.008 * s, "sling", verts=5)
    elif carry == "baby":
        # A baby held in a sling across the chest.
        _ball(B, "baby", ox + 0.1 * s, oy + 0.03 * s, shoulder_z - 0.11 * s, 0.07 * s, "coatCream", sz=0.9)
        _ball(B, "baby_head", ox + 0.12 * s, oy + 0.05 * s, shoulder_z - 0.03 * s, 0.04 * s, "skin")
        B.pole("sling", (ox - 0.02 * s, oy - 0.11 * s, shoulder_z + 0.01 * s), (ox + 0.1 * s, oy + 0.07 * s, shoulder_z - 0.17 * s),
               0.016 * s, "sling", verts=6)
    elif carry == "bag":
        # A big bag at the left hip, its strap across the chest from the right shoulder.
        _ball(B, "bag", ox - 0.02 * s, oy + 0.14 * s, hip_z + 0.04 * s, 0.075 * s, "bagBlack", sz=1.1, sx=1.3)
        B.pole("bag_strap", (ox + 0.02 * s, oy - 0.08 * s, shoulder_z + 0.01 * s), (ox - 0.01 * s, oy + 0.13 * s, hip_z + 0.1 * s),
               0.01 * s, "bagBlack", verts=5)
    elif carry == "spear":
        # A spear upright in the right hand, leaning a little forward.
        hand = hands["r"]
        lean = _rot_y((0, 0, 1), 3.0)          # near upright: the hand-made frame is 23 px wide
        B.pole("spear", hand - lean * 0.34 * s, hand + lean * 0.27 * s, 0.011 * s, "shaft", verts=6)
        tip = hand + lean * 0.27 * s
        B.cone("spear_head", tip.x, tip.y, tip.z, 0.02 * s, 0.0, 0.065 * s, "blade", verts=6)
    elif carry == "yoke":
        # A carrying pole on the right shoulder, front to back, a shallow basket hung from each end.
        y = oy - 0.09 * s
        z = shoulder_z + 0.02 * s
        B.pole("yoke", (ox + 0.34 * s, y, z - 0.03 * s), (ox - 0.34 * s, y, z - 0.03 * s), 0.012 * s, "yoke", verts=6)
        swing_b = 0.015 * s * math.sin(2 * t) if walking else 0.0
        for end in (1, -1):
            bx = ox + end * 0.3 * s + swing_b
            B.cylinder("yoke_basket", bx, y, 0.17 * s + lift, 0.085 * s, 0.07 * s, "basket", verts=14, r2=0.1 * s)
            for dy in (-0.06, 0.06):
                B.pole("yoke_rope", (ox + end * 0.32 * s, y, z - 0.03 * s), (bx, y + dy * s, 0.24 * s + lift), 0.004 * s, "sling", verts=4)


def build_settler_family(B, facing="se", step=0, steps=WALK_STEPS, breath=None, **_):
    """The settler as the hand-made art draws it: a family walking to its new home. The father,
    in a long blue coat and a conical hat, carries a basket of their things; the mother, in cream,
    holds a baby in a sling; their child, in brown, walks at the father's side. `step` (0 to 7) is
    the walk's frame; `breath` (0 or 1) makes it an idle frame instead."""
    if facing not in FACINGS:
        raise ValueError(f"walker facing must be one of {sorted(FACINGS)}, got {facing!r}")
    _materials(B)
    walking = breath is None
    pose = dict(walking=walking, step=step, steps=steps, breath=breath or 0)
    # Side by side as the hand-made settler walks: the mother a little behind on the father's right,
    # the child ahead on his left, so it shows walking e and w too (seen from the front, se: mother,
    # father, child, left to right).
    person(B, 0.0, 0.0, size=1.0, top="coatBlue", coat_len=0.75, trousers="trousersBlack", carry="basket", **pose)
    person(B, -0.1, -0.22, size=0.92, phase=math.pi * 0.5, top="coatCream", coat_len=0.8, trousers="trousersBlack",
           carry="baby", **pose)
    person(B, 0.16, 0.22, size=0.62, phase=math.pi * 0.9, top="shirtBrown", coat_len=0.0, head="hair", **pose)
    _turn(B, FACINGS[facing])
    shadow(B, 0.42, 0.26, up=0.12)
    return 1


def _walker(B, facing, breath, step, steps, shadow_r, **dress):
    """A walker on its own: one person dressed by `dress` (see person), on its shadow."""
    if facing not in FACINGS:
        raise ValueError(f"walker facing must be one of {sorted(FACINGS)}, got {facing!r}")
    _materials(B)
    person(B, 0.0, 0.0, walking=breath is None, step=step, steps=steps, breath=breath or 0, **dress)
    _turn(B, FACINGS[facing])
    shadow(B, *shadow_r)
    return 1


def build_peasant(B, facing="se", step=0, steps=WALK_STEPS, breath=None, **_):
    """The peasant (a lumberyard's harvester): bare-chested and barefoot, in a knee-length grey-brown
    cloth and a rose head wrap."""
    return _walker(B, facing, breath, step, steps, (0.18, 0.12, 0.05), top="skin", skirt="clothGrey", coat_len=0.42,
                   trousers="skin", feet="bare", head="wrap", head_m="wrapRose", face=False)


def build_medic(B, facing="se", step=0, steps=WALK_STEPS, breath=None, **_):
    """The medic: a long navy tunic and turban over white trousers, a black bag at the hip."""
    return _walker(B, facing, breath, step, steps, (0.19, 0.12, 0.05), top="coatNavy", coat_len=0.55,
                   trousers="trousersWhite", head="wrap", head_m="coatNavy", face=False, carry="bag")


def build_fire_watcher(B, facing="se", step=0, steps=WALK_STEPS, breath=None, **_):
    """The fire watcher (the guard in red): a belted red tunic and cap, white trousers, dark leg
    wraps, and a spear in his right hand."""
    return _walker(B, facing, breath, step, steps, (0.18, 0.12, 0.04), top="tunicRed", coat_len=0.3, belt="belt",
                   trousers="trousersWhite", wraps="legWrap", head="wrap", head_m="tunicRed", face=False, carry="spear")


def build_trader(B, facing="se", step=0, steps=WALK_STEPS, breath=None, **_):
    """The trader (the vendor in indigo): a woman in an indigo top and a long black skirt with a red
    sash, her hair in a bun, carrying her goods in two baskets on a shoulder pole. A little smaller
    than the other walkers, as the hand-made vendor is drawn."""
    return _walker(B, facing, breath, step, steps, (0.32, 0.13, 0.05), size=0.84, top="topIndigo", sleeves="skin", skirt="skirtBlack",
                   coat_len=0.95, belt="sashRed", trousers="skirtBlack", feet="bare", head="bun", face=False, carry="yoke")


# ----------------------------------------------------------------------------
# Animals
# ----------------------------------------------------------------------------

def quadruped(B, length, r, leg, head_r, body, walking=True, step=0, steps=5, breath=0, dark=None, neck_up=0.0,
              head_ahead=0.9, tail="down", horns=False, ears=False, nose="dogNose"):
    """A four-legged animal facing +X: a body `length` long and `r` round (its back 2r deep), on legs
    `leg` long that swing in diagonal pairs (front left with back right), a head `head_r` round on a
    short neck `head_ahead` head radii past the body's front, raised `neck_up` above the back's middle. `dark` colours the legs and head
    (the buffalo's); `tail` "down" or "up" (the dog's, curled); `horns` a water buffalo's, sweeping
    out and back; `ears` pointed ears."""
    t = 2 * math.pi * step / steps
    lift = (0.03 * r * (1 + math.cos(2 * t))) if walking else 0.02 * r * breath
    dark = dark or body
    body_z = leg + r * 0.85 + lift              # the body's middle
    half = length / 2
    # The body: an ellipsoid, a little deeper than wide.
    _ball(B, "body", 0.0, 0.0, body_z, r * 0.9, body, sz=1.0 / 0.9, sx=half / (r * 0.9))
    # Legs from under the body, swinging in diagonal pairs; the knees bend back on the front legs and
    # forward on the hind ones, as an animal's do.
    for (lx, ly, ph) in ((0.62, 1, 0.0), (0.62, -1, math.pi), (-0.62, 1, math.pi), (-0.62, -1, 0.0)):
        a = math.sin(t + ph) * 22.0 if walking else 0.0
        bend = 26.0 * max(0.0, math.cos(t + ph)) if walking else 0.0
        top = Vector((lx * half, ly * r * 0.5, body_z - r * 0.35))
        knee = top + _rot_y((0, 0, -(top.z - lift) * 0.52), a)
        foot = knee + _rot_y((0, 0, -(top.z - lift) * 0.5), a + (bend if lx > 0 else -bend))
        _limb(B, "leg", top, knee, foot, r * 0.26, r * 0.19, dark)
    # Neck and head.
    head = Vector((half + head_r * head_ahead, 0.0, body_z + neck_up + (0.02 * r * math.sin(2 * t) if walking else 0.0)))
    B.pole("neck", (half * 0.7, 0.0, body_z + r * 0.25), head, r * 0.42, body, verts=12)
    _ball(B, "head", head.x, head.y, head.z, head_r, dark, sz=0.9, sx=1.25)
    _ball(B, "nose", head.x + head_r * 1.05, 0.0, head.z - head_r * 0.25, head_r * 0.42, nose, sz=0.8)
    if ears:
        for sgn in (1, -1):
            B.cone("ear", head.x - head_r * 0.25, sgn * head_r * 0.5, head.z + head_r * 0.55, head_r * 0.32, 0.0,
                   head_r * 0.8, body, verts=6)
    if horns:
        # A water buffalo's horns: out to each side in a wide arc, sweeping back and up at the tips,
        # tapering, with a ball at each joint so the curve is smooth.
        for sgn in (1, -1):
            pts = []
            for k in range(7):
                u = k / 6
                pts.append(Vector((head.x - head_r * (0.1 + 1.2 * u * u),
                                   sgn * head_r * (0.45 + 1.9 * math.sin(u * 1.9)),
                                   head.z + head_r * (0.45 + 0.6 * u * u))))
            for k, (a_, b_) in enumerate(zip(pts, pts[1:])):
                rad = head_r * 0.2 * (1.0 - 0.75 * k / 6)
                B.pole("horn", a_, b_, rad, "horn", verts=8)
                _ball(B, "horn_joint", b_.x, b_.y, b_.z, rad * 0.95, "horn")
    # The tail.
    base = Vector((-half * 0.95, 0.0, body_z + r * 0.4))
    if tail == "up":
        mid = base + Vector((-r * 0.35, 0.0, r * 0.7)); tip = mid + Vector((r * 0.25, 0.0, r * 0.45))
        B.pole("tail", base, mid, r * 0.16, body, verts=6); B.pole("tail", mid, tip, r * 0.12, body, verts=6)
    else:
        B.pole("tail", base, base + Vector((-r * 0.25, 0.0, -r * 1.4)), r * 0.07, dark, verts=6)


def _animal(B, facing, breath, step, steps, shadow_r, **kw):
    if facing not in FACINGS:
        raise ValueError(f"walker facing must be one of {sorted(FACINGS)}, got {facing!r}")
    _materials(B)
    quadruped(B, walking=breath is None, step=step, steps=steps, breath=breath or 0, **kw)
    _turn(B, FACINGS[facing])
    shadow(B, *shadow_r)
    return 1


def build_dog(B, facing="se", step=0, steps=5, breath=None, **_):
    """The dog that wanders from the houses: cream, pointed ears, its tail curled up."""
    return _animal(B, facing, breath, step, steps, (0.2, 0.11, 0.03), length=0.24, r=0.075, leg=0.13, head_r=0.055,
                   neck_up=0.09, head_ahead=0.5, body="dogCream", tail="up", ears=True)


def build_buffalo(B, facing="se", step=0, steps=5, breath=None, **_):
    """The water buffalo that wanders from the farms: dark grey, darker on its legs and head, long
    horns sweeping back."""
    return _animal(B, facing, breath, step, steps, (0.48, 0.28, 0.05), length=0.66, r=0.19, leg=0.2, head_r=0.1,
                   neck_up=-0.02, head_ahead=0.5, body="buffalo", dark="buffaloDark", horns=True, nose="buffaloDark")
