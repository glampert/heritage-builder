"""Building pictograms for the build palette, one per building, in each realm's
architecture.

A glyph is drawn in a 100 x 100 box centred on (0, 0), ground at y = 34. It's a
list of SVG elements in paint order, written with two colours: S, the solid
silhouette, and C, the cuts through it (doors, windows, tile lines). The
treatments in palette.py turn them into masks: a stamp inks S and leaves C bare,
a relief raises S out of the stone, a gilt plaque gilds S on lacquer.
"""

GROUND = 34


def f(v):
    return f"{v:.1f}".rstrip("0").rstrip(".")


def P(d, solid=True, sw=None):
    """A path: filled with S (or C), or stroked when sw is given."""
    c = "S" if solid else "C"
    if sw is None:
        return f'<path d="{d}" fill="{c}"/>'
    return f'<path d="{d}" fill="none" stroke="{c}" stroke-width="{sw}" stroke-linecap="round" stroke-linejoin="round"/>'


def R(x, y, w, h, solid=True, rx=0):
    c = "S" if solid else "C"
    return f'<rect x="{f(x)}" y="{f(y)}" width="{f(w)}" height="{f(h)}" rx="{rx}" fill="{c}"/>'


def Ci(x, y, r, solid=True, sw=None):
    c = "S" if solid else "C"
    if sw is None:
        return f'<circle cx="{f(x)}" cy="{f(y)}" r="{f(r)}" fill="{c}"/>'
    return f'<circle cx="{f(x)}" cy="{f(y)}" r="{f(r)}" fill="none" stroke="{c}" stroke-width="{sw}"/>'


def E(x, y, rx, ry, solid=True):
    c = "S" if solid else "C"
    return f'<ellipse cx="{f(x)}" cy="{f(y)}" rx="{f(rx)}" ry="{f(ry)}" fill="{c}"/>'


# ---------------------------------------------------------------- realm architecture
def roof(realm, cx, y, w, h):
    """A roof whose eaves span w at height y, rising h."""
    xl, xr = cx - w / 2, cx + w / 2
    if realm == "dailong":  # curved tile roof, upturned eaves and ridge ends
        d = (f"M{f(xl - 6)},{f(y - 8)} Q{f(xl - 1)},{f(y + 1)} {f(xl + 9)},{f(y)} H{f(xr - 9)} Q{f(xr + 1)},{f(y + 1)} {f(xr + 6)},{f(y - 8)} "
             f"Q{f(cx + w * 0.3)},{f(y - h * 0.4)} {f(cx + w * 0.3)},{f(y - h)} L{f(cx + w * 0.37)},{f(y - h - 5)} L{f(cx + w * 0.25)},{f(y - h - 1)} "
             f"H{f(cx - w * 0.25)} L{f(cx - w * 0.37)},{f(y - h - 5)} L{f(cx - w * 0.3)},{f(y - h)} Q{f(cx - w * 0.3)},{f(y - h * 0.4)} {f(xl - 6)},{f(y - 8)}Z")
        return [P(d), P(f"M{f(xl + 4)},{f(y - 4)} H{f(xr - 4)}", False, 2.2)]
    if realm == "kambara":  # steep roof, naga finials curling at the eaves, spikes on the ridge
        d = (f"M{f(xl - 3)},{f(y)} Q{f(xl - 9)},{f(y - 9)} {f(xl - 2)},{f(y - 12)} Q{f(xl - 3)},{f(y - 6)} {f(xl + 3)},{f(y - 6)} "
             f"L{f(cx - w * 0.14)},{f(y - h)} H{f(cx + w * 0.14)} L{f(xr - 3)},{f(y - 6)} Q{f(xr + 3)},{f(y - 6)} {f(xr + 2)},{f(y - 12)} "
             f"Q{f(xr + 9)},{f(y - 9)} {f(xr + 3)},{f(y)}Z")
        out = [P(d)]
        for k in (-1, 0, 1):
            sx = cx + k * w * 0.1
            out.append(P(f"M{f(sx - 2.5)},{f(y - h + 0.5)} L{f(sx)},{f(y - h - 6)} L{f(sx + 2.5)},{f(y - h + 0.5)}Z"))
        out.append(P(f"M{f(cx - w * 0.33)},{f(y - h * 0.42)} H{f(cx + w * 0.33)}", False, 2))
        return out
    # Tampadesa: tiered roof (pyatthat), corners upturned, a spire on top
    h1 = h * 0.55
    d1 = (f"M{f(xl - 5)},{f(y - 7)} L{f(xl + 3)},{f(y)} H{f(xr - 3)} L{f(xr + 5)},{f(y - 7)} L{f(cx + w * 0.3)},{f(y - h1)} H{f(cx - w * 0.3)}Z")
    w2 = w * 0.55
    y2 = y - h1 + 1
    d2 = (f"M{f(cx - w2 / 2 - 4)},{f(y2 - 6)} L{f(cx - w2 / 2 + 2)},{f(y2)} H{f(cx + w2 / 2 - 2)} L{f(cx + w2 / 2 + 4)},{f(y2 - 6)} "
          f"L{f(cx + w2 * 0.22)},{f(y - h)} H{f(cx - w2 * 0.22)}Z")
    return [P(d1), P(d2), P(f"M{f(cx)},{f(y - h)} V{f(y - h - 10)}", True, 2.6), Ci(cx, y - h - 11, 2.2),
            P(f"M{f(cx - w * 0.3)},{f(y - h1 + 0.5)} H{f(cx + w * 0.3)}", False, 2)]


def hall(realm, cx, w, bh, rh, door=True, windows=0, ground=GROUND, stilts=False):
    """A building: a body w wide and bh tall on the ground, under the realm's roof."""
    top = ground - bh
    out = []
    if stilts:
        legs = [cx - w / 2 + 3, cx - w / 6, cx + w / 6, cx + w / 2 - 3]
        for lx in legs:
            out.append(R(lx - 2, ground - 10, 4, 10))
        out.append(R(cx - w / 2, top, w, bh - 10))
    else:
        out.append(R(cx - w / 2, top, w, bh))
    if door:
        dw = min(12, w * 0.28)
        db = ground - (10 if stilts else 0)
        dh = min(bh - (10 if stilts else 0) - 4, 16)
        out.append(P(f"M{f(cx - dw / 2)},{f(db)} V{f(db - dh + dw / 2)} A{f(dw / 2)},{f(dw / 2)} 0 0 1 {f(cx + dw / 2)},{f(db - dh + dw / 2)} V{f(db)}Z", False))
    for k in range(windows):
        side = -1 if k % 2 == 0 else 1
        n = k // 2
        wx = cx + side * (w * 0.32 - n * 9)
        out.append(R(wx - 3, top + 5, 6, 7, False))
    out += roof(realm, cx, top + 1, w + 12, rh)
    return out


def plinth(cx, w, ground=GROUND, h=4):
    return [R(cx - w / 2, ground - h, w, h), P(f"M{f(cx - w / 2 + 2)},{f(ground - h + 1.5)} H{f(cx + w / 2 - 2)}", False, 1.4)]


def ground_line(x0=-40, x1=40, y=GROUND + 4):
    return [R(x0, y - 1.5, x1 - x0, 3, rx=1.5)]


# ---------------------------------------------------------------- emblems
def flame(x, y, s=1.0):
    return [P(f"M{f(x)},{f(y - 14 * s)} C{f(x + 9 * s)},{f(y - 5 * s)} {f(x + 8 * s)},{f(y + 3 * s)} {f(x)},{f(y + 4 * s)} "
              f"C{f(x - 8 * s)},{f(y + 3 * s)} {f(x - 9 * s)},{f(y - 4 * s)} {f(x - 3 * s)},{f(y - 9 * s)} C{f(x - 2 * s)},{f(y - 5 * s)} {f(x)},{f(y - 6 * s)} {f(x)},{f(y - 14 * s)}Z"),
            P(f"M{f(x)},{f(y - 4 * s)} C{f(x + 3 * s)},{f(y - 1 * s)} {f(x + 2 * s)},{f(y + 2 * s)} {f(x)},{f(y + 2 * s)} C{f(x - 2 * s)},{f(y + 2 * s)} {f(x - 3 * s)},{f(y)} {f(x)},{f(y - 4 * s)}Z", False)]


def jar(x, y, s=1.0, solid=True):
    """A jar standing on y."""
    d = (f"M{f(x - 4 * s)},{f(y - 18 * s)} H{f(x + 4 * s)} L{f(x + 3 * s)},{f(y - 15 * s)} C{f(x + 11 * s)},{f(y - 12 * s)} {f(x + 11 * s)},{f(y - 2 * s)} {f(x + 5 * s)},{f(y)} "
         f"H{f(x - 5 * s)} C{f(x - 11 * s)},{f(y - 2 * s)} {f(x - 11 * s)},{f(y - 12 * s)} {f(x - 3 * s)},{f(y - 15 * s)}Z")
    return [P(d, solid)]


def coin(x, y, r, solid=True):
    return [Ci(x, y, r, solid), R(x - r * 0.32, y - r * 0.32, r * 0.64, r * 0.64, not solid)]


def rice_plant(x, y, s=1.0):
    return [P(f"M{f(x)},{f(y)} V{f(y - 16 * s)} M{f(x)},{f(y - 4 * s)} Q{f(x - 7 * s)},{f(y - 8 * s)} {f(x - 9 * s)},{f(y - 15 * s)} "
              f"M{f(x)},{f(y - 7 * s)} Q{f(x + 7 * s)},{f(y - 11 * s)} {f(x + 8 * s)},{f(y - 19 * s)} M{f(x)},{f(y - 16 * s)} Q{f(x + 4 * s)},{f(y - 21 * s)} {f(x + 2 * s)},{f(y - 25 * s)}",
              True, 2.6 * s)]


def waves(x0, x1, y, n=3, amp=3, sw=2.6, solid=True):
    step = (x1 - x0) / n
    d = f"M{f(x0)},{f(y)}" + "".join(f" q{f(step / 4)},{f(-amp)} {f(step / 2)},0 t{f(step / 2)},0" for _ in range(n))
    return [P(d, solid, sw)]


def fish(x, y, s=1.0, solid=True):
    return [P(f"M{f(x - 12 * s)},{f(y)} C{f(x - 6 * s)},{f(y - 8 * s)} {f(x + 6 * s)},{f(y - 7 * s)} {f(x + 10 * s)},{f(y)} "
              f"L{f(x + 16 * s)},{f(y - 6 * s)} V{f(y + 6 * s)} L{f(x + 10 * s)},{f(y)} C{f(x + 6 * s)},{f(y + 7 * s)} {f(x - 6 * s)},{f(y + 8 * s)} {f(x - 12 * s)},{f(y)}Z", solid),
            Ci(x - 6 * s, y - 1.5 * s, 1.6 * s, not solid)]


def book(x, y, s=1.0, solid=True):
    """An open book, its spine at x, resting on y."""
    return [P(f"M{f(x)},{f(y - 2 * s)} Q{f(x - 8 * s)},{f(y - 8 * s)} {f(x - 16 * s)},{f(y - 6 * s)} V{f(y + 6 * s)} Q{f(x - 8 * s)},{f(y + 4 * s)} {f(x)},{f(y + 9 * s)} "
              f"Q{f(x + 8 * s)},{f(y + 4 * s)} {f(x + 16 * s)},{f(y + 6 * s)} V{f(y - 6 * s)} Q{f(x + 8 * s)},{f(y - 8 * s)} {f(x)},{f(y - 2 * s)}Z", solid),
            P(f"M{f(x)},{f(y - 1 * s)} V{f(y + 8 * s)}", not solid, 1.6 * s)]


def tree(x, y, s=1.0):
    return [R(x - 2 * s, y - 14 * s, 4 * s, 14 * s), Ci(x, y - 20 * s, 9 * s), Ci(x - 7 * s, y - 14 * s, 6 * s), Ci(x + 7 * s, y - 14 * s, 6 * s)]


def palm(x, y, s=1.0):
    return [P(f"M{f(x)},{f(y)} Q{f(x + 3 * s)},{f(y - 12 * s)} {f(x + 1 * s)},{f(y - 24 * s)}", True, 3 * s),
            P(f"M{f(x + 1 * s)},{f(y - 24 * s)} Q{f(x - 8 * s)},{f(y - 28 * s)} {f(x - 14 * s)},{f(y - 20 * s)} M{f(x + 1 * s)},{f(y - 24 * s)} Q{f(x + 10 * s)},{f(y - 29 * s)} {f(x + 15 * s)},{f(y - 20 * s)} "
              f"M{f(x + 1 * s)},{f(y - 24 * s)} Q{f(x - 3 * s)},{f(y - 33 * s)} {f(x - 9 * s)},{f(y - 33 * s)} M{f(x + 1 * s)},{f(y - 24 * s)} Q{f(x + 6 * s)},{f(y - 33 * s)} {f(x + 12 * s)},{f(y - 31 * s)}", True, 3 * s)]


def bricks(x, y, cols, rows, bw=9, bh=5, solid=True):
    """A stack of bricks, its bottom-left corner at (x, y), each row offset by half a brick."""
    out = []
    for r in range(rows):
        n = cols - r
        x0 = x + r * bw / 2
        yy = y - (r + 1) * bh
        out.append(R(x0, yy, n * bw, bh - 0.8, solid))
        for k in range(1, n):
            out.append(P(f"M{f(x0 + k * bw)},{f(yy)} V{f(yy + bh)}", not solid, 1.3))
    return out


def mountain(x, y, w, h):
    return [P(f"M{f(x - w / 2)},{f(y)} L{f(x - w * 0.12)},{f(y - h)} L{f(x + w * 0.02)},{f(y - h * 0.82)} L{f(x + w * 0.18)},{f(y - h * 0.95)} L{f(x + w / 2)},{f(y)}Z")]


def lotus(x, y, s=1.0, solid=True):
    return [P(f"M{f(x)},{f(y)} C{f(x - 4 * s)},{f(y - 6 * s)} {f(x - 3 * s)},{f(y - 13 * s)} {f(x)},{f(y - 17 * s)} C{f(x + 3 * s)},{f(y - 13 * s)} {f(x + 4 * s)},{f(y - 6 * s)} {f(x)},{f(y)}Z "
              f"M{f(x - 1 * s)},{f(y)} C{f(x - 9 * s)},{f(y - 3 * s)} {f(x - 12 * s)},{f(y - 8 * s)} {f(x - 12 * s)},{f(y - 12 * s)} C{f(x - 7 * s)},{f(y - 10 * s)} {f(x - 3 * s)},{f(y - 6 * s)} {f(x - 1 * s)},{f(y)}Z "
              f"M{f(x + 1 * s)},{f(y)} C{f(x + 9 * s)},{f(y - 3 * s)} {f(x + 12 * s)},{f(y - 8 * s)} {f(x + 12 * s)},{f(y - 12 * s)} C{f(x + 7 * s)},{f(y - 10 * s)} {f(x + 3 * s)},{f(y - 6 * s)} {f(x + 1 * s)},{f(y)}Z", solid)]


# ---------------------------------------------------------------- buildings
def temple(realm):
    if realm == "dailong":  # a two-tiered pagoda hall
        return (plinth(0, 66) + [R(-26, 6, 52, 24)]
                + [P("M-5,30 V20 A5,5 0 0 1 5,20 V30Z", False), R(-19, 12, 6, 8, False), R(13, 12, 6, 8, False)]
                + roof("dailong", 0, 7, 70, 12) + [R(-13, -14, 26, 10)] + roof("dailong", 0, -13, 44, 12) + [Ci(0, -31, 3)])
    if realm == "kambara":  # a prasat: a lotus-bud tower on a stepped base
        out = [R(-34, 26, 68, 8), R(-27, 18, 54, 8), P("M-30,26 H30 M-23,18 H23", False, 1.4)]
        out += [R(-15, -4, 30, 22), P("M-5,18 V8 A5,5 0 0 1 5,8 V18Z", False)]
        tiers = [(-4, 34), (-12, 28), (-19, 22), (-25, 16), (-30, 10)]
        for k, (ty, tw) in enumerate(tiers):
            out.append(P(f"M{-tw / 2},{ty} Q{-tw / 2},{ty - 7} 0,{ty - 9} Q{tw / 2},{ty - 7} {tw / 2},{ty}Z"))
            out.append(P(f"M{-tw / 2 + 2},{ty - 1} H{tw / 2 - 2}", False, 1.4))
        out.append(P("M-3,-37 Q0,-48 3,-37Z"))
        return out
    # Tampadesa: a stupa: terraces, a bell, rings and the umbrella
    out = [R(-36, 26, 72, 8), R(-28, 19, 56, 7), R(-21, 13, 42, 6), P("M-34,26 H34 M-26,19 H26", False, 1.4)]
    out.append(P("M-19,13 C-19,-4 -10,-12 0,-12 C10,-12 19,-4 19,13Z"))
    out.append(P("M-15,4 Q0,-1 15,4", False, 1.6))
    out += [R(-7, -16, 14, 4), R(-5, -20, 10, 4), P("M-4,-20 L0,-38 L4,-20Z"), P("M-7,-31 Q0,-36 7,-31", True, 2.2), Ci(0, -40, 2.2)]
    return out


def shrine(realm):
    if realm == "dailong":  # a little roadside shrine with incense
        return (ground_line(-24, 24) + [R(-4, 12, 8, 24), R(-16, -4, 32, 17), P("M-6,13 V3 H6 V13Z", False)]
                + roof("dailong", 0, -3, 38, 10) + [P("M-3,9 V2 M0,9 V0 M3,9 V2", True, 1.4)]
                + [P("M-3,1 q-2,-4 0,-7 M3,1 q2,-4 0,-7", True, 1.2)])
    if realm == "kambara":  # a neak ta: a small stone sanctuary under a tree
        return (ground_line(-32, 32) + plinth(4, 34, GROUND, 5) + [R(-6, 8, 20, 21), P("M0,29 V19 A4,4 0 0 1 8,19 V29Z", False)]
                + roof("kambara", 4, 9, 22, 11) + tree(-22, GROUND, 1.0))
    # Tampadesa: a nat spirit house on its post
    return (ground_line(-22, 22) + [R(-3, 6, 6, 28), R(-14, 30, 28, 4), R(-16, -6, 32, 13), R(-17, 6, 34, 3), P("M-5,7 V-1 H5 V7Z", False)]
            + roof("tampadesa", 0, -5, 30, 18))


def theater(realm):
    out = ground_line(-40, 40) + [R(-36, 14, 72, 20), P("M-36,20 H36", False, 2)]
    out += [R(-34, -10, 4, 24), R(30, -10, 4, 24)]
    out += roof(realm, 0, -9, 76, 12)
    # The curtain swags and a mask on the stage
    out += [P("M-30,-8 Q-18,4 0,-6 Q18,4 30,-8", True, 3)]
    out += [P("M-9,-2 Q0,-6 9,-2 Q10,8 0,13 Q-10,8 -9,-2Z"), E(-4, 2, 2.4, 1.6, False), E(4, 2, 2.4, 1.6, False),
            P("M-4,8 Q0,10 4,8", False, 1.6)]
    return out


def university(realm):
    if realm == "dailong":  # a pavilion gate with round windows, as at the Temple of Literature
        out = ground_line(-36, 36) + [R(-26, 4, 6, 30), R(20, 4, 6, 30), R(-8, 10, 16, 24), R(-26, 0, 52, 8)]
        out += roof("dailong", 0, 1, 60, 9) + [R(-14, -20, 28, 13), Ci(0, -14, 4.5, False)] + roof("dailong", 0, -19, 36, 9)
        out += [P("M-4,34 V22 A4,4 0 0 1 4,22 V34Z", False)]
        return out
    return (ground_line(-38, 38) + hall(realm, 0, 60, 26, 16, door=False, windows=0)
            + book(0, 22, 1.15, False))


def palace(realm):
    out = ground_line(-42, 42) + [R(-32, 28, 64, 6), R(-24, 22, 48, 6), P("M-30,28 H30 M-22,22 H22", False, 1.4)]
    out += [R(-36, 2, 72, 20)]
    for cx in (-28, -16, 16, 28):
        out.append(R(cx - 2, 5, 4, 14, False))
    out += [P("M-6,22 V12 A6,6 0 0 1 6,12 V22Z", False)]
    out += roof(realm, 0, 3, 80, 11) + [R(-20, -18, 40, 12)] + roof(realm, 0, -17, 50, 12)
    return out


def citadel(realm):
    out = ground_line(-42, 42)
    out += [R(-38, 0, 76, 34)]
    for k in range(-38, 38, 10):
        out.append(R(k + 1, -6, 6, 7))
    out += [R(-42, -14, 16, 48), R(26, -14, 16, 48)]
    for x0 in (-42, 26):
        for k in range(3):
            out.append(R(x0 + 1 + k * 5.5, -20, 3.5, 7))
    out += [P("M-11,34 V14 A11,11 0 0 1 11,14 V34Z", False), P("M-34,12 V20 M34,12 V20", False, 2.6)]
    out += [P("M-16,4 H16", False, 1.4)]
    return out


def house_lot(realm):
    """The vacant lot: a pegged-out plot, and the house to come drawn in outline."""
    out = [P("M-38,28 H38 L34,36 H-34Z"), R(-36, 16, 4, 14), R(32, 16, 4, 14), P("M-34,18 H34", True, 1.6)]
    out += [P("M-20,26 V2 H20 V26", True, 3), P("M-26,4 L0,-18 L26,4", True, 3), P("M-5,26 V14 H5 V26", True, 2.4)]
    return out


def road(realm):
    return [P("M-40,30 Q-10,26 0,6 Q8,-10 40,-18 V-4 Q16,0 10,12 Q2,32 -40,44Z"),
            P("M-30,36 Q-8,30 3,12 Q10,-2 34,-10", False, 2.2), P("M-22,32 l4,4 M-10,26 l5,3 M-1,16 l6,2 M6,4 l5,1 M16,-4 l4,-1 M28,-9 l3,-2", False, 1.6)]


def granary(realm):
    out = ground_line(-38, 38)
    if realm == "dailong":  # a thatched granary raised on stilts
        out += [R(-24, 22, 4, 12), R(20, 22, 4, 12), R(-4, 22, 4, 12), R(-28, 20, 56, 3)]
        out += [R(-22, -2, 44, 22), P("M-22,6 H22 M-22,13 H22", False, 1.4)]
        out += [P("M-30,0 Q-20,-18 0,-26 Q20,-18 30,0Z"), P("M-20,-8 Q0,-18 20,-8", False, 1.6)]
        return out
    if realm == "kambara":  # round baskets and a granary hall
        out += hall(realm, -8, 44, 26, 14, windows=0) + [P("M18,34 Q14,18 24,14 H32 Q40,18 36,34Z"), P("M18,24 H36", False, 1.6)]
        return out
    return out + hall(realm, -6, 48, 26, 20) + [E(26, 26, 9, 8), R(23, 14, 6, 6), P("M18,26 H34", False, 1.4)]


def market(realm):
    out = ground_line(-40, 40)
    # A stall: posts, a striped awning, baskets
    out += [R(-30, 0, 4, 34), R(26, 0, 4, 34)]
    awn = "M-36,2 L-26,-14 H26 L36,2 Q30,7 24,2 Q18,7 12,2 Q6,7 0,2 Q-6,7 -12,2 Q-18,7 -24,2 Q-30,7 -36,2Z"
    if realm == "tampadesa":
        out += [P("M0,-24 V-14", True, 2.4), Ci(0, -26, 2.4)]
    out += [P(awn), P("M-14,-13 L-18,2 M0,-13 V2 M14,-13 L18,2", False, 2)]
    out += [R(-26, 18, 52, 4)]
    out += [P("M-22,18 Q-22,8 -13,8 Q-4,8 -4,18Z"), P("M4,18 Q4,8 13,8 Q22,8 22,18Z")]
    out += [Ci(-16, 7, 2.6), Ci(-11, 6, 2.6), Ci(10, 7, 2.6), Ci(15, 6, 2.6)]
    out += [R(-22, 22, 44, 12, False), R(-20, 24, 40, 10)]
    return out


def rice_farm(realm):
    out = [P("M-40,8 L40,8 L44,34 L-44,34Z"), P("M-36,16 H36 M-38,25 H38", False, 2.2)]
    for x in (-24, -8, 8, 24):
        out += rice_plant(x, 6, 0.9)
    return out


def livestock_farm(realm):
    out = ground_line(-40, 40)
    # A water buffalo's head, horns sweeping back, over a fence
    out += [R(-40, 14, 80, 4), R(-40, 24, 80, 4)]
    for x in (-34, -12, 12, 34):
        out.append(R(x - 2, 8, 4, 26))
    out += [P("M-14,-10 Q-15,6 -6,14 Q0,18 6,14 Q15,6 14,-10 Q0,-16 -14,-10Z")]
    out += [P("M-12,-10 Q-30,-14 -34,-4 Q-28,-24 -6,-16Z"), P("M12,-10 Q30,-14 34,-4 Q28,-24 6,-16Z")]
    out += [E(-6, -3, 2.4, 2, False), E(6, -3, 2.4, 2, False), E(-3, 10, 1.6, 1.2, False), E(3, 10, 1.6, 1.2, False)]
    return out


def fishing_wharf(realm):
    out = []
    # A boat on the waves, a fish above it
    out += [P("M-36,10 Q-30,24 -10,26 H18 Q34,24 38,8 Q20,14 0,14 Q-20,14 -36,10Z")]
    out += [P("M-4,14 V-14", True, 2.6), P("M-2,-14 Q14,-6 16,10 H-2Z")]
    out += waves(-42, 42, 33, 4, 3, 2.6)
    out += fish(-22, -12, 0.9)
    return out


def storage_yard(realm):
    out = ground_line(-40, 40)
    out += [R(-36, -6, 4, 40), R(32, -6, 4, 40)] + roof(realm, 0, -5, 78, 10)
    out += [R(-28, 16, 18, 18), P("M-28,16 L-10,34 M-10,16 L-28,34", False, 1.6)]
    out += [R(-28, 0, 18, 15), P("M-28,0 L-10,15 M-10,0 L-28,15", False, 1.6)]
    out += jar(4, 34, 0.95) + jar(22, 34, 0.95) + jar(13, 17, 0.8)
    return out


def distillery(realm):
    out = ground_line(-40, 40)
    out += hall(realm, -14, 40, 28, 14, windows=0)
    out += jar(20, 34, 1.25)
    out += [P("M20,10 V2 Q20,-6 28,-6 H36 V0", True, 2.6), Ci(36, 6, 2.6), Ci(36, 13, 1.8)]
    return out


def pottery_kiln(realm):
    out = ground_line(-40, 40)
    # A domed kiln with its fire mouth and chimney, a jar beside it
    out += [P("M-32,34 V12 Q-32,-10 -6,-12 Q20,-10 20,12 V34Z"), R(-2, -24, 8, 14)]
    out += [P("M-18,34 V24 A6,6 0 0 1 -6,24 V34Z", False)]
    out += flame(-12, 31, 0.55)
    out += [P("M-26,6 Q-6,-6 14,6", False, 1.6)]
    out += [P("M2,-26 q-3,-6 2,-10 q4,-4 1,-9", True, 2.2)]
    out += jar(30, 34, 1.0)
    return out


def brick_maker(realm):
    out = ground_line(-40, 40)
    out += bricks(-36, 34, 5, 4, 10, 6)
    out += [R(18, 18, 20, 16), R(18, 18, 20, 3, False), R(21, 23, 6, 9, False), R(29, 23, 6, 9, False)]
    out += roof(realm, 28, 16, 26, 10)
    return out


def mine(realm):
    out = ground_line(-42, 42)
    out += mountain(0, 34, 84, 54)
    out += [P("M-14,34 V18 Q-14,4 0,4 Q14,4 14,18 V34Z", False)]
    out += [R(-12, 10, 3, 24), R(9, 10, 3, 24), R(-14, 8, 28, 4)]
    # A pick
    out += [P("M22,-6 Q30,-16 40,-12 Q30,-12 26,-4Z"), P("M28,-12 L18,8", True, 2.6)]
    return out


def clay_pit(realm):
    out = [P("M-42,16 Q-30,30 0,30 Q30,30 42,16 Z"), P("M-30,22 Q0,32 30,22", False, 2)]
    out += [P("M-42,16 H42", True, 2.6)]
    out += [E(-18, 8, 9, 6), E(-2, 4, 8, 6), E(-10, -4, 7, 5)]
    # A spade stuck in the earth
    out += [P("M20,-24 V6", True, 3), P("M14,-26 H26", True, 3), P("M14,4 H26 V12 Q20,20 14,12Z")]
    return out


def lumberyard(realm):
    out = ground_line(-40, 40)
    # Logs stacked, ends showing rings, an axe in the block
    for (x, y) in [(-26, 26), (-10, 26), (6, 26), (-18, 12), (-2, 12), (-10, -2)]:
        out += [Ci(x, y, 8), Ci(x, y, 4, False, 1.4)]
    out += [R(20, 18, 16, 16), P("M20,22 H36", False, 1.4)]
    out += [P("M28,18 L32,-6", True, 3), P("M26,-14 Q36,-16 40,-6 Q34,-8 30,-2Z")]
    return out


def roof_tile_kiln(realm):
    out = ground_line(-40, 40)
    out += [P("M-34,34 V8 Q-34,-8 -14,-8 Q6,-8 6,8 V34Z"), P("M-22,34 V24 A6,6 0 0 1 -10,24 V34Z", False)]
    out += flame(-16, 31, 0.55) + [R(-18, -18, 7, 11)]
    # Curved roof tiles, stacked
    for k in range(4):
        y = 30 - k * 8
        out += [P(f"M10,{y} Q24,{y - 9} 38,{y} V{y + 4} Q24,{y - 5} 10,{y + 4}Z")]
    return out


def police_station(realm):
    out = ground_line(-40, 40) + hall(realm, -6, 44, 26, 15)
    # A spear and a shield
    out += [P("M30,34 V-18", True, 2.6), P("M27,-18 L30,-30 L33,-18Z")]
    out += [P("M22,6 Q30,2 38,6 Q38,20 30,26 Q22,20 22,6Z"), P("M30,8 V22", False, 1.6)]
    return out


def tax_office(realm):
    out = ground_line(-40, 40) + hall(realm, -10, 46, 28, 15, windows=2)
    out += coin(26, 26, 8) + coin(30, 12, 7)
    return out


def apothecary(realm):
    out = ground_line(-40, 40)
    # A mortar and pestle, with a leaf and a gourd
    out += [P("M-26,6 H14 Q14,26 -6,30 Q-26,26 -26,6Z"), R(-10, 30, 8, 4), P("M-24,12 H12", False, 1.6)]
    out += [P("M2,-24 L-8,6", True, 5), Ci(2, -25, 4)]
    out += [P("M20,34 Q16,22 26,18 Q22,14 26,8 Q30,14 26,18 Q36,22 32,34Z")]
    out += [P("M-30,-8 Q-34,-26 -14,-26 Q-16,-10 -30,-8Z"), P("M-30,-8 Q-24,-16 -18,-22", False, 1.4)]
    return out


def hospital(realm):
    out = ground_line(-40, 40) + hall(realm, -8, 52, 30, 16, windows=2)
    # A medicine gourd hung by the door
    out += [P("M29,-2 V4", True, 2), P("M29,4 Q23,9 26,14 Q20,20 24,28 Q29,32 34,28 Q38,20 32,14 Q35,9 29,4Z")]
    out += [P("M26,14 H32", False, 1.4)]
    return out


def fire_watch(realm):
    out = ground_line(-40, 40)
    # A watchtower on four legs, a bucket and a flame
    out += [P("M-20,34 L-12,-6 M20,34 L12,-6 M-17,18 H17 M-14,6 H14", True, 3), R(-18, -14, 36, 10)]
    out += roof(realm, 0, -13, 44, 10)
    out += [P("M-6,-14 V-6 M6,-14 V-6", False, 1.6)]
    out += [P("M26,22 H38 L36,34 H28Z"), P("M26,22 Q32,14 38,22", True, 1.6)]
    out += flame(-31, 34, 0.6)
    return out


def builders_guild(realm):
    out = ground_line(-40, 40)
    # Scaffold, a mallet and a carpenter's square
    out += [P("M-34,34 V-14 M-10,34 V-14 M-34,-4 H-10 M-34,12 H-10 M-34,26 L-10,-4", True, 2.6)]
    out += [R(-6, 18, 30, 16)] + roof(realm, 9, 19, 34, 9)
    out += [P("M18,-30 V0 M18,0 H38", True, 4)]
    out += [P("M30,-12 L40,-24", True, 3), P("M34,-30 L46,-20 L42,-16 L30,-26Z")]
    return out


def small_well(realm):
    out = ground_line(-36, 36)
    out += [R(-16, 14, 32, 20), P("M-16,20 H16 M-16,27 H16 M-6,14 V20 M6,20 V27 M-6,27 V34", False, 1.4)]
    out += [R(-18, 10, 36, 5)]
    out += [R(-16, -10, 3, 20), R(13, -10, 3, 20), R(-20, -12, 40, 3)]
    out += [P("M0,-10 V2", True, 1.4), P("M-4,2 H4 L3,9 H-3Z")]
    return out


def large_well(realm):
    out = ground_line(-40, 40)
    out += [R(-26, 14, 52, 20), P("M-26,21 H26 M-26,28 H26 M-14,14 V21 M0,21 V28 M14,14 V21 M-14,28 V34 M14,28 V34", False, 1.4)]
    out += [R(-28, 10, 56, 5), R(-24, -4, 3, 14), R(21, -4, 3, 14)]
    out += roof(realm, 0, -3, 62, 12)
    out += [P("M0,-2 V3", True, 1.4), P("M-4,3 H4 L3,9 H-3Z")]
    return out


def garden(realm):
    out = [P("M-40,24 Q-20,16 0,22 Q22,28 40,20 V34 H-40Z")]
    out += [E(10, 28, 18, 4, False)]
    out += lotus(8, 26, 0.7)
    if realm == "kambara":
        out += palm(-24, 22, 1.0)
    elif realm == "tampadesa":
        out += tree(-24, 22, 1.2) + [P("M26,22 V4", True, 2), P("M20,4 Q26,-4 32,4Z")]
    else:
        # A twisted pine, as in a bonsai garden, and a lantern
        out += [P("M-26,22 Q-30,8 -20,2 Q-12,-4 -22,-12", True, 4), E(-12, -2, 10, 4), E(-26, -12, 9, 4), E(-16, -20, 7, 3.4)]
        out += [R(26, 6, 4, 16), R(21, 0, 14, 6), P("M19,0 L28,-8 L37,0Z")]
    return out


BUILDINGS = {
    "vacant_lot": ("Vacant lot", house_lot),
    "road": ("Road", road),
    "granary": ("Granary", granary),
    "market": ("Market", market),
    "rice_farm": ("Rice farm", rice_farm),
    "livestock_farm": ("Livestock farm", livestock_farm),
    "fishing_wharf": ("Fishing wharf", fishing_wharf),
    "storage_yard": ("Storage yard", storage_yard),
    "distillery": ("Distillery", distillery),
    "pottery_kiln": ("Pottery kiln", pottery_kiln),
    "brick_maker": ("Brick maker", brick_maker),
    "mine": ("Mine", mine),
    "clay_pit": ("Clay pit", clay_pit),
    "lumberyard": ("Lumberyard", lumberyard),
    "roof_tile_kiln": ("Roof-tile kiln", roof_tile_kiln),
    "police_station": ("Guard post", police_station),
    "tax_office": ("Tax office", tax_office),
    "apothecary": ("Apothecary", apothecary),
    "hospital": ("Hospital", hospital),
    "fire_watch": ("Fire watch", fire_watch),
    "builders_guild": ("Builders' guild", builders_guild),
    "small_well": ("Small well", small_well),
    "large_well": ("Large well", large_well),
    "governor_palace": ("Governor's palace", palace),
    "citadel": ("Citadel", citadel),
    "shrine": ("Shrine", shrine),
    "temple": ("Temple", temple),
    "theater": ("Theatre", theater),
    "university": ("University", university),
    "garden": ("Garden", garden),
}


def glyph(name, realm):
    return BUILDINGS[name][1](realm)
