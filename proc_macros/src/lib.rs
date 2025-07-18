mod debug_ui;

// Procedural macro that implements the `#[derive(DrawDebugUi)]` annotation.
// Will try to generate a draw_debug_ui() function referencing all members of a structure.
#[proc_macro_derive(DrawDebugUi, attributes(debug_ui))]
pub fn draw_debug_ui_proc_macro(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    debug_ui::draw_debug_ui_proc_macro_impl(input)
}
