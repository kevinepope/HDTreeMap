//! Treemap rendering (`docs/PLAN.md` §3.4): colours and the per-pixel cushion rasteriser.
//!
//! - [`palette`] — a hue per top extension (or per top-level folder), greys for the rest.
//! - [`raster`] — paints a `layout::TreemapLayout` into an RGBA buffer, cushioned, flat or
//!   bevelled,
//!   in parallel row bands natively.
//!
//! The `HDTREEMAP_STATS` environment variable ([`stats_enabled`]) makes the treemap view
//! print how long each layout and raster pass took ([`stats_line`]; the web demo turns it
//! on with `?stats` and logs to the console).

pub mod palette;
pub mod raster;

pub use palette::{ColorBy, Palette, Rgb, FOLDER_BLOCK, HUES, OTHER_FILES};
pub use raster::{
    cushion_intensity, rasterise, Shading, AMBIENT, BACKGROUND, BEVEL_WIDTH, BRIGHTNESS, LIGHT,
};

/// Where stats lines go when stats are on: stderr natively, or a sink a shell installed
/// ([`enable_stats`]; the web shell logs to the browser console).
static SINK: std::sync::OnceLock<fn(&str)> = std::sync::OnceLock::new();
static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Whether stats are on: `HDTREEMAP_STATS` is set, or a shell called [`enable_stats`]
/// first (there is no environment on wasm).
pub fn stats_enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var_os("HDTREEMAP_STATS").is_some())
}

/// Turn stats on with `sink` receiving each line (before anything has asked
/// [`stats_enabled`]; later calls change nothing).
pub fn enable_stats(sink: fn(&str)) {
    let _ = SINK.set(sink);
    let _ = ENABLED.set(true);
}

/// Write one stats line (`hdtreemap stats: ` + `line`).
pub fn stats_line(line: &str) {
    let line = format!("hdtreemap stats: {line}");
    match SINK.get() {
        Some(sink) => sink(&line),
        None => eprintln!("{line}"),
    }
}

#[cfg(test)]
mod tests;
