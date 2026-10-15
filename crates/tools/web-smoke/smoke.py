#!/usr/bin/env python3
"""Web smoke test for Heritage Builder.

Serves web/, opens the game in headless Chromium with Playwright, and drives it through the
automation bridge (window.heritage.exec): ping, version, load a preset map, check the state, take
a screenshot through the game (base64 PNG) and one through Playwright to compare.

Build the web target first, then from the repository root:
    pip install playwright && playwright install chromium
    python3 crates/tools/web-smoke/smoke.py [--out out/web] [--timeout 180] [--headed] [--swiftshader]

Prints one JSON line per step and exits with status 0 if every step passed, 1 if not.
"""

import argparse
import base64
import functools
import http.server
import json
import struct
import sys
import threading
import time
from pathlib import Path

from playwright.sync_api import sync_playwright

REPO_ROOT = Path(__file__).resolve().parents[3]
WEB_DIR = REPO_ROOT / "web"


def serve(directory):
    """Serves `directory` on a free localhost port, in a background thread."""

    class Handler(http.server.SimpleHTTPRequestHandler):
        extensions_map = {
            **http.server.SimpleHTTPRequestHandler.extensions_map,
            ".wasm": "application/wasm",
            ".js": "text/javascript",
            ".mjs": "text/javascript",
        }

        def log_message(self, *args):
            pass  # Quiet: the steps report what matters.

    handler = functools.partial(Handler, directory=str(directory))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def png_size(data):
    """Width and height from a PNG's header, or None if it isn't a PNG."""
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        return None
    return struct.unpack(">II", data[16:24])


class Smoke:
    def __init__(self, page, out_dir):
        self.page = page
        self.out_dir = out_dir
        self.failed = 0

    def report(self, step, ok, **details):
        if not ok:
            self.failed += 1
        print(json.dumps({"step": step, "ok": ok, **details}), flush=True)

    def call(self, method, params=None, timeout_secs=60):
        """Runs a game command through the bridge. Returns (result, error)."""
        outcome = self.page.evaluate(
            """async ([method, params, timeoutMs]) => {
                const timeout = new Promise((_, reject) =>
                    setTimeout(() => reject({ code: "Timeout", message: `No answer in ${timeoutMs} ms.` }), timeoutMs));
                try { return { result: await Promise.race([window.heritage.exec(method, params), timeout]) }; }
                catch (error) { return { error: { code: error.code, message: error.message ?? String(error) } }; }
            }""",
            [method, params or {}, int(timeout_secs * 1000)],
        )
        return outcome.get("result"), outcome.get("error")

    def wait_until_ready(self, timeout_secs):
        """Waits for the page to load the game: ping answers once the engine runs."""
        deadline = time.monotonic() + timeout_secs
        last_error = None
        while time.monotonic() < deadline:
            has_bridge = self.page.evaluate("() => typeof window.heritage?.exec === 'function'")
            if has_bridge:
                result, error = self.call("ping", timeout_secs=10)
                if result and result.get("pong"):
                    return result
                last_error = error
            time.sleep(0.5)
        raise TimeoutError(f"The game didn't answer within {timeout_secs}s (last error: {last_error}).")

    def check(self, step, method, params=None, expect=None):
        """Runs a command and checks its result with `expect(result) -> bool`."""
        result, error = self.call(method, params)
        ok = error is None and (expect is None or bool(expect(result)))
        self.report(step, ok, command=method, result=result, error=error)
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--out", default="out/web", help="Folder for the screenshots. Default out/web.")
    parser.add_argument("--timeout", type=float, default=180.0, help="Seconds to wait for the game to load.")
    parser.add_argument("--headed", action="store_true", help="Show the browser window.")
    parser.add_argument("--swiftshader", action="store_true", help="Render on the CPU (SwiftShader), e.g. without a GPU.")
    args = parser.parse_args()

    if not (WEB_DIR / "pkg" / "HeritageBuilder_bg.wasm").is_file():
        print("No web build in web/pkg. Build it first: cargo run -p web-builder", file=sys.stderr)
        return 2

    out_dir = (REPO_ROOT / args.out) if not Path(args.out).is_absolute() else Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    server = serve(WEB_DIR)
    url = f"http://127.0.0.1:{server.server_address[1]}/index.html"

    console = []
    with sync_playwright() as playwright:
        # The web build renders with WebGPU, which headless Chromium only offers behind a flag.
        # Full Chromium ("chromium" channel, new headless mode), not Playwright's headless shell:
        # the shell's WebGPU rejects buffer maps, so the game's screenshots fail there.
        browser_args = ["--enable-unsafe-webgpu"]
        if args.swiftshader:
            browser_args.append("--use-webgpu-adapter=swiftshader")
            # Linux Chrome runs WebGPU on Vulkan, which headless Chrome leaves off. Without it the
            # canvas stays blank and buffer maps fail. SwiftShader provides the Vulkan device.
            if sys.platform.startswith("linux"):
                browser_args += ["--enable-features=Vulkan", "--use-vulkan=swiftshader", "--use-angle=swiftshader"]
        browser = playwright.chromium.launch(channel="chromium", headless=not args.headed, args=browser_args)
        page = browser.new_page(viewport={"width": 1280, "height": 720}, accept_downloads=True)
        page.on("console", lambda message: console.append(f"[{message.type}] {message.text}"))
        page.on("pageerror", lambda error: console.append(f"[pageerror] {error}"))

        smoke = Smoke(page, out_dir)
        try:
            page.goto(url)
            started = time.monotonic()
            ping = smoke.wait_until_ready(args.timeout)
            smoke.report("load", True, url=url, seconds=round(time.monotonic() - started, 1), frame=ping.get("frame"))

            smoke.check("version", "version", expect=lambda result: result.get("render_api") == "Wgpu")
            smoke.check("load preset", "session.load_preset", {"preset": 2}, expect=lambda result: result.get("map_size") == [9, 9])
            smoke.check("in game", "session.state", expect=lambda result: result.get("menus_mode") == "InGame")
            smoke.check(
                "buildings",
                "world.list",
                {"type": "building"},
                expect=lambda result: result.get("total", 0) > 0,
            )
            smoke.check(
                "place",
                "world.place",
                {"tile": "small_well", "cell": [3, 3]},
                expect=lambda result: result.get("tile") == "small_well",
            )

            shot, error = smoke.call("screenshot", {"inline": True})
            size = None
            if shot and shot.get("png_base64"):
                png = base64.b64decode(shot["png_base64"])
                size = png_size(png)
                (out_dir / "smoke.png").write_bytes(png)
            ok = error is None and size is not None and size[0] > 0 and size[1] > 0
            smoke.report("screenshot", ok, size=size, game_size=(shot or {}).get("size"), error=error, file=str(out_dir / "smoke.png"))

            page.screenshot(path=str(out_dir / "smoke-page.png"))
            smoke.report("page screenshot", True, file=str(out_dir / "smoke-page.png"))

            # The screenshot hotkey downloads the PNG on web.
            page.mouse.click(5, 300)  # Focus the canvas, away from the HUD buttons.
            with page.expect_download(timeout=30_000) as download_info:
                page.keyboard.press("F12")
            download = download_info.value
            hotkey_path = out_dir / "hotkey.png"
            download.save_as(str(hotkey_path))
            size = png_size(hotkey_path.read_bytes())
            smoke.report("hotkey download", size is not None, file_name=download.suggested_filename, size=size)
        except Exception as error:  # noqa: BLE001 - any failure fails the smoke test.
            smoke.report("error", False, error=str(error))
        finally:
            browser.close()
            server.shutdown()

    console_log = out_dir / "console.log"
    console_log.write_text("\n".join(console) + "\n")

    if smoke.failed:
        print(f"Browser console (last 40 lines; all of it in {console_log}):", file=sys.stderr)
        for line in console[-40:]:
            print("  " + line, file=sys.stderr)

    print(json.dumps({"ok": smoke.failed == 0, "failed": smoke.failed}), flush=True)
    return 0 if smoke.failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
