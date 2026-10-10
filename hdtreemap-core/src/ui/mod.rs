//! The widget tree. Built only from agg-gui widgets.
//!
//! The root is a `Rebuilder` keyed on [`AppState::version`]: when the state switches screens
//! the root rebuilds the matching screen on the next layout.
//!
//! - [`start_screen`] — app title, one card per volume, "Choose Folder…", the demo banner.
//! - [`target_screen`] — the chosen path and a Back button (replaced by the scan screen in M1).

pub mod start_screen;
pub mod target_screen;

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
            Screen::Target(target) => target_screen::build(&state, &target, &fonts),
        },
    ))
}
