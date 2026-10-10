//! # hdtreemap-core
//!
//! The target-agnostic core of HDTreeMap, a disk-usage treemap viewer (see `docs/PLAN.md`).
//! Both shells build the same app from here:
//!
//! - `hdtreemap-native` calls [`build_app`] with its real `PlatformServices`;
//! - `hdtreemap-wasm` calls it with [`platform::DemoPlatform`].
//!
//! Modules:
//! - [`fonts`] — theme + font installation and the Font Awesome code points.
//! - [`format`] — human-readable sizes.
//! - [`platform`] — the `PlatformServices` contract, volume-list cleanup, the demo platform.
//! - [`state`] — the shared app state (which screen, which scan target).
//! - [`ui`] — the widget tree (start screen, target screen).

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
    App::new(ui::build_root(state, platform, fonts))
}
