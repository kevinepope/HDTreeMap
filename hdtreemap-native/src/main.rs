//! # Native shell for HDTreeMap
//!
//! Thinnest possible desktop shim: the winit window, event loop, wgpu surface, input
//! forwarding and frame painting all belong to `agg-gui-shell`. This crate names the window,
//! reads the command line ([`cli`]), and hands the shared app from `hdtreemap-core` the real
//! OS services ([`platform::NativePlatform`]) or, with `--demo`, the web demo's simulated
//! drives. `--dump-top N` scans without a window ([`dump`]).
//!
//! Usage: see [`cli::USAGE`]. `HDTREEMAP_SCREENSHOT=out.png` captures one frame after
//! start-up and exits (README images, checking the UI without a person at the keyboard);
//! `HDTREEMAP_SCREENSHOT_FRAMES=N` sets how many frames to let settle first (default 10).

mod cli;
mod dump;
mod platform;

use std::process::ExitCode;
use std::rc::Rc;

use agg_gui_shell::{NoHost, RedrawPolicy, ShellConfig, ShellError};
use hdtreemap_core::platform::{DemoPlatform, PlatformServices};
use hdtreemap_core::state::ScanTarget;
use hdtreemap_core::{build_app, install_theme_and_fonts, AppState, APP_TITLE};

use cli::Command;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match cli::parse(&args) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("hdtreemap: {message}\n\n{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };
    let (platform, target, highlight): (Rc<dyn PlatformServices>, Option<ScanTarget>, _) =
        match command {
            Command::Help => {
                println!("{}", cli::USAGE);
                return ExitCode::SUCCESS;
            }
            Command::DumpTop { top, source } => {
                return match dump::run(top, &source) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(message) => {
                        eprintln!("hdtreemap: {message}");
                        ExitCode::FAILURE
                    }
                };
            }
            Command::App { path, highlight } => (
                Rc::new(platform::NativePlatform),
                path.map(ScanTarget::folder),
                highlight,
            ),
            Command::Demo { id, highlight } => {
                let demo = DemoPlatform::default();
                let target = match id {
                    Some(id) => match demo.list_volumes().into_iter().find(|v| v.id == id) {
                        Some(volume) => Some(ScanTarget::volume(&volume)),
                        None => {
                            eprintln!("hdtreemap: unknown demo drive {id:?}\n\n{}", cli::USAGE);
                            return ExitCode::from(2);
                        }
                    },
                    None => None,
                };
                (Rc::new(demo), target, highlight)
            }
        };
    match run_app(platform, target, highlight) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("hdtreemap: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Open the window over `platform`, scanning `target` at once when given (and highlighting
/// extension `highlight` when it finishes).
fn run_app(
    platform: Rc<dyn PlatformServices>,
    target: Option<ScanTarget>,
    highlight: Option<String>,
) -> Result<(), ShellError> {
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
        let state = AppState::new();
        state.highlight_on_finish(highlight);
        if let Some(target) = target {
            let source = platform.scan_source(&target);
            state.start_scan(target, source);
        }
        let app = build_app(state, platform, fonts);
        Ok((app, NoHost))
    })
}
