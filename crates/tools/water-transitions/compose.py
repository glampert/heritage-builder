#!/usr/bin/env python3
"""Composes the water shore sprites that the hand-made tile set doesn't have, out of the ones it has.

The water tile's hand-made variations cover land on one side of the cell (0001, 0010, 0100, 1000),
on two adjacent sides (0011, 0110, 1001, 1100) and at one diagonal (ne_corner, nw_corner,
se_corner, sw_corner): 13 of the 47 combinations of land neighbors a water cell can have. This
script makes the other 42 by shifting and overlaying those.

In the hand-made sprites, a land neighbor's shoreline lies a quarter of the way past the cell's
center (land covers 0.625 of the cell, water the rest), and a diagonal neighbor's land is the
square past both of the cell's edges on that side. Two land neighbors on opposite sides would
cover the whole cell, so a one-cell channel between them needs both shores moved towards their
land by CELL_SHIFT; the water left between the banks is the channel. Every other generated
variation keeps the same banks:

- Land on three sides (0111, 1011, 1101, 1110) or four (1111): the inner corners move the same
  way as the shores, closing the channel into a bay or a pond.
- Several diagonals (ne_se_corner, ...): a corner moves along each of its edges that a channel
  crosses, i.e. each edge whose other diagonal is land too: two adjacent corners leave a channel
  mouth between them, four leave a crossing.
- A side plus both diagonals on the far side (1000_se_sw_corner, ...): a T junction, the shore and
  both corners moved as channel banks. A side plus one far diagonal (1000_se_corner, ...) keeps
  the hand-made positions: there the diagonal is another island, not a bank.
- Two adjacent sides plus the opposite diagonal (1100_sw_corner, ...): a bend in a channel.
- A side plus the far diagonal of one water side and the diagonal next to the land on that same
  water side (0010_nw_sw_corner, ...): a channel entering beside the land, into wider water such
  as a lake. The shore bends from the channel's bank at that end to its usual position at the
  other, and the far corner moves as a bank.

Within a cell, land wins over water and dirt over grass, so overlaying the shifted pieces leaves
water only where none of them has land. A shifted shore or corner repeats its edge over the part
of the cell it no longer covers, which is all water. A bank that ends at a bay's end or a bend's
outer corner stops where the bank across it has its dirt, so the dirt meets in a square corner:
the north and south shores' dirt is 0.55 wide, the east and west shores' 0.25.

Names: the four digits are the sides with land, north, east, south, west (north is the upper-right
edge on screen, east the lower-right); `_corner` lists the diagonals with land, in the order ne,
nw, se, sw. game::tile::water builds the same names. Only the standard library is used, so the
PNG reading and writing is done here (8-bit RGB/RGBA, not interlaced).

Usage, from the repository root:
    python3 crates/tools/water-transitions/compose.py [--out DIR] [NAME ...]
Writes assets/tiles/terrain/water/water/<name>/<name>.png for every generated variation, or only
the named ones. --out writes them under DIR instead, to look at them first; --list prints the
names, in the order the tile set lists them.
"""
import collections
import os
import struct
import sys
import zlib

SPRITE_DIR = os.path.join("assets", "tiles", "terrain", "water", "water")

# The sprites are 369x236. The top face is the diamond centered at (CX, CY) with half extents
# (HW, HH); the 22 rows below it are the cell's two visible sides, drawn as an extrusion.
W, H = 369, 236
CX, CY, HW, HH = 184.0, 107.0, 184.0, 106.0

# How far a shore or corner moves towards its land where it is the bank of a one-cell channel, in
# cell units where the cell's center is 0 and its edges are at -1 and 1. A hand-made shoreline is
# at -0.25 on the land's side of the center, so a bank is at 0.31 and a channel is 0.62 wide.
CELL_SHIFT = 0.56

# Scores decide which layer a pixel comes from: dirt beats grass, which beats water, and the
# water nearest a shore beats the water further from one. Layers whose scores are closer than
# FADE are cross-faded, which hides the seams in open water.
DIST_RANGE = 90.0  # Pixels. Water further than this from a layer's shore scores -1.
BASE_WATER_SCORE = -0.65
FADE = 0.3

EDGE = {"n": "1000", "e": "0100", "s": "0010", "w": "0001"}
INNER = {("n", "e"): "1100", ("e", "s"): "0110", ("s", "w"): "0011", ("n", "w"): "1001"}
CORNER = {"ne": "ne_corner", "nw": "nw_corner", "se": "se_corner", "sw": "sw_corner"}
# Outward direction of each side, as (du, dv): u runs from the south edge (-1) to the north edge
# (+1), v from the west edge (-1) to the east edge (+1).
SIDE_DIR = {"n": (1, 0), "s": (-1, 0), "e": (0, 1), "w": (0, -1)}
# Which diagonals can show land next to a side with land: the two on the far side.
FAR_CORNERS = {"n": ("se", "sw"), "e": ("nw", "sw"), "s": ("ne", "nw"), "w": ("ne", "se")}
# The two sides each corner touches; a corner is covered when either side is land.
CORNER_SIDES = {"ne": ("n", "e"), "nw": ("n", "w"), "se": ("s", "e"), "sw": ("s", "w")}
CORNER_ORDER = ["ne", "nw", "se", "sw"]
HAND_MADE = {"0000", *EDGE.values(), *INNER.values(), *CORNER.values()}


# ----------------------------------------------
# PNG reading and writing
# ----------------------------------------------

def read_png(path):
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", path
    pos, idat = 8, []
    while pos < len(data):
        length, kind = struct.unpack_from(">I4s", data, pos)
        body = data[pos + 8:pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            width, height, bit_depth, color_type, _, _, interlace = struct.unpack(">IIBBBBB", body)
            assert bit_depth == 8 and color_type in (2, 6) and interlace == 0, path
            channels = 4 if color_type == 6 else 3
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"IEND":
            break
    raw = zlib.decompress(b"".join(idat))
    stride = width * channels
    out = bytearray(width * height * 4)
    prev = bytearray(stride)
    pos = 0
    for y in range(height):
        filter_type = raw[pos]
        row = bytearray(raw[pos + 1:pos + 1 + stride])
        pos += 1 + stride
        if filter_type == 1:
            for i in range(channels, stride):
                row[i] = (row[i] + row[i - channels]) & 0xFF
        elif filter_type == 2:
            for i in range(stride):
                row[i] = (row[i] + prev[i]) & 0xFF
        elif filter_type == 3:
            for i in range(stride):
                left = row[i - channels] if i >= channels else 0
                row[i] = (row[i] + ((left + prev[i]) >> 1)) & 0xFF
        elif filter_type == 4:
            for i in range(stride):
                a = row[i - channels] if i >= channels else 0
                b = prev[i]
                c = prev[i - channels] if i >= channels else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                row[i] = (row[i] + (a if (pa <= pb and pa <= pc) else (b if pb <= pc else c))) & 0xFF
        if channels == 4:
            out[y * width * 4:(y + 1) * width * 4] = row
        else:
            o = y * width * 4
            for x in range(width):
                out[o + x * 4:o + x * 4 + 3] = row[x * 3:x * 3 + 3]
                out[o + x * 4 + 3] = 255
        prev = row
    return width, height, out


def write_png(path, width, height, rgba):
    stride = width * 4
    rows = []
    prev = bytearray(stride)
    for y in range(height):
        row = rgba[y * stride:(y + 1) * stride]
        filtered = bytearray(stride)
        for i in range(stride):  # Paeth filter on every row.
            a = row[i - 4] if i >= 4 else 0
            b = prev[i]
            c = prev[i - 4] if i >= 4 else 0
            p = a + b - c
            pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
            filtered[i] = (row[i] - (a if (pa <= pb and pa <= pc) else (b if pb <= pc else c))) & 0xFF
        rows.append(b"\x04" + bytes(filtered))
        prev = row

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(b"".join(rows), 9)) + chunk(b"IEND", b"")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    open(path, "wb").write(png)


# ----------------------------------------------
# Geometry and pixel classes
# ----------------------------------------------

def below_top_face(x, y):
    # Under the cell's lower edges, where the two side faces are drawn.
    return y > CY + HH * (1.0 - abs(x - CX) / HW) + 0.5


def cell_coords(x, y):
    # Pixel to cell coordinates: u runs from the south edge (-1) to the north edge (+1), v from
    # the west edge (-1) to the east edge (+1).
    a = (x - CX) / HW
    b = (y - CY) / HH
    return a - b, a + b


def pixel_coords(u, v):
    a = (u + v) / 2.0
    b = (v - u) / 2.0
    return int(round(CX + a * HW)), int(round(CY + b * HH))


def landness(r, g, b):
    # Grass and dirt have more red than blue; water and foam have more blue.
    return min(1.0, max(0.0, (r - b + 4) / 14.0))


def dirtness(r, g, b, land):
    # Dirt is dark; grass has green near or above 100.
    return land * min(1.0, max(0.0, (95 - g) / 25.0))


class Layer:
    """One source sprite, optionally shifted, with its pixel classes and distance to its land."""

    def __init__(self, rgba, du=0.0, dv=0.0, is_base=False, repeat_edges=True, clip=None, ku=0.0, kv=0.0):
        self.is_base = is_base
        self.rgba = bytearray(W * H * 4)
        if du == 0.0 and dv == 0.0 and ku == 0.0 and kv == 0.0 and clip is None:
            self.rgba[:] = rgba
        else:
            # Shift the top face by (du, dv) in cell coordinates. `ku` adds to the shift along u
            # as v goes from -1 to 1, `kv` to the shift along v as u does: a shore that bends.
            # Where the shifted sprite no longer covers the cell, its edge repeats if
            # `repeat_edges`: for a side's shore or a corner that edge is all water. An inner
            # corner's far edges hold land too, so it leaves that part of the cell to the other
            # pieces instead. `clip` keeps the piece inside a range of u or v, e.g. a bank that
            # stops where its channel ends.
            limit = 0.98
            for y in range(H):
                for x in range(W):
                    i = (y * W + x) * 4
                    if rgba[i + 3] == 0 or below_top_face(x, y):
                        continue
                    u, v = cell_coords(x, y)
                    if clip is not None:
                        axis, lo, hi = clip
                        if not lo <= (u if axis == "u" else v) <= hi:
                            continue
                    su = u - (du + ku * (v + 1.0) / 2.0)
                    sv = v - (dv + kv * (u + 1.0) / 2.0)
                    if not repeat_edges and (abs(su) > limit or abs(sv) > limit):
                        continue
                    su = min(max(su, -limit), limit)
                    sv = min(max(sv, -limit), limit)
                    sx, sy = pixel_coords(su, sv)
                    sx = min(max(sx, 0), W - 1)
                    sy = min(max(sy, 0), H - 1)
                    si = (sy * W + sx) * 4
                    self.rgba[i:i + 3] = rgba[si:si + 3]
                    self.rgba[i + 3] = 255
        self.land = [0.0] * (W * H)
        self.dirt = [0.0] * (W * H)
        for p in range(W * H):
            i = p * 4
            if self.rgba[i + 3] > 128:
                land = landness(self.rgba[i], self.rgba[i + 1], self.rgba[i + 2])
                self.land[p] = land
                self.dirt[p] = dirtness(self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], land)
        self.dist = chamfer_distance([land > 0.5 for land in self.land])

    def has_pixel(self, p):
        # A shifted layer's own soft edge, now inside the cell, is left to the other layers.
        return self.rgba[p * 4 + 3] > 200

    def score(self, p):
        if self.is_base:
            penalty = 0.0 if self.land[p] > 0.5 else -BASE_WATER_SCORE
        else:
            penalty = min(self.dist[p], DIST_RANGE) / DIST_RANGE
        return 2.0 * self.dirt[p] + self.land[p] - penalty


def chamfer_distance(mask):
    # Two-pass chamfer distance (3-4 weights, scaled to pixels) from every pixel to the nearest
    # set pixel of `mask`.
    big = 10 ** 6
    dist = [0 if m else big for m in mask]
    for y in range(H):
        for x in range(W):
            p = y * W + x
            if dist[p] == 0:
                continue
            best = dist[p]
            if x > 0:
                best = min(best, dist[p - 1] + 3)
            if y > 0:
                best = min(best, dist[p - W] + 3)
                if x > 0:
                    best = min(best, dist[p - W - 1] + 4)
                if x < W - 1:
                    best = min(best, dist[p - W + 1] + 4)
            dist[p] = best
    for y in range(H - 1, -1, -1):
        for x in range(W - 1, -1, -1):
            p = y * W + x
            if dist[p] == 0:
                continue
            best = dist[p]
            if x < W - 1:
                best = min(best, dist[p + 1] + 3)
            if y < H - 1:
                best = min(best, dist[p + W] + 3)
                if x < W - 1:
                    best = min(best, dist[p + W + 1] + 4)
                if x > 0:
                    best = min(best, dist[p + W - 1] + 4)
            dist[p] = best
    return [d / 3.0 for d in dist]


# ----------------------------------------------
# Compositing
# ----------------------------------------------

def compose(base, layers, sprites):
    """Blends the layers over the base, pixel by pixel, by score. Returns RGBA bytes."""
    out = bytearray(W * H * 4)
    all_layers = [base] + layers
    for p in range(W * H):
        i = p * 4
        # The base keeps its silhouette, soft edges included, so tiles still blend into their
        # neighbors as the hand-made ones do.
        alpha = base.rgba[i + 3]
        if alpha == 0 or below_top_face(p % W, p // W):
            continue
        scored = sorted(((layer.score(p), n) for n, layer in enumerate(all_layers) if layer.has_pixel(p)), reverse=True)
        if not scored:
            out[i:i + 4] = base.rgba[i:i + 4]
            continue
        first = all_layers[scored[0][1]].rgba
        if len(scored) > 1 and scored[0][0] - scored[1][0] < FADE:
            second = all_layers[scored[1][1]].rgba
            w = 0.5 + (scored[0][0] - scored[1][0]) / (2.0 * FADE)
            for c in range(3):
                out[i + c] = int(round(first[i + c] * w + second[i + c] * (1.0 - w)))
        else:
            out[i:i + 3] = first[i:i + 3]
        out[i + 3] = alpha
    draw_side_faces(out, sprites)
    return out


def draw_side_faces(out, sprites):
    # The side under each bottom edge is brown where the top face above it is land, water otherwise.
    water_faces = sprites["0000"]
    brown_s = sprites["0010"]  # Land along the whole south edge: brown south face.
    brown_e = sprites["0100"]  # Land along the whole east edge: brown east face.
    for y in range(H):
        for x in range(W):
            i = (y * W + x) * 4
            if water_faces[i + 3] == 0 or not below_top_face(x, y):
                continue
            edge_y = int(CY + HH * (1.0 - abs(x - CX) / HW))
            above = (max(0, edge_y - 3) * W + x) * 4
            land = out[above + 3] > 128 and landness(out[above], out[above + 1], out[above + 2]) > 0.5
            source = (brown_s if x < CX else brown_e) if land else water_faces
            out[i:i + 3] = source[i:i + 3]
            out[i + 3] = water_faces[i + 3]


# ----------------------------------------------
# Recipes
# ----------------------------------------------

VERTEX = {"ne": (1, 1), "nw": (1, -1), "se": (-1, 1), "sw": (-1, -1)}  # (u, v) of each corner.
OPPOSITE_CORNER = {("n", "e"): "sw", ("e", "s"): "nw", ("s", "w"): "ne", ("n", "w"): "se"}


def variation_name(sides, corners):
    name = ""
    if sides or not corners:
        name = "".join("1" if side in sides else "0" for side in "nesw")
    if corners:
        name += ("_" if name else "") + "_".join(c for c in CORNER_ORDER if c in corners) + "_corner"
    return name


# A piece names a sprite and how Layer transforms it.
Piece = collections.namedtuple("Piece", "sprite du dv repeat clip ku kv", defaults=(0.0, 0.0, True, None, 0.0, 0.0))

# How far each side's dirt reaches from the cell's center once its shore is shifted, in cell
# units. The hand-made north and south shores have a dirt band 0.55 wide, the east and west ones
# 0.25. A bank that ends at another bank stops where that bank's dirt ends, so the two meet in a
# square corner of dirt.
DIRT_EXTENT = {"n": 0.3 + CELL_SHIFT, "s": 0.3 + CELL_SHIFT, "e": 0.0 + CELL_SHIFT, "w": 0.0 + CELL_SHIFT}


def shifted_edge(side, ends_towards=None, bounded=False):
    # The shore of `side` as a channel bank. With `ends_towards`, the bank stops at the bank on
    # that side, where its channel ends; `bounded` stops it at the banks on both ends, for the
    # bank that closes a channel or a pond.
    du, dv = SIDE_DIR[side]
    axis = "v" if side in "ns" else "u"  # The axis the shore runs along.
    across = "e" if side in "ns" else "n"  # The sides at the ends of that axis.
    clip = None
    if bounded:
        clip = (axis, -DIRT_EXTENT[across], DIRT_EXTENT[across])
    elif ends_towards is not None:
        end = DIRT_EXTENT[ends_towards]
        clip = (axis, -1.0, end) if SIDE_DIR[ends_towards] in ((1, 0), (0, 1)) else (axis, -end, 1.0)
    return Piece(EDGE[side], du * CELL_SHIFT, dv * CELL_SHIFT, clip=clip)


def bending_edge(side, bank_end):
    # The shore of `side` as a channel bank at the `bank_end` side of the cell, bending to its
    # usual position at the other end.
    du, dv = SIDE_DIR[side]
    ax, ay = SIDE_DIR[bank_end]
    if side in "ns":
        # The shore runs along v; the bank is at v = ay.
        return Piece(EDGE[side], du * CELL_SHIFT if ay < 0 else 0.0, 0.0, ku=du * CELL_SHIFT * ay)
    # The shore runs along u; the bank is at u = ax.
    return Piece(EDGE[side], 0.0, dv * CELL_SHIFT if ax < 0 else 0.0, kv=dv * CELL_SHIFT * ax)


def shifted_inner(a, b):
    pair = (a, b) if (a, b) in INNER else (b, a)
    du = (SIDE_DIR[a][0] + SIDE_DIR[b][0]) * CELL_SHIFT
    dv = (SIDE_DIR[a][1] + SIDE_DIR[b][1]) * CELL_SHIFT
    return Piece(INNER[pair], du, dv, repeat=False)


def corner_piece(c, along_u, along_v):
    # A corner moved towards its vertex along u and/or v, by CELL_SHIFT each.
    su, sv = VERTEX[c]
    return Piece(CORNER[c], su * CELL_SHIFT if along_u else 0.0, sv * CELL_SHIFT if along_v else 0.0)


def recipe(sides, corners):
    """The base sprite (or None for open water) and the pieces laid over it for land on `sides`
    and at `corners`."""
    sides, corners = set(sides), set(corners)
    if len(sides) == 2 and sides in ({"n", "s"}, {"e", "w"}):
        # A channel: both shores moved towards their land.
        return None, [shifted_edge(s) for s in sorted(sides)]
    if len(sides) == 3:
        # A bay: the two banks and the bank across the end, rounded off by the inner corners.
        pair = {"n", "s"} if {"n", "s"} <= sides else {"e", "w"}
        closed = (sides - pair).pop()
        pieces = [shifted_edge(s, closed) for s in sorted(pair)] + [shifted_edge(closed, bounded=True)]
        return None, pieces + [shifted_inner(s, closed) for s in sorted(pair)]
    if len(sides) == 4:
        # A pond: four banks in a ring, rounded off by the four inner corners.
        return None, [shifted_edge(s, bounded=True) for s in "nesw"] + [shifted_inner(a, b) for (a, b) in INNER]
    if len(sides) == 1:
        side = sides.pop()
        attached = [c for c in corners if side in CORNER_SIDES[c]]
        if attached:
            # A channel entering beside the land: the shore bends from its bank at that end, and
            # the far corner on the same water side is its other bank.
            entry = next(s for s in CORNER_SIDES[attached[0]] if s != side)
            far = next(c for c in corners if c not in attached)
            return None, [bending_edge(side, entry), corner_piece(far, entry in "ew", entry in "ns")]
        if corners == set(FAR_CORNERS[side]):
            # A T junction: the shore and both corners are channel banks.
            return None, [shifted_edge(side)] + [corner_piece(c, True, True) for c in sorted(corners)]
        # Another island past the corner: hand-made positions.
        return EDGE[side], [corner_piece(c, False, False) for c in sorted(corners)]
    if len(sides) == 2:
        # A bend: the outer bank's two shores, each stopping at the other, its inner corner, and
        # the inner bank's corner.
        pair = tuple(s for s in "nesw" if s in sides)
        pair = pair if pair in INNER else (pair[1], pair[0])
        a, b = pair
        return None, [shifted_edge(a, b), shifted_edge(b, a), shifted_inner(a, b), corner_piece(OPPOSITE_CORNER[pair], True, True)]
    # Diagonals only. A corner moves along each of its edges whose other diagonal is land too: a
    # channel crosses that edge, and the corner is one of its banks.
    pieces = []
    for c in sorted(corners):
        su, sv = VERTEX[c]
        other_on_u_edge = next(o for o, (ou, ov) in VERTEX.items() if (ou, ov) == (su, -sv))  # Same north/south edge.
        other_on_v_edge = next(o for o, (ou, ov) in VERTEX.items() if (ou, ov) == (-su, sv))  # Same east/west edge.
        pieces.append(corner_piece(c, other_on_v_edge in corners, other_on_u_edge in corners))
    return None, pieces


def all_variations():
    """Every valid combination of land neighbors, as (sides, corners), in a fixed order."""
    combos = []
    for side_bits in range(16):
        sides = {s for n, s in enumerate("nesw") if side_bits & (8 >> n)}
        allowed = [c for c in CORNER_ORDER if not any(s in sides for s in CORNER_SIDES[c])]
        for corner_bits in range(1 << len(allowed)):
            corners = {c for n, c in enumerate(allowed) if corner_bits & (1 << n)}
            combos.append((sides, corners))
    # A channel entering beside a single land side: the far corner of that water side plus the
    # corner next to the land on it.
    for side in "nesw":
        for entry in "nesw":
            if entry in "ns" and side in "ns" or entry in "ew" and side in "ew":
                continue
            on_entry = {c for c in CORNER_ORDER if entry in CORNER_SIDES[c]}
            combos.append(({side}, on_entry))
    return combos


def recipes(sprites):
    """Yields (name, make) for every generated variation, where make() builds (base, layers)."""
    for sides, corners in all_variations():
        name = variation_name(sides, corners)
        if name in HAND_MADE:
            continue
        base, pieces = recipe(sides, corners)
        yield name, (lambda base=base, pieces=pieces: (
            Layer(sprites[base] if base else sprites["0000"], is_base=True),
            [Layer(sprites[p.sprite], p.du, p.dv, repeat_edges=p.repeat, clip=p.clip, ku=p.ku, kv=p.kv) for p in pieces],
        ))


def main(argv):
    out_dir = SPRITE_DIR
    names = []
    args = argv[1:]
    while args:
        arg = args.pop(0)
        if arg == "--out":
            out_dir = args.pop(0)
        elif arg == "--list":
            print("\n".join(name for name, _ in recipes({})))
            return 0
        else:
            names.append(arg)
    sprites = {}
    for name in list(EDGE.values()) + list(INNER.values()) + list(CORNER.values()) + ["0000"]:
        width, height, rgba = read_png(os.path.join(SPRITE_DIR, name, name + ".png"))
        assert (width, height) == (W, H), name
        sprites[name] = rgba
    generated = 0
    for name, make in recipes(sprites):
        if names and name not in names:
            continue
        base, layers = make()
        path = os.path.join(out_dir, name, name + ".png")
        write_png(path, W, H, compose(base, layers, sprites))
        print(path)
        generated += 1
    print(f"{generated} sprites written.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
