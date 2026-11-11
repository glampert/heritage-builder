"""The ui-art catalog (art.json) and manifest (manifest.json), and the checks that need no
browser. Standard library only: CI runs `make_ui_art.py --check` and the unit tests without
Pillow, numpy or Playwright.

A catalog entry draws one sprite per realm it lists, and one PNG per state:

    {
      "id": "button_primary",              # unique per realm; [a-z0-9_]
      "group": "buttons",                  # free text, for --list
      "call": "button",                    # a drawing call (render.CALLS, KNOWN_CALLS below)
      "args": { "primary": true },         # the call's other parameters
      "size": [160, 56],                   # the widget rect the call draws, in points
      "bleed": [2, 2, 5, 7],               # how far the sprite reaches past that rect, in points:
                                           # left, top, right, bottom; negative crops into it
      "slice": [24, 24, 32, 36],           # 9-slice borders in pixels of the PNG (l, t, r, b)
      "center": "stretch",                 # or "tile": the middle and edges repeat; drawn tileable
      "states": ["idle", "hovered", "pressed", "disabled"],
      "realms": { "dailong": {}, "kambara": { "bleed": [...] } }   # per-realm overrides
    }

The PNG is (size + bleed) x scale pixels, at assets/ui/skins/<realm>/<id>.png, or
<id>_<state>.png when the entry has states. An entry's own "scale" sets its pixels per point (it's
shot at the catalog's and resized), and "format": "jpeg" writes an opaque .jpg instead (the home
backdrops: a full-screen picture at 1 pixel per point). More keys: "elements" (keep only these top-level
SVG elements of the call's output), "capture" (keep only what one helper drew during the call),
"omit" (helpers that draw nothing), "attrs" (theme attributes to set first), "rotate" (90: the
drawing turned a quarter clockwise about the rect's centre, so the sprite is height x width),
"out" (another output path pattern) and, per state, "css" (a CSS filter), "args" and "call".
"""

import copy
import hashlib
import json
import os
import re
import struct

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(HERE)))
CATALOG_PATH = os.path.join(HERE, "art.json")
MANIFEST_PATH = os.path.join(HERE, "manifest.json")
UI_DIR = "assets/ui"
SKIN_ART_DIR = "skins"                 # under assets/ui
SKIN_CONFIG_DIR = "assets/configs/ui/skins"
WEB_MANIFEST = "web/asset_manifest.json"
MANIFEST_VERSION = 1

REALMS = ("dailong", "kambara", "tampadesa")
KNOWN_STATES = ("idle", "hovered", "pressed", "disabled", "selected", "locked", "current")
CENTERS = ("stretch", "tile")
# Output formats and their file extensions: PNG with alpha for sprites, JPEG for opaque pictures.
FORMATS = {"png": ".png", "jpeg": ".jpg"}

# The drawing calls the renderer knows (render.CALLS has one function per name).
KNOWN_CALLS = (
    "mpanel", "box", "bar_bg", "panel_bg", "card", "row_bg", "text_input", "caption", "speech",
    "button", "tab", "item_marker", "icon_button", "build_button", "checkbox", "toggle", "slider",
    "dropdown", "divider", "realm_emblem", "seal", "home_scene",
)

TOP_KEYS = {"version", "scale", "pad", "realms", "state_filters", "sprites", "notes"}
ENTRY_KEYS = {"id", "group", "call", "args", "size", "bleed", "slice", "center", "states", "realms",
              "elements", "capture", "omit", "attrs", "rotate", "out", "scale", "format", "notes"}
STATE_KEYS = {"css", "args", "call"}
DEFAULT_OUT = "skins/{realm}/{id}%s"
DEFAULT_STATE_OUT = "skins/{realm}/{id}_{state}%s"
ID_RE = re.compile(r"^[a-z0-9_]+$")


class CatalogError(ValueError):
    pass


# ----------------------------------------------------------------------------
# Loading and validating
# ----------------------------------------------------------------------------
def load_catalog(path=CATALOG_PATH):
    with open(path, encoding="utf-8") as f:
        raw = json.load(f)
    return expand(raw)


def _num(v):
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def _int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def expand(raw):
    """The catalog's sprites, one record per (realm, id), with every default filled in and
    every output path. Raises CatalogError on anything malformed."""
    if not isinstance(raw, dict):
        raise CatalogError("the catalog must be a JSON object")
    unknown = set(raw) - TOP_KEYS
    if unknown:
        raise CatalogError(f"unknown top-level keys: {sorted(unknown)}")
    if raw.get("version", 1) != 1:
        raise CatalogError("version must be 1")
    scale = raw.get("scale", 2)
    pad = raw.get("pad", 80)
    if not _num(scale) or scale <= 0:
        raise CatalogError("scale must be a positive number")
    if not _num(pad) or pad < 0:
        raise CatalogError("pad must be a number >= 0")
    realms = raw.get("realms", list(REALMS))
    for r in realms:
        if r not in REALMS:
            raise CatalogError(f"unknown realm {r!r}")
    filters = raw.get("state_filters", {})
    for s, css in filters.items():
        if s not in KNOWN_STATES:
            raise CatalogError(f"state_filters: unknown state {s!r}")
        if not isinstance(css, str):
            raise CatalogError(f"state_filters.{s}: must be a CSS filter string")
    sprites = raw.get("sprites")
    if not isinstance(sprites, list):
        raise CatalogError("sprites must be a list")

    records, seen_ids, seen_out = [], set(), {}
    for n, entry in enumerate(sprites):
        where = f"sprites[{n}]"
        if not isinstance(entry, dict):
            raise CatalogError(f"{where}: must be an object")
        sid = entry.get("id")
        if not isinstance(sid, str) or not ID_RE.match(sid):
            raise CatalogError(f"{where}: id must match {ID_RE.pattern}")
        where = f"{sid}"
        unknown = set(entry) - ENTRY_KEYS
        if unknown:
            raise CatalogError(f"{where}: unknown keys {sorted(unknown)}")
        per_realm = entry.get("realms")
        if per_realm is None:
            per_realm = {r: {} for r in realms}
        if not isinstance(per_realm, dict) or not per_realm:
            raise CatalogError(f"{where}: realms must be a non-empty object of realm -> overrides")
        for realm, over in per_realm.items():
            if realm not in realms:
                raise CatalogError(f"{where}: unknown realm {realm!r}")
            if not isinstance(over, dict):
                raise CatalogError(f"{where}.realms.{realm}: must be an object")
            bad = set(over) - (ENTRY_KEYS - {"id", "realms"})
            if bad:
                raise CatalogError(f"{where}.realms.{realm}: unknown keys {sorted(bad)}")
            merged = {k: v for k, v in entry.items() if k != "realms"}
            for k, v in over.items():
                if k == "args" and isinstance(v, dict):
                    if not isinstance(merged.get("args", {}), dict):
                        raise CatalogError(f"{realm}/{sid}: args must be an object")
                    merged["args"] = {**merged.get("args", {}), **v}
                else:
                    merged[k] = v
            if (realm, sid) in seen_ids:
                raise CatalogError(f"{where}: id used twice in {realm}")
            seen_ids.add((realm, sid))
            rec = _record(merged, realm, scale, pad, filters, f"{realm}/{sid}")
            for st in rec["states"]:
                out = st["out"]
                if out in seen_out:
                    raise CatalogError(f"{realm}/{sid}: output {out} is also {seen_out[out]}'s")
                seen_out[out] = f"{realm}/{sid}"
            records.append(rec)
    return records


def _record(e, realm, shot_scale, pad, filters, where):
    call = e.get("call")
    if call not in KNOWN_CALLS:
        raise CatalogError(f"{where}: unknown call {call!r}")
    args = e.get("args", {})
    if not isinstance(args, dict):
        raise CatalogError(f"{where}: args must be an object")
    size = e.get("size")
    if not (isinstance(size, list) and len(size) == 2 and all(_num(v) and v > 0 for v in size)):
        raise CatalogError(f"{where}: size must be [width, height] in points, both > 0")
    bleed = e.get("bleed", [0, 0, 0, 0])
    if not (isinstance(bleed, list) and len(bleed) == 4 and all(_num(v) for v in bleed)):
        raise CatalogError(f"{where}: bleed must be [left, top, right, bottom] in points")
    # The output's pixels per point: the catalog's, or the entry's own (shot at the catalog's
    # scale and resized), e.g. 1 for a full-screen backdrop.
    scale = e.get("scale", shot_scale)
    if not _num(scale) or scale <= 0:
        raise CatalogError(f"{where}: scale must be a positive number")
    fmt = e.get("format", "png")
    if fmt not in FORMATS:
        raise CatalogError(f"{where}: format must be one of {sorted(FORMATS)}")
    for v in list(size) + list(bleed):
        if abs(v * scale - round(v * scale)) > 1e-6:
            raise CatalogError(f"{where}: size and bleed must be whole pixels at scale {scale:g}, got {v:g} pt")
    rotate = e.get("rotate", 0)
    if rotate not in (0, 90):
        raise CatalogError(f"{where}: rotate must be 0 or 90")
    base = list(size) if rotate == 0 else [size[1], size[0]]
    sw = base[0] + bleed[0] + bleed[2]
    sh = base[1] + bleed[1] + bleed[3]
    if sw <= 0 or sh <= 0:
        raise CatalogError(f"{where}: size plus bleed must be positive")
    pw, ph = sw * scale, sh * scale
    if abs(pw - round(pw)) > 1e-6 or abs(ph - round(ph)) > 1e-6:
        raise CatalogError(f"{where}: (size + bleed) x scale must be whole pixels, got {pw} x {ph}")
    png_size = [int(round(pw)), int(round(ph))]
    if any(b >= pad for b in bleed):
        raise CatalogError(f"{where}: a bleed of {pad} points or more doesn't fit the page's pad")

    slc = e.get("slice")
    center = e.get("center")
    if slc is not None:
        if not (isinstance(slc, list) and len(slc) == 4 and all(_int(v) and v >= 0 for v in slc)):
            raise CatalogError(f"{where}: slice must be [left, top, right, bottom], whole pixels >= 0")
        if slc[0] + slc[2] > png_size[0] or slc[1] + slc[3] > png_size[1]:
            raise CatalogError(f"{where}: slice {slc} doesn't fit the {png_size[0]}x{png_size[1]} px sprite")
        center = center or "stretch"
        if center not in CENTERS:
            raise CatalogError(f"{where}: center must be one of {CENTERS}")
        if center == "tile" and (slc[0] + slc[2] >= png_size[0] or slc[1] + slc[3] >= png_size[1]):
            raise CatalogError(f"{where}: a tiled 9-slice needs a middle; slice {slc} leaves none")
    elif center is not None:
        raise CatalogError(f"{where}: center without slice")

    elements = e.get("elements")
    if elements is not None and not (isinstance(elements, list) and elements and all(_int(v) and v >= 0 for v in elements)):
        raise CatalogError(f"{where}: elements must be a list of element indices")
    cap = e.get("capture")
    if cap is not None and not (isinstance(cap, dict) and set(cap) <= {"helper", "index"}
                                and isinstance(cap.get("helper"), str) and _int(cap.get("index", 0))):
        raise CatalogError(f"{where}: capture must be {{\"helper\": name, \"index\": n}}")
    omit = e.get("omit", [])
    if not (isinstance(omit, list) and all(isinstance(v, str) for v in omit)):
        raise CatalogError(f"{where}: omit must be a list of helper names")
    attrs = e.get("attrs", {})
    if not isinstance(attrs, dict):
        raise CatalogError(f"{where}: attrs must be an object")

    states_raw = e.get("states")
    states = []
    if states_raw is None:
        names = [None]
        spec = {}
    elif isinstance(states_raw, list):
        names = states_raw
        spec = {s: {} for s in states_raw}
    elif isinstance(states_raw, dict):
        names = list(states_raw)
        spec = states_raw
    else:
        raise CatalogError(f"{where}: states must be a list or an object")
    if states_raw is not None and not names:
        raise CatalogError(f"{where}: states is empty")
    pattern = e.get("out", (DEFAULT_OUT if states_raw is None else DEFAULT_STATE_OUT) % FORMATS[fmt])
    if not isinstance(pattern, str):
        raise CatalogError(f"{where}: out must be a path pattern")
    for name in names:
        if name is not None and name not in KNOWN_STATES:
            raise CatalogError(f"{where}: unknown state {name!r} (known: {', '.join(KNOWN_STATES)})")
        st = spec.get(name, {}) if name is not None else {}
        if not isinstance(st, dict) or set(st) - STATE_KEYS:
            raise CatalogError(f"{where}: state {name!r} takes only {sorted(STATE_KEYS)}")
        css = st.get("css", filters.get(name, "") if name else "")
        if not isinstance(css, str):
            raise CatalogError(f"{where}: state {name!r}: css must be a CSS filter string")
        st_call = st.get("call", call)
        if st_call not in KNOWN_CALLS:
            raise CatalogError(f"{where}: state {name!r}: unknown call {st_call!r}")
        if not isinstance(st.get("args", {}), dict):
            raise CatalogError(f"{where}: state {name!r}: args must be an object")
        st_args = {**args, **st.get("args", {})}
        try:
            out = pattern.format(realm=realm, id=e["id"], state=name or "")
        except (KeyError, IndexError) as err:
            raise CatalogError(f"{where}: out pattern {pattern!r}: unknown field {err}")
        if not out_ok(out, fmt):
            raise CatalogError(f"{where}: output {out!r} must be a {FORMATS[fmt]} under {SKIN_ART_DIR}/")
        states.append({"state": name, "css": css, "call": st_call, "args": st_args, "out": out})
    if len({s["out"] for s in states}) != len(states):
        raise CatalogError(f"{where}: two states share an output path; put {{state}} in out")

    return {
        "realm": realm, "id": e["id"], "group": e.get("group", ""), "call": call, "args": args,
        "size": list(size), "bleed": list(bleed), "rotate": rotate, "png_size": png_size, "scale": scale, "pad": pad,
        "shot_scale": shot_scale, "format": fmt,
        "slice": list(slc) if slc is not None else None, "center": center,
        "elements": e.get("elements"), "capture": cap, "omit": omit, "attrs": attrs,
        "states": states,
    }


def out_ok(out, fmt="png"):
    """An output path, relative to assets/ui, must be inside skins/, with its format's extension
    (.png, or .jpg for JPEG)."""
    if not out.endswith(FORMATS[fmt]) or "\\" in out or out.startswith("/"):
        return False
    parts = out.split("/")
    return parts[0] == SKIN_ART_DIR and len(parts) >= 2 and all(p not in ("", ".", "..") for p in parts)


def repo_path(out):
    """The repository-relative path of an output path."""
    return f"{UI_DIR}/{out}"


def outputs(records):
    """(record, state, repository-relative path) for every PNG the catalog makes."""
    for rec in records:
        for st in rec["states"]:
            yield rec, st, repo_path(st["out"])


def select(records, realms=None, ids=None):
    """The records for these realms and ids (all when None). An id that none of the chosen
    realms has raises CatalogError."""
    in_realms = [r for r in records if not realms or r["realm"] in realms]
    if ids:
        known = {r["id"] for r in in_realms}
        missing = [i for i in ids if i not in known]
        if missing:
            where = f" in {', '.join(realms)}" if realms else ""
            raise CatalogError(f"no sprite with id {', '.join(missing)}{where}")
    return [r for r in in_realms if not ids or r["id"] in ids]


# What run() and --prune do with one output. Pure, so the rule that hand-made art is never
# overwritten or deleted without --force is unit-tested.
RENDER, UNCHANGED, SKIP_UNLISTED, SKIP_CHANGED = "render", "unchanged", "skip_unlisted", "skip_changed"
DELETE, KEEP_CHANGED, GONE = "delete", "keep_changed", "gone"


def render_decision(exists, known, current_hash, source_hash, force, entry_hash=None):
    """Whether to render one output. exists: the file is on disk; known: its manifest entry
    (None when the manifest doesn't list it); current_hash: the file's hash (when it exists);
    source_hash: the hash of what would be drawn now; entry_hash: its catalog_hash() now. An
    entry the manifest records no catalog hash for counts as unchanged if its source is."""
    if not exists or force:
        return RENDER
    if known is None:
        return SKIP_UNLISTED
    if current_hash != known.get("png_hash"):
        return SKIP_CHANGED
    catalog_same = entry_hash is None or known.get("catalog_hash") in (None, entry_hash)
    if known.get("source_hash") == source_hash and catalog_same:
        return UNCHANGED
    return RENDER


# What of a record and state its catalog_hash covers: everything the catalog says about drawing
# and shooting it (the SVG's source hash doesn't cover the state's CSS filter, the crop or the
# scale). Changes to the generator itself only show in the source hash, which takes a render.
CATALOG_HASH_KEYS = ("realm", "call", "args", "size", "bleed", "rotate", "png_size", "scale", "pad",
                     "slice", "center", "elements", "capture", "omit", "attrs")
STATE_HASH_KEYS = ("state", "css", "call", "args", "out")


def catalog_hash(rec, st):
    """A hash of what the catalog says about one output, which the manifest records when it's
    rendered, so --check can tell a sprite whose catalog entry changed since. The format and the
    shot's scale count only when they aren't the defaults (PNG, shot at the output's scale), so
    the sprites from before they existed keep their hashes."""
    data = {k: rec[k] for k in CATALOG_HASH_KEYS}
    if rec.get("format", "png") != "png":
        data["format"] = rec["format"]
    if rec.get("shot_scale", rec["scale"]) != rec["scale"]:
        data["shot_scale"] = rec["shot_scale"]
    data["state"] = {k: st[k] for k in STATE_HASH_KEYS}
    return hashlib.sha256(json.dumps(data, sort_keys=True).encode()).hexdigest()[:16]


def prune_decision(exists, known, current_hash, force):
    """What --prune does with an output the catalog no longer has: delete it only when it's
    still what this script wrote (or with --force); a changed file is kept as hand-made art.
    Either way its manifest entry goes."""
    if not exists:
        return GONE
    if force or current_hash == known.get("png_hash"):
        return DELETE
    return KEEP_CHANGED


# ----------------------------------------------------------------------------
# Manifest
# ----------------------------------------------------------------------------
def load_manifest(path=MANIFEST_PATH):
    if not os.path.exists(path):
        return {"version": MANIFEST_VERSION, "sprites": {}}
    with open(path, encoding="utf-8") as f:
        m = json.load(f)
    if m.get("version") != MANIFEST_VERSION or not isinstance(m.get("sprites"), dict):
        raise CatalogError(f"{path}: not a version {MANIFEST_VERSION} ui-art manifest")
    return m


def save_manifest(manifest, path=MANIFEST_PATH):
    data = {"version": MANIFEST_VERSION, "sprites": dict(sorted(manifest["sprites"].items()))}
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, sort_keys=True)
        f.write("\n")


def file_hash(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()[:16]


def png_size(path):
    """(width, height) from a PNG's header, without decoding it."""
    with open(path, "rb") as f:
        head = f.read(24)
    if head[:8] != b"\x89PNG\r\n\x1a\n" or head[12:16] != b"IHDR":
        raise ValueError(f"{path}: not a PNG")
    return list(struct.unpack(">II", head[16:24]))


# The JPEG markers that start a frame (SOF0 to SOF15, but DHT, JPG and DAC), whose header holds
# the image's size.
_JPEG_SOF = {0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xC6, 0xC7, 0xC9, 0xCA, 0xCB, 0xCD, 0xCE, 0xCF}


def jpeg_size(path):
    """(width, height) from a JPEG's frame header, without decoding it."""
    with open(path, "rb") as f:
        data = f.read()
    if data[:2] != b"\xff\xd8":
        raise ValueError(f"{path}: not a JPEG")
    i = 2
    while i + 4 <= len(data):
        if data[i] != 0xFF:
            raise ValueError(f"{path}: broken JPEG marker at byte {i}")
        marker = data[i + 1]
        if marker == 0xFF:  # Fill byte.
            i += 1
            continue
        if marker in (0xD8, 0x01) or 0xD0 <= marker <= 0xD7:  # No length.
            i += 2
            continue
        (length,) = struct.unpack(">H", data[i + 2:i + 4])
        if marker in _JPEG_SOF:
            if i + 9 > len(data):
                break
            height, width = struct.unpack(">HH", data[i + 5:i + 9])
            return [width, height]
        i += 2 + length
    raise ValueError(f"{path}: no JPEG frame header")


def image_size(path):
    """(width, height) of a PNG or JPEG, by its extension, from its header."""
    return jpeg_size(path) if path.lower().endswith((".jpg", ".jpeg")) else png_size(path)


# ----------------------------------------------------------------------------
# Checks
# ----------------------------------------------------------------------------
def check(root, records, manifest):
    """Everything --check verifies, with no rendering. Returns a list of error strings."""
    errors = []
    sprites = manifest.get("sprites", {})
    expected = {}
    for rec, st, path in outputs(records):
        expected[path] = (rec, st)
        where = f"{path} ({rec['realm']}/{rec['id']}{'/' + st['state'] if st['state'] else ''})"
        full = os.path.join(root, path)
        if not os.path.exists(full):
            errors.append(f"{where}: missing; run make_ui_art.py")
            continue
        try:
            size = image_size(full)
        except ValueError as e:
            errors.append(str(e))
            continue
        if size != rec["png_size"]:
            errors.append(f"{where}: is {size[0]}x{size[1]} px, the catalog says {rec['png_size'][0]}x{rec['png_size'][1]}")
        m = sprites.get(path)
        if m is None:
            errors.append(f"{where}: not in manifest.json; it wasn't made by make_ui_art.py")
            continue
        if m.get("png_hash") != file_hash(full):
            errors.append(f"{where}: changed since make_ui_art.py wrote it; render it again, or take it out of "
                          "art.json and manifest.json to keep it as hand-made art")
        if m.get("png_size") != size:
            errors.append(f"{where}: manifest.json records {m.get('png_size')}, the file is {size}")
        if m.get("catalog_hash") != catalog_hash(rec, st):
            errors.append(f"{where}: its art.json entry changed since make_ui_art.py rendered it; run make_ui_art.py")
        if m.get("spill"):
            sides = ", ".join(f"{n} {v:g}" for n, v in zip(("left", "top", "right", "bottom"), m["spill"]) if v > 0)
            errors.append(f"{where}: the drawing reaches past the sprite ({sides} pt); raise its bleed and render it again")
    for path in sprites:
        if path not in expected:
            errors.append(f"{path}: in manifest.json but not in the catalog; run make_ui_art.py --prune")
    errors += check_web_manifest(root, expected)
    errors += check_skins(root, records)
    return errors


def check_web_manifest(root, paths):
    """Every sprite is in the web build's preload list."""
    wm = os.path.join(root, WEB_MANIFEST)
    if not os.path.exists(wm):
        return []
    with open(wm, encoding="utf-8") as f:
        listed = set(json.load(f))
    missing = [p for p in paths if os.path.exists(os.path.join(root, p)) and p not in listed]
    if not missing:
        return []
    shown = ", ".join(missing[:3]) + (f" and {len(missing) - 3} more" if len(missing) > 3 else "")
    return [f"not in {WEB_MANIFEST}: {shown}; run `cargo run -p web-builder -- manifest`"]


# A skin's 9-slice `center` that suits each catalog `center`: art drawn to repeat (tile) may
# repeat cut (tile) or resized to whole tiles (round); stretched art only stretches.
SKIN_CENTERS = {"tile": ("tile", "round"), "stretch": ("stretch",)}

# Skin blocks that hold no sprite paths: the style (fonts are under assets/fonts/), colours, and
# sounds (under assets/sounds/sfx/ui/).
SKIN_NON_SPRITE_BLOCKS = ("style", "text", "sounds")

# The skin every other realm's file is merged over.
SKIN_BASE_FILE = "dailong.json"


def check_skins(root, records):
    """Compares the skin files (assets/configs/ui/skins/*.json), when they exist, with the
    catalog: every sprite path a skin names exists under assets/ui/ (a path without an image
    extension is a folder, such as an icon set's, or a stem that takes a state suffix), and a
    9-slice of a catalog sprite has the catalog's slice and a center that suits it.

    A realm's file is checked as the game reads it, merged over dailong.json (objects key by key,
    anything else replaced), so a slice or center it doesn't repeat comes from Đại Long's.

    A skin object that holds "sprite" is a panel, button or separator: with "slice" a 9-slice,
    without one a stretched sprite, which a catalog 9-slice may not be. A framed button's
    "selected" frame is a 9-slice cut by its "selected_slice", else its "slice". Other strings
    that hold a "/" are sprite paths, outside the style, text and sounds blocks."""
    sdir = os.path.join(root, SKIN_CONFIG_DIR)
    if not os.path.isdir(sdir):
        return []
    by_path, by_stem = {}, {}
    for rec in records:
        for st in rec["states"]:
            by_path[st["out"]] = rec
            if st["state"]:
                by_stem[st["out"][: -len(f"_{st['state']}.png")]] = rec
    errors = []
    files = {}
    for name in sorted(os.listdir(sdir)):
        if not name.endswith(".json"):
            continue
        path = os.path.join(sdir, name)
        try:
            with open(path, encoding="utf-8") as f:
                skin = json.load(f)
        except (OSError, ValueError) as e:
            errors.append(f"{SKIN_CONFIG_DIR}/{name}: {e}")
            continue
        if not isinstance(skin, dict):
            errors.append(f"{SKIN_CONFIG_DIR}/{name}: not a JSON object")
            continue
        files[name] = skin
    base = files.get(SKIN_BASE_FILE, {})
    for name, skin in files.items():
        if name != SKIN_BASE_FILE:
            skin = merge_json(copy.deepcopy(base), skin)
        blocks = {k: v for k, v in skin.items() if k not in SKIN_NON_SPRITE_BLOCKS}
        for where, node in _walk(blocks, name):
            if isinstance(node, str) and "/" in node and not node.startswith("#"):
                if not _sprite_exists(root, node):
                    errors.append(f"{SKIN_CONFIG_DIR}/{where}: {node} doesn't exist under {UI_DIR}/")
            elif isinstance(node, dict) and isinstance(node.get("sprite"), str):
                errors += _check_slice(where, node["sprite"], node.get("slice"), node.get("center", "stretch"),
                                       by_path.get(node["sprite"]) or by_stem.get(node["sprite"]), stretched_ok=True)
                # A framed role's `selected` frame (the icon button's): a 9-slice the game cuts
                # by its `selected_slice`, else by the role's `slice`, with the role's center.
                if isinstance(node.get("selected"), str):
                    slice_ = node.get("selected_slice", node.get("slice"))
                    errors += _check_slice(f"{where}:selected", node["selected"], slice_, node.get("center", "stretch"),
                                           by_path.get(node["selected"]), stretched_ok=False)
    return errors


def _check_slice(where, sprite, slice_, center, rec, stretched_ok):
    """The errors of a skin's 9-slice `slice_` (None: a stretched sprite, which only
    `stretched_ok` allows) and `center` of `sprite`, against its catalog record `rec`, if any."""
    if rec is None:
        return []
    if slice_ is None:
        if rec["slice"] is not None or not stretched_ok:
            return [f"{SKIN_CONFIG_DIR}/{where}: {sprite} is a 9-slice in the catalog "
                    f"({rec['realm']}/{rec['id']}, slice {rec['slice']}); give its slice"]
        return []
    if rec["slice"] is None:
        return [f"{SKIN_CONFIG_DIR}/{where}: {sprite} isn't a 9-slice in the catalog"]
    errors = []
    if slice_ != rec["slice"]:
        errors.append(f"{SKIN_CONFIG_DIR}/{where}: slice {slice_}, the catalog's "
                      f"{rec['realm']}/{rec['id']} has {rec['slice']}")
    if center not in SKIN_CENTERS.get(rec["center"], (rec["center"],)):
        errors.append(f"{SKIN_CONFIG_DIR}/{where}: center {center!r} doesn't suit the catalog's "
                      f"{rec['realm']}/{rec['id']}, drawn to {rec['center']!r}")
    return errors


def merge_json(base, overlay):
    """The game's engine::config::merge_json: objects merge key by key, recursively, and any other
    value replaces the base's."""
    if isinstance(base, dict) and isinstance(overlay, dict):
        for k, v in overlay.items():
            base[k] = merge_json(base[k], v) if k in base else v
        return base
    return overlay


def _walk(node, where):
    yield where, node
    if isinstance(node, dict):
        for k, v in node.items():
            yield from _walk(v, f"{where}:{k}")
    elif isinstance(node, list):
        for i, v in enumerate(node):
            yield from _walk(v, f"{where}:{i}")


def _sprite_exists(root, rel):
    # An image file, a folder (e.g. an icon set's), or a stem that takes a state suffix.
    base = os.path.join(root, UI_DIR, rel)
    if rel.endswith((".png", ".jpg", ".jpeg")):
        return os.path.isfile(base)
    if os.path.isdir(base):
        return True
    return any(os.path.isfile(f"{base}_{s}.png") for s in KNOWN_STATES)
