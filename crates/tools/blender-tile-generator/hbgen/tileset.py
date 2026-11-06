"""Merging catalog entries into assets/tiles/<layer>/tile_set.json. Standard
library only.

The tile sets are partly hand-formatted, so the file is never re-serialised:
only the text of the tile defs the generator owns is replaced, and new defs are
appended to their category. Rules:
- A generated def follows its catalog entry: name, the entry's tile_set fields
  in order, logical_size and draw_size from the manifest, then its variations.
- Variations are append-only. Saves store a variation by index, so existing
  ones keep their place, and one the catalog no longer lists is kept as it is.
- A def that exists but none of whose sprites the generator wrote is hand-made,
  and syncing it is an error.
"""

import json
import os

from . import catalog as cat
from . import pngio

INLINE_OBJECT_KEYS = {"materials"}          # written on one line, as the hand-made defs do
_RESERVED = {"name", "logical_size", "draw_size", "variations"}


class SyncError(Exception):
    pass


# ----------------------------------------------------------------------------
# JSON with text spans
# ----------------------------------------------------------------------------
def parse_spans(text):
    """Parse JSON text. Returns (value, spans): spans maps every value's path,
    a tuple of keys and indices, to its (start, end) offsets in text."""
    spans, dec, n = {}, json.JSONDecoder(), len(text)

    def ws(i):
        while i < n and text[i] in " \t\r\n":
            i += 1
        return i

    def expect(i, ch):
        if i >= n or text[i] != ch:
            raise SyncError(f"JSON: expected '{ch}' at offset {i}")

    def value(i, path):
        i = ws(i); start = i
        if i >= n:
            raise SyncError("JSON: unexpected end")
        if text[i] in "{[":
            close, is_obj = ("}", True) if text[i] == "{" else ("]", False)
            out = {} if is_obj else []
            i = ws(i + 1)
            if text[i] == close:
                i += 1
            else:
                while True:
                    if is_obj:
                        i = ws(i); expect(i, '"')
                        key, i = dec.raw_decode(text, i)
                        i = ws(i); expect(i, ":")
                        out[key], i = value(i + 1, path + (key,))
                    else:
                        v, i = value(i, path + (len(out),)); out.append(v)
                    i = ws(i)
                    if i < n and text[i] == ",":
                        i += 1; continue
                    expect(i, close); i += 1; break
        else:
            try:
                out, i = dec.raw_decode(text, i)
            except json.JSONDecodeError as e:
                raise SyncError(f"JSON: {e}")
        spans[path] = (start, i)
        return out, i

    doc, end = value(0, ())
    if text[ws(end):].strip():
        raise SyncError("JSON: trailing text")
    return doc, spans


def format_value(v, indent, key=None):
    """JSON text in the tile sets' style: two-space indent, objects in
    INLINE_OBJECT_KEYS on one line. indent is the column of the value's line."""
    pad = " " * indent
    if isinstance(v, dict):
        if not v:
            return "{}"
        if key in INLINE_OBJECT_KEYS and all(not isinstance(x, (dict, list)) for x in v.values()):
            return "{ " + ", ".join(f"{json.dumps(k)}: {json.dumps(x)}" for k, x in v.items()) + " }"
        items = [f"{pad}  {json.dumps(k)}: {format_value(x, indent + 2, k)}" for k, x in v.items()]
        return "{\n" + ",\n".join(items) + "\n" + pad + "}"
    if isinstance(v, list):
        if not v:
            return "[]"
        return "[\n" + ",\n".join(pad + "  " + format_value(x, indent + 2) for x in v) + "\n" + pad + "]"
    return json.dumps(v, ensure_ascii=False)


def _line_indent(text, pos):
    start = text.rfind("\n", 0, pos) + 1
    i = start
    while i < len(text) and text[i] == " ":
        i += 1
    return i - start


def _append_to_array(text, spans, path, length, item):
    s, e = spans[path]
    if length:
        ls, le = spans[path + (length - 1,)]
        ind = _line_indent(text, ls)
        return text[:le] + ",\n" + " " * ind + format_value(item, ind) + text[le:]
    ind = _line_indent(text, s)
    return text[:s] + "[\n" + " " * (ind + 2) + format_value(item, ind + 2) + "\n" + " " * ind + "]" + text[e:]


# ----------------------------------------------------------------------------
# Tile defs from the catalog
# ----------------------------------------------------------------------------
def _variation_obj(entry, v):
    obj = {}
    if v["name"]:
        obj["name"] = v["name"]
    obj.update(v["tile_set"])
    obj["anim_sets"] = [{"frames": [{"name": f["name"]} for f in entry["frames"]]}]
    return obj


def desired_def(entry, existing, sprites, notes):
    """The tile def a generated entry should have, given the existing def (or
    None) and the manifest's sprites. Appends warnings to notes."""
    bad = _RESERVED & set(entry["tile_set"])
    if bad:
        raise SyncError(f"{entry['tile']}: tile_set may not set {sorted(bad)}; they come from the render")
    recs = []
    for job in cat.jobs([dict(entry, generated=True)]):
        rec = sprites.get(job["path"])
        if rec is None:
            raise SyncError(f"{entry['tile']}: {job['path']} isn't rendered yet; run build.py")
        recs.append(rec)
    if any((r["logical_size"], r["draw_size"]) != (recs[0]["logical_size"], recs[0]["draw_size"]) for r in recs):
        raise SyncError(f"{entry['tile']}: its sprites rendered at different sizes; build.py renders a tile's sprites at one height")
    if existing is not None:
        ours = [ev for ev in existing.get("variations", []) if cat.sprite_path(entry, ev.get("name", "")) in sprites]
        if not ours:
            raise SyncError(f"{entry['tile']}: the tile def exists, but none of its sprites came from the generator, "
                            "so it's hand-made. Set \"generated\": false in the catalog, or remove the def first.")
    d = {"name": entry["tile"]}
    d.update(entry["tile_set"])
    for k in ("logical_size", "draw_size"):            # width first, as the engine's Size and the hand-made defs
        d[k] = {"width": recs[0][k]["width"], "height": recs[0][k]["height"]}
    wanted = {v["name"]: v for v in entry["variations"]}
    out = []
    for ev in (existing or {}).get("variations", []):
        name = ev.get("name", "")
        if name in wanted:
            out.append(_variation_obj(entry, wanted.pop(name)))
        else:
            out.append(ev)
            notes.append(f"{entry['tile']}: kept variation '{name}', which the catalog no longer lists (saves store variations by index)")
    out += [_variation_obj(entry, v) for v in entry["variations"] if v["name"] in wanted]
    d["variations"] = out
    return d


def sync_text(text, entries, sprites, notes):
    """The tile set's text with the entries merged in."""
    for entry in entries:
        doc, spans = parse_spans(text)
        cats = doc.get("categories", [])
        ci = next((i for i, c in enumerate(cats) if c.get("name") == entry["category"]), None)
        if ci is None:
            d = desired_def(entry, None, sprites, notes)
            text = _append_to_array(text, spans, ("categories",), len(cats), {"name": entry["category"], "tile_defs": [d]})
            notes.append(f"{entry['tile']}: added category '{entry['category']}'")
            continue
        defs = cats[ci].get("tile_defs", [])
        di = next((j for j, t in enumerate(defs) if t.get("name") == entry["tile"]), None)
        d = desired_def(entry, defs[di] if di is not None else None, sprites, notes)
        if di is None:
            text = _append_to_array(text, spans, ("categories", ci, "tile_defs"), len(defs), d)
        else:
            s, e = spans[("categories", ci, "tile_defs", di)]
            text = text[:s] + format_value(d, _line_indent(text, s)) + text[e:]
    json.loads(text)                                      # never write a broken file
    return text


# ----------------------------------------------------------------------------
# Integrity checks and the whole sync
# ----------------------------------------------------------------------------
def check_sprites(root, jobs, sprites, errors, web_manifest=True):
    """Each rendered sprite exists at its recorded size and is in the web build's asset list."""
    listed = None
    wm = os.path.join(root, "web", "asset_manifest.json")
    if web_manifest and os.path.exists(wm):
        with open(wm, encoding="utf-8") as f:
            listed = set(json.load(f))
    for job in jobs:
        rec = sprites.get(job["path"])
        if rec is None:
            errors.append(f"{job['path']}: not rendered; run build.py")
            continue
        full = os.path.join(root, job["path"])
        if not os.path.exists(full):
            errors.append(f"{job['path']}: in the manifest but missing on disk")
            continue
        size = list(pngio.read_size(full))
        if size != rec["png_size"]:
            errors.append(f"{job['path']}: is {size[0]}x{size[1]}, the manifest says {rec['png_size'][0]}x{rec['png_size'][1]}")
        dens = rec["density"]
        if size[0] % dens or size[1] % dens or [size[0] // dens, size[1] // dens] != [rec["draw_size"]["width"], rec["draw_size"]["height"]]:
            errors.append(f"{job['path']}: {size[0]}x{size[1]} px isn't draw_size x {dens}")
        if rec.get("fast"):
            errors.append(f"{job['path']}: rendered with --fast; render it again without it before committing")
        if listed is not None and job["path"] not in listed:
            errors.append(f"{job['path']}: not in web/asset_manifest.json; run `cargo run -p web-builder -- manifest`")


def sync(root, entries, manifest, write=True):
    """Merge every generated entry into its layer's tile set. Returns
    (changes, notes, errors); changes maps a tile set path to (old, new) text."""
    notes, errors, changes = [], [], {}
    sprites = manifest.get("sprites", {})
    gen = [e for e in entries if e["generated"]]
    check_sprites(root, cat.jobs(gen), sprites, errors)
    # Tiles with a sprite not rendered yet were reported above; merge the rest.
    gen = [e for e in gen if all(j["path"] in sprites for j in cat.jobs([e]))]
    for layer in sorted({e["layer"] for e in gen}):
        path = os.path.join(root, "assets", "tiles", layer, "tile_set.json")
        with open(path, encoding="utf-8") as f:
            old = f.read()
        try:
            new = sync_text(old, [e for e in gen if e["layer"] == layer], sprites, notes)
        except SyncError as e:
            errors.append(str(e)); continue
        if new != old:
            changes[path] = (old, new)
            if write:
                with open(path, "w", encoding="utf-8") as f:
                    f.write(new)
    return changes, notes, errors
