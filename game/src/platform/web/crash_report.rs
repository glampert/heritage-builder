
pub fn initialize(set_panic_hook: bool) {
    if !set_panic_hook {
        return;
    }

    // On WASM, install console_error_panic_hook for better panic messages.
    // TODO: Enable once console_error_panic_hook is wired up.
}
