"""Checks on rendered sprites, and the contact sheet. Standard library only.

Each sprite must match the engine's format (see README §10):
- the width is logical width x density, the height a multiple of density;
- the top row is empty (nothing clipped above);
- the diamond's left and right corners touch the sides, a quarter of the width
  above the bottom, give or take the ink (one logical px);
- the bottom row holds only the footprint's front corner, at the centre.
Brightness and saturation are compared with the palette's targets; a miss is a
warning, since buildings differ (a sandy yard is brighter than a stone court).
"""

import colorsys
import os

from . import pngio

# Medians over opaque pixels: (target, tolerance). Luminance is Rec. 709 luma of the
# stored (gamma-encoded) values, as Blender's image.pixels returns them for a PNG,
# which is how README §5 measured it. Measured: vn_fortress 0.29 / 0.47, the
# roof-tile kiln 0.26 / 0.49; the hand-made pottery kiln 0.29 / 0.39 and brick maker 0.35 / 0.61.
TARGETS = {"viet": dict(luminance=(0.29, 0.08), saturation=(0.48, 0.15))}


def _median(xs):
    xs = sorted(xs)
    return xs[len(xs) // 2] if xs else 0.0


def check_sprite(path, rec, palette="viet", full_width=True):
    """(errors, warnings, stats) for one sprite. rec is its manifest record."""
    errors, warnings = [], []
    w, h, px = pngio.read_rgba(path)
    d = rec["density"]
    lw = rec["logical_size"]["width"]
    A = lambda x, y: px[(y * w + x) * 4 + 3]
    if w != lw * d:
        errors.append(f"width {w} isn't the logical width {lw} x density {d}")
    if h % d:
        errors.append(f"height {h} isn't a multiple of the density {d}")
    if any(A(x, 0) for x in range(w)):
        errors.append("the top row isn't empty: the model may be clipped at the top")
    if full_width:
        for side, xs in (("left", range(0, d)), ("right", range(w - d, w))):
            rows = [y for y in range(h) if any(A(x, y) > 127 for x in xs)]
            if not rows:
                errors.append(f"nothing touches the {side} edge: the ground diamond should span the width")
                continue
            expected = h - 1 - w // 4                     # corner row, counted from the top
            if abs(max(rows) - expected) > d:
                errors.append(f"the {side} corner is at row {max(rows)}, expected {expected} (± {d}): the anchor is off")
    bottom = [x for x in range(w) if A(x, h - 1) > 127]
    if not bottom:
        errors.append("the bottom row is empty: the footprint's front corner should touch it")
    elif min(bottom) < w // 2 - 2 * d or max(bottom) > w // 2 + 2 * d:
        errors.append(f"the bottom row is opaque from x {min(bottom)} to {max(bottom)}; only the front corner, "
                      f"around x {w // 2}, should be: something reaches below the footprint")
    lums, sats = [], []
    for i in range(0, w * h, 3):
        o = i * 4
        if px[o + 3] >= 250:
            r, g, b = px[o], px[o + 1], px[o + 2]
            lums.append((0.2126 * r + 0.7152 * g + 0.0722 * b) / 255)
            sats.append(colorsys.rgb_to_hsv(r / 255, g / 255, b / 255)[1])
    stats = dict(luminance=round(_median(lums), 3), saturation=round(_median(sats), 3), size=[w, h])
    for k, (target, tol) in TARGETS.get(palette, {}).items():
        if abs(stats[k] - target) > tol:
            warnings.append(f"median {k} {stats[k]} is outside {target} ± {tol} for the '{palette}' palette")
    return errors, warnings, stats


def contact_sheet(out_path, paths, scale_px=2, columns=4, pad=8, densities=None):
    """Every sprite at scale_px px per logical px over a checkerboard, in a grid.
    densities maps a path to its density (default 8)."""
    thumbs = []
    for p in paths:
        w, h, px = pngio.read_rgba(p)
        f = max(1, (densities or {}).get(p, 8) // scale_px)
        tw, th = w // f, h // f
        t = bytearray(tw * th * 4)
        n = f * f
        for ty in range(th):
            for tx in range(tw):
                acc = [0, 0, 0, 0]
                for yy in range(ty * f, ty * f + f):
                    base = (yy * w + tx * f) * 4
                    for xx in range(f):
                        o = base + xx * 4; a = px[o + 3]
                        acc[0] += px[o] * a; acc[1] += px[o + 1] * a; acc[2] += px[o + 2] * a; acc[3] += a
                o = (ty * tw + tx) * 4
                if acc[3]:
                    t[o:o + 4] = bytes((acc[0] // acc[3], acc[1] // acc[3], acc[2] // acc[3], acc[3] // n))
        thumbs.append((tw, th, t))
    if not thumbs:
        return None
    cw = max(t[0] for t in thumbs) + pad; ch = max(t[1] for t in thumbs) + pad
    cols = min(columns, len(thumbs)); rows = (len(thumbs) + cols - 1) // cols
    W, H = cols * cw + pad, rows * ch + pad
    sheet = bytearray(W * H * 4)
    for y in range(H):
        for x in range(W):
            v = 205 if ((x // 12) + (y // 12)) % 2 == 0 else 165
            o = (y * W + x) * 4; sheet[o:o + 4] = bytes((v, v, v, 255))
    for i, (tw, th, t) in enumerate(thumbs):
        ox = pad + (i % cols) * cw; oy = pad + (i // cols) * ch + (ch - pad - th)    # bottoms aligned per row
        for y in range(th):
            for x in range(tw):
                s = (y * tw + x) * 4; a = t[s + 3]
                if a:
                    o = ((oy + y) * W + ox + x) * 4
                    for c in range(3):
                        sheet[o + c] = (t[s + c] * a + sheet[o + c] * (255 - a)) // 255
        for x in range(tw):                                 # a red line on each sprite's bottom edge
            o = ((oy + th) * W + ox + x) * 4
            if oy + th < H: sheet[o:o + 4] = b"\xd0\x30\x30\xff"
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    pngio.write_rgba(out_path, W, H, sheet)
    return out_path
