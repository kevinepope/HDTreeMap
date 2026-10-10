//! The widget tree. Built only from agg-gui widgets.
//!
//! The root is a `Rebuilder` keyed on [`AppState::version`]: when the state switches screens
//! the root rebuilds the matching screen on the next layout.
//!
//! - [`start_screen`] — app title, one clickable card per volume, "Choose Folder…", the
//!   demo banner.
//! - [`scan_screen`] — live progress of a running scan, and Cancel.
//! - [`main_view`] — the results: toolbar with the [`breadcrumb`], the [`treemap_view`],
//!   and the status bar (item lines from [`describe`]).
//! - [`frame_stats`] — whole-frame timing under `HDTREEMAP_STATS`.

pub mod breadcrumb;
pub mod describe;
pub mod ext_legend;
pub mod folder_tree;
pub mod frame_stats;
pub mod main_view;
pub mod scan_screen;
pub mod start_screen;
pub mod treemap_view;

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
            Screen::Results(target) => main_view::build(&state, &platform, &target, &fonts),
        },
    ))
}
