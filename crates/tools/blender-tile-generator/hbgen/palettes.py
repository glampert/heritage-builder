"""Palettes and the tuned look: lighting, outlines, grade and Eevee settings.

Palette colours are sRGB hex. lin() converts them to linear before they reach
shader nodes; feeding hex-as-linear produces the washed-out pastel look."""

PALETTES = {
    "viet": {
        "roof":      ("3e170c", "82402a", "b8683e"),   # shadow / mid / highlight
        "ridge":     "2a1a10",
        "stone":     ("a49e8a", "7c766a", "3e3a32"),   # block A / block B / mortar
        "stoneDark": ("7e7865", "5c5748", "32302a"),
        "pave":      ("b2a88c", "9a9075", "5a5343"),
        "wood":      ("4a2a16", "8a5630"),
        "woodDark":  ("26140a", "4e2e18"),
        "grass":     ("4a6e26", "7c9e4a"),
        "leaf":      ("3a6a2a", "7eaa56"),
        "trunk":     ("3a2415", "5a3a22"),
        "earth":     ("36241a", "62452c"),
        "water":     ("142c48", "346890"),
        "lantern":   "d83c1c",
        "gold":      "d4902e",
        "archDark":  "120c08",
        # Workshop and yard materials (first used by the roof-tile kiln).
        "brick":      ("a85a3a", "8a4630", "4e3226"),   # fired brick A / B / mortar
        "terracotta": ("5a2412", "a4502e", "d08050"),   # roof tiles: shadow / mid / highlight
        "yard":       ("a87e4e", "cfa26a"),             # packed sandy earth
        "clay":       ("5e5246", "8a7a66"),             # raw clay
        "fire":       "ff8c2a",
        "plaster":    ("b88a44", "d8b064"),             # ochre lime plaster
        "roofGrey":   ("2a221c", "5a4e44", "8c7e6c"),   # weathered grey-brown tiles, as on the hand-made workshops
        "bamboo":     ("8a7636", "c9b46a"),             # scaffold poles
        "rope":       "e2d6b0",                         # builders' string lines
    },
    # Bagan-inspired: red brick, teak, sandstone paving, gilded central shikhara.
    "burma": {
        "roof":      ("3a1a0e", "6e3420", "9a5030"),
        "ridge":     "26130a",
        "stone":     ("9a4a34", "7a3626", "4a2016"),   # red brick A / B / mortar
        "stoneDark": ("7a3a2a", "5e2c1e", "3a1810"),
        "pave":      ("c4ae86", "a8936c", "7a6a4a"),   # sandstone flags
        "wood":      ("4a2a16", "8a5630"),
        "woodDark":  ("26140a", "4e2e18"),
        "grass":     ("4a6e26", "7c9e4a"),
        "leaf":      ("3a6a2a", "7eaa56"),
        "trunk":     ("3a2415", "5a3a22"),
        "earth":     ("36241a", "62452c"),
        "water":     ("142c48", "346890"),
        "lantern":   "d83c1c",
        "gold":      "e0a63a",
        "goldStone": ("c99a2e", "a87c22", "6a4c14"),   # gilded carved masonry
        "sandstone": ("a8926a", "d2bc92"),
        "archDark":  "120c08",
    },
    # Angkor-inspired: grey laterite/sandstone with moss, red-tiled galleries.
    "khmer": {
        "roof":      ("3e1e12", "7a3a24", "985232"),
        "ridge":     "2a1a10",
        "stone":     ("aaa697", "8c8878", "5a574c"),
        "stoneDark": ("807d70", "64625a", "3e3d38"),
        "pave":      ("aca898", "928e7e", "605d54"),
        "wood":      ("4a2a16", "8a5630"),
        "woodDark":  ("26140a", "4e2e18"),
        "grass":     ("4a6e26", "7c9e4a"),
        "leaf":      ("3a6a2a", "7eaa56"),
        "trunk":     ("3a2415", "5a3a22"),
        "earth":     ("36241a", "62452c"),
        "water":     ("142c48", "346890"),
        "lantern":   "d83c1c",
        "gold":      "d4902e",
        "moss":      ("3e6020", "6a8c36"),             # overlay on all stone materials
        "archDark":  "120c08",
    },
}

LIGHTING = dict(
    sun_energy=5.4, sun_color=(1.0, 0.93, 0.82), sun_rot=(50, -10, 210), sun_angle=2,
    fill_energy=0.8, fill_color=(0.9, 0.93, 1.0), fill_rot=(60, 0, 40),
    world_color=(0.82, 0.80, 0.74), world_strength=0.37,
)
OUTLINE = dict(thickness=1.6, color=(0.09, 0.05, 0.03), alpha=0.9, noise_amp=0.6, noise_period=8, jitter_amp=0.7, jitter_freq=12)
GRADE = dict(saturation=1.0, contrast=0.08)
EEVEE = dict(samples=96, fast_gi_distance=1.5)


def lin(h):
    r, g, b = [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    f = lambda c: c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
    return (f(r), f(g), f(b), 1.0)

