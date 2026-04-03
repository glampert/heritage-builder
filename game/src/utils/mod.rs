#![allow(unused_imports)]

// Re-export shim: all utils types now live in the `common` crate.
// This module re-exports everything so existing `crate::utils::*` paths continue to work.

// Sub-modules — re-export from common.
pub mod callback {
    pub use common::callback::*;
}

pub mod constants {
    pub use common::constants::*;
}

pub mod coords {
    pub use common::coords::*;
}

pub mod fixed_string {
    pub use common::fixed_string::*;
    // Re-export #[macro_export] macros.
    pub use common::{
        format_fixed_string,
        format_fixed_string_trunc,
        write_fixed_string,
        write_fixed_string_trunc,
        append_fixed_string,
        append_fixed_string_trunc,
    };
}

pub mod hash {
    pub use common::hash::*;
}

pub mod mem {
    pub use common::mem::*;
    // Re-export #[macro_export] macros.
    pub use common::{singleton, singleton_late_init};
}

pub mod time {
    pub use common::time::*;
}

// Root-level type re-exports from common.
pub use common::{
    Vec2, Color, Size, Rect, RectCorners, RectEdges, RectTexCoords,
    FieldAccessorXY,
    map_value_to_range, normalize_value, lerp, approx_equal,
};

// Root-level macro re-exports from common.
pub use common::{name_of, swap2, bitflags_with_display, field_accessor_xy};

// This must stay here — env!("CARGO_PKG_VERSION") resolves to the game crate's version.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
