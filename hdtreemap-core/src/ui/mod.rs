//! The widget tree. Built only from agg-gui widgets.
//!
//! The root is a `Rebuilder` keyed on [`AppState::version`]: when the state switches screens
//! the root rebuilds the matching screen on the next layout.
//!
//! - [`start_screen`] — app title, one clickable card per volume, "Choose Folder…", the
//!   demo banner.
//! - [`scan_screen`] — live progress of a running scan, and Cancel.
//! - [`results_screen`] — totals, largest folders, top file types, Back and Rescan.

pub mod results_screen;
pub mod scan_screen;
pub mod start_screen;

use std::rc::Rc;

use agg_gui::{Rebuilder, Widget};

use crate::fonts::Fonts;
use crate::platform::PlatformServices;
use crate::state::{AppState, Screen};

/// The app's root widget.
pub fn build_root(
    state: AppState,
    platform: Rc<dyn PlatformServices>,
    fonts: Fonts,
) -> Box<dyn Widget> {
    let version_state = state.clone();
    Box::new(Rebuilder::new(
        move || version_state.version(),
        move || match state.screen() {
            Screen::Start => start_screen::build(&state, &platform, &fonts),
            Screen::Scanning(target) => scan_screen::build(&state, &target, &fonts),
            Screen::Results(target) => results_screen::build(&state, &platform, &target, &fonts),
        },
    ))
}
