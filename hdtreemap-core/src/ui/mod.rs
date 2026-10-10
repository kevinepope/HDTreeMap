//! The widget tree. Built only from agg-gui widgets.
//!
//! The root ([`build_root`]) is a `Stack`: the app column (the in-window menu bar, unless
//! the shell installed a native one, over the current screen), then the overlays: the
//! context-menu host, the toasts, and the Move to Trash and About sheets. All of it sits
//! in [`menus::CommandKeys`], which runs the menu commands' shortcuts. The screen is a
//! `Rebuilder` keyed on [`AppState::version`]: when the state switches screens it rebuilds
//! the matching one on the next layout.
//!
//! - [`start_screen`] — app title, one clickable card per volume, "Choose Folder…", the
//!   demo banner.
//! - [`scan_screen`] — live progress of a running scan, and Cancel.
//! - [`main_view`] — the results: toolbar with the [`breadcrumb`], the folder tree
//!   ([`folder_tree`]) and extension legend ([`ext_legend`]) beside the [`treemap_view`],
//!   and the status bar (item lines from [`describe`]).
//! - [`commands`], [`node_actions`], [`menus`] — what the menus, shortcuts and right-click
//!   menus do; [`context_menu`], [`toast`], [`sheets`] — the overlays that show them.
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
pub mod scan_screen;
pub mod sheets;
pub mod start_screen;
pub mod toast;
pub mod treemap_view;

use std::rc::Rc;
use std::sync::Arc;

use agg_gui::{Rebuilder, Stack, Widget};

use crate::fonts::Fonts;
use crate::platform::PlatformServices;
use crate::state::{AppState, Screen};
use commands::Commands;

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
            Screen::Start => start_screen::build(&screen_state, &platform, &screen_fonts),
            Screen::Scanning(target) => scan_screen::build(&screen_state, &target, &screen_fonts),
            Screen::Results(target) => main_view::build(&screen_commands, &target, &screen_fonts),
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
        .add(Box::new(toast::Toasts::new(&state, Arc::clone(&fonts.ui))))
        .add(sheets::trash_sheet(&commands, &fonts))
        .add(sheets::about_sheet(&commands, &fonts));
    Box::new(menus::CommandKeys::new(Box::new(stack), commands))
}
