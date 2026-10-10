//! # hdtreemap-core
//!
//! The target-agnostic core of HDTreeMap, a disk-usage treemap viewer (see `docs/PLAN.md`).
//! Both shells build the same app from here:
//!
//! - `hdtreemap-native` calls [`build_app`] with its real `PlatformServices`;
//! - `hdtreemap-wasm` calls it with [`platform::DemoPlatform`].
//!
//! Modules:
//! - [`demo_fs`] — the simulated drives' file trees and the `DemoScanner` that streams them.
//! - [`fonts`] — theme + font installation and the Font Awesome code points.
//! - [`format`] — human-readable sizes, counts, durations.
//! - [`model`] — the scanned tree (arena of nodes), size modes, extension table.
//! - [`platform`] — the `PlatformServices` contract, volume-list cleanup, the demo platform.
//! - [`scan`] — `ScanSource`, live progress, and the native `FsScanner`.
//! - [`state`] — the shared app state (screen, running scan, finished result) and the scan pump.
//! - [`ui`] — the widget tree (start, scan and results screens).

pub mod demo_fs;
pub mod fonts;
pub mod format;
pub mod model;
pub mod platform;
pub mod scan;
pub mod state;
pub mod ui;

use std::rc::Rc;

use agg_gui::App;

pub use fonts::{install_theme_and_fonts, Fonts};
pub use platform::PlatformServices;
pub use state::AppState;

/// Window title and start-screen heading.
pub const APP_TITLE: &str = "HDTreeMap";

/// Build the whole app over `state`, using `platform` for OS services. Call
/// [`install_theme_and_fonts`] first.
pub fn build_app(state: AppState, platform: Rc<dyn PlatformServices>, fonts: Fonts) -> App {
    state::install_scan_pump();
    App::new(ui::build_root(state, platform, fonts))
}
