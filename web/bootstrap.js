// Heritage Builder — WASM Bootstrap
//
// Loads the WASM module and starts the game. The Rust code handles
// asset loading via the asset manifest after wgpu initialization.

import init, { heritageExec } from "./pkg/HeritageBuilder.js";

// Automation bridge: `await window.heritage.exec("session.state")` runs a game command and
// resolves with its result, or rejects with { code, message }. `help` lists the commands.
// Playwright drives the web build through it (see crates/tools/web-smoke/).
window.heritage = {
    exec: (method, params = {}) => heritageExec(method, params),
};

const loadingBar    = document.getElementById("loading-bar");
const loadingStatus = document.getElementById("loading-status");
const loadingScreen = document.getElementById("loading-screen");
const errorScreen   = document.getElementById("error-screen");
const errorDetails  = document.getElementById("error-details");

function setProgress(percent, message) {
    loadingBar.style.width = `${percent}%`;
    loadingStatus.textContent = message;
}

function showError(message) {
    errorDetails.textContent = message;
    errorScreen.classList.add("visible");
    loadingScreen.classList.add("hidden");
}

async function start() {
    try {
        setProgress(10, "Loading WASM module...");

        // Initialize the WASM module. This calls the Rust main() function,
        // which sets up the winit event loop via spawn_app().
        await init({ module_or_path: "./pkg/HeritageBuilder_bg.wasm" });

        // Loading screen progress is now updated from Rust during initialization,
        // and hidden once the game is fully initialized (in wasm_runner finish_init).
    } catch (err) {
        console.error("Failed to start Heritage Builder:", err);
        showError(err.toString());
    }
}

// Handle browser resize — the canvas auto-scales via CSS (100%/100%),
// but winit picks up resize events via its own ResizeObserver.

// Prevent default browser behaviors that interfere with the game.
document.addEventListener("contextmenu", (e) => e.preventDefault());

start();
