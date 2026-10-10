//! # Native shell for HDTreeMap
//!
//! Thinnest possible desktop shim: the winit window, event loop, wgpu surface, input
//! forwarding and frame painting all belong to `agg-gui-shell`. This crate names the window,
//! reads the command line ([`cli`]), and hands the shared app from `hdtreemap-core` the real
//! OS services ([`platform::NativePlatform`]) or, with `--demo`, the web demo's simulated
//! drives. On macOS it installs the app's menus as the system menu bar ([`menu_bar`]), and
//! it closes the window when File ▸ Quit is chosen. `--dump-top N` scans without a window
//! ([`dump`]).
//!
//! Usage: see [`cli::USAGE`]. `HDTREEMAP_SCREENSHOT=out.png` captures one frame after
//! start-up and exits (README images, checking the UI without a person at the keyboard);
//! `HDTREEMAP_SCREENSHOT_FRAMES=N` sets how many frames to let settle first (default 10);
//! `HDTREEMAP_SCREENSHOT_SHOW` opens the context menu, the trash confirmation, a menu or
//! About first ([`show`]); `HDTREEMAP_IN_WINDOW_MENU=1` uses the in-window menu bar on
//! macOS.

mod cli;
mod dump;
mod menu_bar;
mod platform;
mod show;

use std::process::ExitCode;
use std::rc::Rc;

use agg_gui::App;
use agg_gui_shell::{RedrawPolicy, ShellConfig, ShellControl, ShellError, ShellHost};
use hdtreemap_core::platform::{DemoPlatform, PlatformServices};
use hdtreemap_core::state::ScanTarget;
use hdtreemap_core::ui::commands::Commands;
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
        // macOS: the system menu bar; elsewhere the window shows its own.
        // HDTREEMAP_IN_WINDOW_MENU=1 shows the in-window one on macOS too (screenshots).
        let in_window = std::env::var_os("HDTREEMAP_IN_WINDOW_MENU").is_some();
        let native_menu = !in_window && menu_bar::install(&Commands::new(&state, &platform));
        state.set_native_menu_bar(native_menu);
        state.highlight_on_finish(highlight);
        if let Some(target) = target {
            if !platform.is_demo() {
                // No start screen lists the volumes first: look them up off the UI thread.
                platform::list_volumes_in_background(&state);
            }
            let source = platform.scan_source(&target);
            state.start_scan(target, source);
        }
        let host = AppHost {
            state: state.clone(),
            script: show::Show::from_env().map(show::Script::new),
        };
        let app = build_app(state, platform, fonts);
        Ok((app, host))
    })
}

/// The shell side of the app: closes the window when File ▸ Quit was chosen, and replays
/// `HDTREEMAP_SCREENSHOT_SHOW`'s input ([`show`]).
struct AppHost {
    state: AppState,
    script: Option<show::Script>,
}

impl ShellHost for AppHost {
    fn on_frame(&mut self, app: &mut App, _frame: &agg_gui_shell::Frame) {
        if let Some(script) = &mut self.script {
            script.on_frame(app, &self.state);
        }
    }

    fn on_idle(&mut self, _app: &mut App, control: &mut ShellControl<'_>) {
        if self.state.quit_requested() {
            control.request_exit();
        }
    }
}
