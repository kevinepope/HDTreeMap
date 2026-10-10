//! Treemap rendering (`docs/PLAN.md` §3.4): colours and the per-pixel cushion rasteriser.
//!
//! - [`palette`] — a hue per top extension, greys for everything else.
//! - [`raster`] — paints a `layout::TreemapLayout` into an RGBA buffer, cushioned or flat,
//!   in parallel row bands natively.
//!
//! The `HDTREEMAP_STATS` environment variable ([`stats_enabled`]) makes the treemap view
//! print how long each layout and raster pass took.

pub mod palette;
pub mod raster;

pub use palette::{Palette, Rgb, FOLDER_BLOCK, HUES, OTHER_FILES};
pub use raster::{cushion_intensity, rasterise, Shading, AMBIENT, BACKGROUND, BRIGHTNESS, LIGHT};

/// Whether `HDTREEMAP_STATS` is set (always false where there is no environment, on wasm).
pub fn stats_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("HDTREEMAP_STATS").is_some())
}

#[cfg(test)]
mod tests;
