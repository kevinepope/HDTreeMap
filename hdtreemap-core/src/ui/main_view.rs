//! The main results view, shown when a scan finishes: a toolbar (back to the drive list,
//! the zoom breadcrumb, size mode, cushion shading, Rescan), the treemap filling the middle,
//! and a status bar (hovered or selected item, totals, unreadable items, free space).
//!
//! The view is built once per screen; the parts that change while it is up rebuild on
//! their own `Rebuilder`s keyed on the finer `AppState` versions (`state/view.rs`): the
//! breadcrumb on the map version, the item line on the detail version, the totals on the
//! tree version. The treemap widget itself persists and keeps its raster cache.
//!
//! [`build_centre`] is the middle of the view on its own, the seam where M3 puts the folder
//! tree and the extension legend beside the map (in a `Splitter`).

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::{
    current_visuals, Button, Checkbox, Container, EllipsisMode, FlexColumn, FlexRow, HAnchor,
    Insets, Label, Rebuilder, SegmentedControl, VAnchor, Widget,
};

use super::breadcrumb;
use super::describe;
use super::start_screen::start_scan;
use super::treemap_view::{TreemapView, TREEMAP_ID};
use crate::fonts::{fa, Fonts};
use crate::format::{format_bytes, format_count};
use crate::model::SizeMode;
use crate::platform::{volume_containing, PlatformServices};
use crate::render::Shading;
use crate::state::{AppState, ScanTarget};

/// Widget id of the whole view.
pub const MAIN_VIEW_ID: &str = "main-view";
/// Widget id of the back-to-drives button.
pub const BACK_ID: &str = "main-back";
/// Widget id of the Rescan button.
pub const RESCAN_ID: &str = "main-rescan";
/// Widget id of the size-mode switch.
pub const SIZE_MODE_ID: &str = "main-size-mode";
/// Widget id of the cushion-shading checkbox.
pub const SHADING_ID: &str = "main-shading";
/// Widget id of the status bar's hovered / selected item line.
pub const STATUS_ITEM_ID: &str = "status-item";
/// Widget id of the status bar's totals.
pub const STATUS_TOTALS_ID: &str = "status-totals";
/// Widget id of the status bar's unreadable-items note.
pub const STATUS_ERRORS_ID: &str = "status-errors";
/// Widget id of the status bar's free-space note.
pub const STATUS_FREE_ID: &str = "status-free";

/// What the status bar's item line says with nothing hovered or selected.
pub const STATUS_IDLE_TEXT: &str = "Point at the map to see what uses the space.";

/// Status-bar text size.
const STATUS_FONT: f64 = 13.0;

/// Build the main view for `target` from the finished scan in `state`.
pub fn build(
    state: &AppState,
    platform: &Rc<dyn PlatformServices>,
    target: &ScanTarget,
    fonts: &Fonts,
) -> Box<dyn Widget> {
    Box::new(
        FlexColumn::new()
            .with_panel_bg()
            .with_top_anchor(true)
            .add(toolbar(state, platform, target, fonts))
            .add_flex(build_centre(state), 1.0)
            .add(status_bar(state, platform, target, fonts))
            .with_name(MAIN_VIEW_ID),
    )
}

/// The middle of the view: the treemap.
pub fn build_centre(state: &AppState) -> Box<dyn Widget> {
    Box::new(TreemapView::new(state).with_name(TREEMAP_ID))
}

fn toolbar(
    state: &AppState,
    platform: &Rc<dyn PlatformServices>,
    target: &ScanTarget,
    fonts: &Fonts,
) -> Box<dyn Widget> {
    let back_state = state.clone();
    let back = Button::new("Drives", Arc::clone(&fonts.ui))
        .with_icon(fa::ARROW_LEFT, Arc::clone(&fonts.icons))
        .with_focus_on_click(false)
        .on_click(move || back_state.back_to_start())
        .with_tooltip("Back to the list of drives")
        .with_v_anchor(VAnchor::CENTER)
        .with_name(BACK_ID);

    let crumb_state = state.clone();
    let version_state = state.clone();
    let crumb_target = target.clone();
    let crumb_fonts = fonts.clone();
    let crumbs = Rebuilder::new(
        move || version_state.map_version(),
        move || breadcrumb::build(&crumb_state, &crumb_target, &crumb_fonts),
    )
    .with_h_anchor(HAnchor::STRETCH)
    .with_v_anchor(VAnchor::CENTER);

    let modes = [SizeMode::Allocated, SizeMode::Apparent];
    let selected = Rc::new(Cell::new(usize::from(
        state.size_mode() == SizeMode::Apparent,
    )));
    let mode_state = state.clone();
    let size_mode = SegmentedControl::new(
        modes.iter().map(|m| m.label()).collect(),
        selected,
        Arc::clone(&fonts.ui),
    )
    .with_fit_width(true)
    .on_change(move |i| mode_state.set_size_mode(modes[i.min(1)]))
    .with_v_anchor(VAnchor::CENTER)
    .with_name(SIZE_MODE_ID);

    let shading_state = state.clone();
    let shading = Checkbox::new(
        "Cushions",
        Arc::clone(&fonts.ui),
        state.shading() == Shading::Cushion,
    )
    .on_change(move |on| {
        shading_state.set_shading(if on { Shading::Cushion } else { Shading::Flat })
    })
    .with_v_anchor(VAnchor::CENTER)
    .with_name(SHADING_ID);

    let rescan_state = state.clone();
    let rescan_platform = Rc::clone(platform);
    let rescan_target = target.clone();
    let rescan = Button::new("Rescan", Arc::clone(&fonts.ui))
        .with_icon(fa::REFRESH, Arc::clone(&fonts.icons))
        .with_focus_on_click(false)
        .on_click(move || start_scan(&rescan_state, &rescan_platform, rescan_target.clone()))
        .with_v_anchor(VAnchor::CENTER)
        .with_name(RESCAN_ID);

    Box::new(
        FlexRow::new()
            .with_gap(10.0)
            .with_inner_padding(Insets::symmetric(10.0, 8.0))
            .with_h_anchor(HAnchor::STRETCH)
            .add(Box::new(back))
            .add_flex(Box::new(crumbs), 1.0)
            .add(Box::new(size_mode))
            .add(Box::new(shading))
            .add(Box::new(rescan)),
    )
}

fn status_bar(
    state: &AppState,
    platform: &Rc<dyn PlatformServices>,
    target: &ScanTarget,
    fonts: &Fonts,
) -> Box<dyn Widget> {
    let item_state = state.clone();
    let item_version = state.clone();
    let item_fonts = fonts.clone();
    let item = Rebuilder::new(
        move || item_version.detail_version(),
        move || item_line(&item_state, &item_fonts),
    )
    .with_h_anchor(HAnchor::STRETCH)
    .with_v_anchor(VAnchor::CENTER);

    let totals_state = state.clone();
    let totals_version = state.clone();
    let totals_fonts = fonts.clone();
    let totals = Rebuilder::new(
        move || totals_version.tree_version(),
        move || totals_text(&totals_state, &totals_fonts),
    )
    .with_v_anchor(VAnchor::CENTER);

    let mut row = FlexRow::new()
        .with_gap(18.0)
        .with_h_anchor(HAnchor::STRETCH)
        .add_flex(Box::new(item), 1.0)
        .add(Box::new(totals));
    if let Some(errors) = errors_note(state, platform, fonts) {
        row = row.add(errors);
    }
    let free = target.free_bytes.or_else(|| {
        volume_containing(&platform.list_volumes(), &target.path).map(|v| v.free_bytes)
    });
    if let Some(free) = free {
        row = row.add(status_label(
            format!("{} free", format_bytes(free)),
            fonts,
            STATUS_FREE_ID,
        ));
    }
    let visuals = current_visuals();
    Box::new(
        Container::new()
            .with_fit_height(true)
            .with_background(visuals.top_bar_bg)
            .with_inner_padding(Insets::symmetric(12.0, 6.0))
            .with_h_anchor(HAnchor::STRETCH)
            .add(Box::new(row)),
    )
}

/// The hovered item, else the selected one, else a hint.
fn item_line(state: &AppState, fonts: &Fonts) -> Box<dyn Widget> {
    let text = state
        .with_result(|r, _| {
            let hover = state.hovered().or_else(|| {
                state.selected().map(|node| crate::state::Hover {
                    node,
                    aggregate: None,
                })
            })?;
            Some(describe::status_line(&r.tree, hover))
        })
        .flatten()
        .unwrap_or_else(|| STATUS_IDLE_TEXT.to_string());
    Box::new(
        Label::new(text, Arc::clone(&fonts.ui))
            .with_font_size(STATUS_FONT)
            .with_ellipsis_mode(EllipsisMode::PathMiddle)
            .with_h_anchor(HAnchor::STRETCH)
            .with_v_anchor(VAnchor::CENTER)
            .with_name(STATUS_ITEM_ID),
    )
}

/// `"24.3 GB · 12,345 files · 678 folders"`, in the active size mode.
fn totals_text(state: &AppState, fonts: &Fonts) -> Box<dyn Widget> {
    let text = state
        .with_result(|r, _| {
            let tree = &r.tree;
            format!(
                "{} \u{b7} {} \u{b7} {} folders",
                format_bytes(tree.size(tree.root())),
                describe::files_phrase(tree.file_count()),
                format_count(tree.dir_count())
            )
        })
        .unwrap_or_default();
    status_label(text, fonts, STATUS_TOTALS_ID)
}

/// `"⚠ 12 unreadable"`, with a Full Disk Access hint on macOS (native).
fn errors_note(
    state: &AppState,
    platform: &Rc<dyn PlatformServices>,
    fonts: &Fonts,
) -> Option<Box<dyn Widget>> {
    let total = state.with_result(|r, _| r.errors.total())?;
    if total == 0 {
        return None;
    }
    let mut text = format!("{}  {} unreadable", fa::WARNING, format_count(total));
    if cfg!(target_os = "macos") && !platform.is_demo() {
        text.push_str(" \u{2014} grant Full Disk Access in System Settings to read them");
    }
    Some(status_label(text, fonts, STATUS_ERRORS_ID))
}

fn status_label(text: String, fonts: &Fonts, id: &str) -> Box<dyn Widget> {
    Box::new(
        Label::new(text, Arc::clone(&fonts.ui))
            .with_font_size(STATUS_FONT)
            .with_dim(true)
            .with_v_anchor(VAnchor::CENTER)
            .with_name(id),
    )
}
