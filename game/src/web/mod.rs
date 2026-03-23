// WASM support modules.
//
// When targeting wasm32 (feature = "web"), imgui-sys is cross-compiled from
// C++ to WASM via clang. The resulting WASM binary imports C standard library
// functions (malloc, free, vsnprintf, sscanf, qsort, etc.) from an "env"
// module that the JS host must provide.
//
// These imports are satisfied by a two-part system:
//
//  1. `web/libc.js` — JS shims for string/memory functions (memchr, strcmp,
//     strncpy, etc.), file I/O stubs, and thin wrappers that forward to the
//     Rust implementations below.
//
//  2. `web/libc.rs` (this crate) — Rust implementations of functions that
//     need direct access to WASM linear memory or the indirect function table:
//       - `c_malloc` / `c_free` — pair with Rust's global allocator and track
//         allocation sizes (C's free doesn't pass a size).
//       - `c_qsort` — needs to invoke the C comparator via function pointer,
//         which requires the WASM indirect call table (not accessible from JS).
//       - `c_vsnprintf` — full printf implementation reading from wasm32
//         va_list (a pointer into the linear memory argument area).
//       - `c_sscanf` — parses one value from an input string; handles the
//         format specifiers imgui actually uses (%d, %u, %x, %f, %lf).
//
// All `#[no_mangle] pub extern "C"` functions here are automatically exported
// from the WASM binary and called by libc.js via `wasm.<fn_name>(...)`.

mod libc;
