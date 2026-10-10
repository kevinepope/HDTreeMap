//! The widget tree. Built only from agg-gui widgets.
//!
//! The root ([`build_root`]) is a `Stack`: the app column (the in-window menu bar, unless
//! the shell installed a native one, over the current screen), then the overlays: the
//! context-menu host and the Move to Trash and About sheets. All of it sits in
//! [`menus::CommandKeys`], which runs the menu commands' shortcuts, inside agg-gui's
//! `ToastHost`, which paints the state's toasts (`AppState::notify`) over everything in
//! the bottom-right corner, clear of the status bar. The screen is a
//! `Rebuilder` keyed on [`AppState::version`]: when the state switches screens it rebuilds
//! the matching one on the next layout. Everything follows the light or dark look
//! (`theme.rs`): most widgets paint with the current `Visuals`, and the few built with a
//! theme colour rebuild on [`AppState::theme_version`] (the start screen as a whole; in the
//! main view, the scan strip, status bar and theme button).
//!
//! - [`start_screen`] — app title, one clickable card per volume, "Choose Folder…", the
//!   demo banner.
//! - [`main_view`] — the results, live while the scan runs: toolbar with the
//!   [`breadcrumb`], the [`scan_strip`] (progress and Cancel, while scanning), the folder
//!   tree ([`folder_tree`]) and extension legend ([`ext_legend`]) beside the
//!   [`treemap_view`], and the status bar (item lines from [`describe`]).
//! - [`commands`], [`node_actions`], [`menus`] — what the menus, shortcuts and right-click
//!   menus do; [`context_menu`], [`sheets`] — the overlays that show them.
//! - [`frame_stats`] — whole-frame timing under `HDTREEMAP_STATS`.

pub mod breadcrumb;
pub mod commands;
pub mod context_menu;
pub mod describe;
pub mod ext_legend;
pub mod folder_tree;
pub mod frame_stats;
pub mod main_view;
pub mod menus;
pub mod node_actions;
pub mod scan_strip;
pub mod sheets;
pub mod start_screen;
pub mod treemap_view;

use std::rc::Rc;
use std::sync::Arc;

use agg_gui::widgets::toast::ToastHost;
use agg_gui::{HAnchor, Insets, Rebuilder, Stack, VAnchor, Widget};

use crate::fonts::Fonts;
use crate::platform::PlatformServices;
use crate::state::{AppState, Screen};
use commands::Commands;

/// Widget id of the toast host (the root).
pub const TOAST_HOST_ID: &str = "toasts";

/// Where the toasts stack: the bottom-right corner, above the status bar, so they sit
/// over the map.
const TOAST_INSET: Insets = Insets {
    left: 16.0,
    right: 16.0,
    top: 16.0,
    bottom: 44.0,
};

/// The app's root widget.
pub fn build_root(
    state: AppState,
    platform: Rc<dyn PlatformServices>,
    fonts: Fonts,
) -> Box<dyn Widget> {
    let commands = Commands::new(&state, &platform);
    let version_state = state.clone();
    let screen_state = state.clone();
    let screen_fonts = fonts.clone();
    let screen_commands = commands.clone();
    let screen = Rebuilder::new(
        move || version_state.version(),
        move || match screen_state.screen() {
            Screen::Start => {
                // Built with theme colours: rebuilt when the look changes.
                let (key, state) = (screen_state.clone(), screen_state.clone());
                let (platform, fonts) = (Rc::clone(&platform), screen_fonts.clone());
                Box::new(
                    Rebuilder::new(
                        move || key.theme_version(),
                        move || start_screen::build(&state, &platform, &fonts),
                    )
                    .with_h_anchor(HAnchor::STRETCH)
                    .with_v_anchor(VAnchor::STRETCH),
                )
            }
            Screen::Scanning(target) | Screen::Results(target) => {
                main_view::build(&screen_commands, &target, &screen_fonts)
            }
        },
    );
    let bar: Option<Box<dyn Widget>> = (!state.has_native_menu_bar())
        .then(|| Box::new(menus::build_menu_bar(&commands, &fonts)) as Box<dyn Widget>);
    let column = menus::TopBarFrame::new(bar, Box::new(screen));
    let stack = Stack::new()
        .add(Box::new(column))
        .add(Box::new(context_menu::ContextMenuHost::new(
            &state,
            commands.nodes().clone(),
            Arc::clone(&fonts.ui),
        )))
        .add(sheets::trash_sheet(&commands, &fonts))
        .add(sheets::about_sheet(&commands, &fonts));
    let keys = menus::CommandKeys::new(Box::new(stack), commands);
    Box::new(
        ToastHost::new(Box::new(keys), state.toasts(), Arc::clone(&fonts.ui))
            .with_icon_font(Arc::clone(&fonts.icons))
            .with_inset(TOAST_INSET)
            .with_name(TOAST_HOST_ID),
    )
}
