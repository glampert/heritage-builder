"""Live preview of the mockups in a browser.

    python3 serve.py [--port 8765] [--host 127.0.0.1] [--open]

Builds every screen, serves them at http://127.0.0.1:8765/ and watches the
generator's files, assets/fonts and the icon paths in
crates/tools/ui-icons/icons.json. On a save it re-runs only the scripts
that the change affects, and the open page reloads itself. A failed build
shows its error over the page and keeps the last good screens.

Standard library only (the scripts it runs need Pillow, like before).
"""
import argparse
import html
import json
import os
import pathlib
import subprocess
import sys
import threading
import time
import webbrowser
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import unquote, urlparse

HERE = pathlib.Path(__file__).resolve().parent
BUILD = HERE / "build"
FONTS_DIR = (HERE / "../../../../assets/fonts").resolve()
ICONS = (HERE / "../../../../crates/tools/ui-icons/icons.json").resolve()  # The icon paths minimap.py reads.

REALMS = [("dailong", "Đại Long"), ("kambara", "Kambara"), ("tampadesa", "Tampadesa")]
SCREENS = [
    ("world-map", "World map"), ("intro", "Intro"), ("intermission", "Intermission"), ("hud", "In-game HUD"),
    ("menu-home", "Home"), ("menu-load-save", "Load / save"), ("menu-settings", "Settings"),
    ("menu-custom-game", "Custom game"), ("menu-about", "About"),
    ("campaign-picker", "Campaign picker"), ("game-menu", "Game menu"), ("game-quit", "Quit popup"),
    ("briefing", "Briefing"), ("campaign-complete", "Campaign complete"), ("hud-menus", "HUD menus and build menu"),
    ("inspector", "Building inspector"), ("event", "Event popup"),
    ("city-overview", "City book: overview"), ("city-population", "City book: population"), ("city-goods", "City book: goods"),
    ("city-finances", "City book: finances"), ("city-ratings", "City book: ratings"), ("city-labour", "City book: labour"),
    ("city-food", "City book: food"),
    ("city-heritage", "City book: heritage"),
]
ORDER = [f"{s}-{r}" for s, _ in SCREENS for r, _ in REALMS] + ["minimap-concepts", "minimap-lenses"]
SHEETS = [("minimap-concepts", "Minimap concepts"), ("minimap-lenses", "Minimap widget and lenses")]

# Which build steps a changed file needs. The scripts import each other
# (screens imports world_map, menus imports both), so a change to a shared
# file rebuilds everything that depends on it.
STEPS = ["fonts", "backdrops", "world_map", "screens", "menus", "minimap", "pages", "dialogs", "palette"]
AFFECTS = {
    "fonts.py": ["fonts", "world_map", "screens", "menus", "minimap", "pages", "dialogs", "palette"],
    "backdrops.py": ["backdrops", "screens", "menus", "pages", "dialogs", "palette"],
    "common.py": ["world_map", "screens", "menus", "minimap", "pages", "dialogs", "palette"],
    "world_map.py": ["world_map", "screens", "menus", "minimap", "pages", "dialogs", "palette"],
    "content.py": ["world_map"],
    "screens.py": ["screens", "menus", "pages", "dialogs", "palette"],
    "screens_content.py": ["screens", "dialogs", "palette"],
    "menus.py": ["menus", "pages", "dialogs", "palette"],
    "menus_content.py": ["menus", "pages", "dialogs", "palette"],
    "minimap.py": ["minimap", "screens", "menus", "pages", "dialogs", "palette"],
    "pages.py": ["pages", "dialogs", "palette"],
    "pages_content.py": ["pages", "dialogs", "palette"],
    "dialogs.py": ["dialogs", "palette"],
    "dialogs_content.py": ["dialogs", "palette"],
    "palette.py": ["palette"],
    "palette_glyphs.py": ["palette"],
    "kambara_carving.py": ["palette"],
}


class State:
    def __init__(self):
        self.lock = threading.Lock()
        self.version = 0
        self.building = False
        self.error = ""
        self.last = ""
        self.pending = set()   # steps a failed build left undone

    def snapshot(self):
        with self.lock:
            return dict(version=self.version, building=self.building, error=self.error, last=self.last)


STATE = State()


def run_steps(steps):
    with STATE.lock:
        steps = [s for s in STEPS if s in steps or s in STATE.pending]
        STATE.building = True
    t0 = time.time()
    error = ""
    undone = set()
    for i, step in enumerate(steps):
        proc = subprocess.run([sys.executable, f"{step}.py"], cwd=HERE, capture_output=True, text=True)
        if proc.returncode:
            error = f"{step}.py failed:\n\n{proc.stderr.strip() or proc.stdout.strip()}"
            undone = set(steps[i:])
            break
    with STATE.lock:
        STATE.building = False
        STATE.error = error
        STATE.pending = undone
        STATE.last = f"{', '.join(steps)} in {time.time() - t0:.1f} s" + (" (failed)" if error else "")
        if not error:
            STATE.version += 1
    print(("ERROR " + error.splitlines()[0]) if error else f"built {STATE.last}", flush=True)


def initial_steps():
    # Every screen script, every time; fonts and backdrops only when their output is missing.
    steps = set(STEPS) - {"fonts", "backdrops"}
    if not (BUILD / "fonts.css").exists():
        steps.add("fonts")
    if not all((BUILD / f"city_{r}.jpg").exists() for r, _ in REALMS):
        steps.add("backdrops")
    return steps


def watched_files():
    files = {p: AFFECTS[p.name] for p in HERE.glob("*.py") if p.name in AFFECTS}
    for p in FONTS_DIR.glob("*.ttf"):
        files[p] = AFFECTS["fonts.py"]
    files[ICONS] = AFFECTS["minimap.py"]
    return files


def watch(interval=0.4):
    mtimes = {p: p.stat().st_mtime for p in watched_files()}
    while True:
        time.sleep(interval)
        changed = set()
        for p, affects in watched_files().items():
            try:
                m = p.stat().st_mtime
            except FileNotFoundError:
                continue
            if mtimes.get(p) != m:
                mtimes[p] = m
                changed.update(affects)
        if changed:
            time.sleep(0.15)   # let an editor finish writing
            run_steps(changed)


VIEW_PAGE = """<!doctype html><html><head><meta charset="utf-8"><title>{title} · mockups</title>
<style>
:root {{ color-scheme: dark; }}
html, body {{ margin: 0; height: 100%; background: #16130f; color: #eadfc8; font: 14px/1.4 system-ui, sans-serif; overflow: hidden; }}
#bar {{ position: fixed; inset: 0 0 auto 0; height: 40px; display: flex; align-items: center; gap: 14px; padding: 0 14px; background: #241e17; border-bottom: 1px solid #3a3025; z-index: 2; }}
#bar a {{ color: #eadfc8; text-decoration: none; padding: 4px 9px; border-radius: 5px; }}
#bar a:hover {{ background: #3a3025; }}
#bar a.on {{ background: #8a3b20; }}
#bar .sep {{ flex: 1; }}
#status {{ color: #a99a80; font-size: 12px; }}
#stage {{ position: fixed; inset: 40px 0 0 0; display: flex; align-items: center; justify-content: center; }}
#frame {{ width: 1920px; height: 1080px; border: 0; transform-origin: center; flex: none; background: #000; }}
#err {{ position: fixed; inset: 60px 40px auto 40px; max-height: 70%; overflow: auto; background: #3a0d0a; color: #ffd9d0; border: 1px solid #b23b2a; border-radius: 6px; padding: 14px 18px; white-space: pre-wrap; font: 13px/1.45 ui-monospace, monospace; z-index: 3; display: none; }}
</style></head><body>
<div id="bar"><a href="/">All screens</a>{realm_links}<span class="sep"></span>
<a href="/view/{prev}" title="Previous (←)">←</a><b>{title}</b><a href="/view/{next}" title="Next (→)">→</a>
<span class="sep"></span><span id="status">connecting…</span><a href="/build/{name}.html" target="_blank" title="Open the page alone">↗</a></div>
<div id="stage"><iframe id="frame" src="/build/{name}.html"></iframe></div>
<pre id="err"></pre>
<script>
const frame = document.getElementById('frame'), stage = document.getElementById('stage');
function fit() {{ const s = Math.min(stage.clientWidth / 1920, stage.clientHeight / 1080); frame.style.transform = `scale(${{s}})`; }}
addEventListener('resize', fit); fit();
addEventListener('keydown', e => {{
  if (e.key === 'ArrowLeft') location.href = '/view/{prev}';
  if (e.key === 'ArrowRight') location.href = '/view/{next}';
}});
let seen = null;
async function poll() {{
  try {{
    const s = await (await fetch('/__state', {{cache: 'no-store'}})).json();
    if (seen !== null && s.version !== seen) frame.src = '/build/{name}.html?v=' + s.version;
    seen = s.version;
    const err = document.getElementById('err');
    err.style.display = s.error ? 'block' : 'none'; err.textContent = s.error;
    document.getElementById('status').textContent = s.building ? 'building…' : (s.last ? 'built ' + s.last : '');
  }} catch (e) {{ document.getElementById('status').textContent = 'server stopped'; }}
  setTimeout(poll, 500);
}}
poll();
</script></body></html>"""

INDEX_PAGE = """<!doctype html><html><head><meta charset="utf-8"><title>Mockups · live preview</title>
<style>
:root {{ color-scheme: dark; }}
body {{ margin: 0; padding: 28px 36px; background: #16130f; color: #eadfc8; font: 15px/1.5 system-ui, sans-serif; }}
h1 {{ font-size: 22px; margin: 0 0 4px; }} p {{ color: #a99a80; margin: 0 0 22px; }}
table {{ border-collapse: collapse; }} th, td {{ padding: 7px 18px 7px 0; text-align: left; }}
th {{ color: #a99a80; font-weight: 600; }} a {{ color: #f0c27a; }}
</style></head><body><h1>Mockups · live preview</h1>
<p>Edit a file in the generator folder and the open screen rebuilds and reloads. ← and → step through the screens.</p>
<table><tr><th>Screen</th>{realm_heads}</tr>{rows}</table></body></html>"""


def view_page(name):
    i = ORDER.index(name)
    if name in dict(SHEETS):
        return VIEW_PAGE.format(title=html.escape(dict(SHEETS)[name]), name=name, realm_links="",
                                prev=ORDER[i - 1], next=ORDER[(i + 1) % len(ORDER)])
    screen, realm = name.rsplit("-", 1)
    title = f"{dict(SCREENS)[screen]} · {dict(REALMS)[realm]}"
    links = "".join(f'<a class="{"on" if r == realm else ""}" href="/view/{screen}-{r}">{html.escape(label)}</a>' for r, label in REALMS)
    return VIEW_PAGE.format(title=html.escape(title), name=name, realm_links=links,
                            prev=ORDER[i - 1], next=ORDER[(i + 1) % len(ORDER)])


def index_page():
    heads = "".join(f"<th>{html.escape(label)}</th>" for _, label in REALMS)
    rows = "".join(f"<tr><td>{html.escape(label)}</td>" + "".join(f'<td><a href="/view/{s}-{r}">open</a></td>' for r, _ in REALMS) + "</tr>"
                   for s, label in SCREENS)
    rows += "".join(f'<tr><td>{html.escape(label)}</td><td colspan="3"><a href="/view/{s}">open</a></td></tr>' for s, label in SHEETS)
    return INDEX_PAGE.format(realm_heads=heads, rows=rows)


class Handler(SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def send_text(self, body, ctype="text/html; charset=utf-8", status=200):
        data = body.encode()
        self.send_response(status)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        path = unquote(urlparse(self.path).path)
        if path == "/":
            return self.send_text(index_page())
        if path == "/__state":
            return self.send_text(json.dumps(STATE.snapshot()), "application/json")
        if path.startswith("/view/") and path[6:] in ORDER:
            return self.send_text(view_page(path[6:]))
        if path.startswith("/build/"):
            target = (BUILD / path[7:]).resolve()
            if target.parent == BUILD.resolve() and target.suffix in (".html", ".jpg") and target.is_file():
                data = target.read_bytes()
                self.send_response(200)
                self.send_header("Content-Type", "text/html; charset=utf-8" if target.suffix == ".html" else "image/jpeg")
                self.send_header("Content-Length", str(len(data)))
                self.send_header("Cache-Control", "no-store")
                self.end_headers()
                self.wfile.write(data)
                return
        self.send_text("Not found", "text/plain; charset=utf-8", 404)


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--port", type=int, default=8765)
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--open", action="store_true", help="open the index in the default browser")
    args = ap.parse_args()
    os.chdir(HERE)
    print("building…", flush=True)
    run_steps(initial_steps())
    threading.Thread(target=watch, daemon=True).start()
    server = ThreadingHTTPServer((args.host, args.port), Handler)
    url = f"http://{args.host}:{server.server_address[1]}/"
    print(f"serving {url}  (Ctrl+C to stop)", flush=True)
    if args.open:
        webbrowser.open(url)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
