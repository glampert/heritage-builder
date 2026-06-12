// Engine-side `DrawDebugUi` implementations for engine subsystems and shared
// `common` types. Kept here (rather than scattered across the subsystem modules)
// so all hand-written engine debug-panel drawing lives in one place.
//
// NOTE: The orphan rule requires these impls to live in the engine crate, since
// both the `DrawDebugUi` trait (`engine::ui`) and the implemented types are owned
// by `engine` (or by `common`, which `engine` depends on).

mod sound;
mod texture;
mod ui_text;
mod update_timer;
