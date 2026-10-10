//! # Native shell for HDTreeMap
//!
//! Thinnest possible desktop shim: the winit window, event loop, wgpu surface, input
//! forwarding and frame painting all belong to `agg-gui-shell`. This crate names the window,
//! reads the optional folder argument from the command line, and hands the shared app from
//! `hdtreemap-core` the real OS services ([`platform::NativePlatform`]).
//!
//! Usage: `hdtreemap [PATH]`. With a path the app opens straight on that scan target.
//! `HDTREEMAP_SCREENSHOT=out.png` captures one frame after start-up and exits (README images,
//! checking the UI without a person at the keyboard); `HDTREEMAP_SCREENSHOT_FRAMES=N` sets how
//! many frames to let settle first (default 10).

mod platform;

use std::path::PathBuf;
use std::rc::Rc;

use agg_gui_shell::{NoHost, RedrawPolicy, ShellConfig, ShellError};
use hdtreemap_core::{build_app, install_theme_and_fonts, AppState, APP_TITLE};

fn main() -> Result<(), ShellError> {
    let initial_path = std::env::args_os().nth(1).map(PathBuf::from);

    let mut config = ShellConfig::new(APP_TITLE)
        .with_logical_size(1200.0, 800.0)
        .with_min_logical_size(640.0, 480.0)
        // Nothing animates on its own; repaint only when input or state asks.
        .with_redraw_policy(RedrawPolicy::Reactive)
        .with_device_label("hdtreemap-native");
    if let Ok(path) = std::env::var("HDTREEMAP_SCREENSHOT") {
        let frames = std::env::var("HDTREEMAP_SCREENSHOT_FRAMES")
            .ok()
            .and_then(|f| f.parse().ok())
            .unwrap_or(10);
        config = config.with_screenshot(path, frames);
    }

    agg_gui_shell::run(config, move |init| {
        let fonts = install_theme_and_fonts(init.device_scale()).map_err(ShellError::app)?;
        let state = AppState::with_initial_path(initial_path);
        let app = build_app(state, Rc::new(platform::NativePlatform), fonts);
        Ok((app, NoHost))
    })
}
