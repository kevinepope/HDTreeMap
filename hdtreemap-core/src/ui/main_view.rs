//! The main results view: a toolbar (back to the drive list, the zoom breadcrumb, size mode,
//! cushion shading, Rescan), the three linked views in the middle, and a status bar
//! (hovered or selected item, highlighted extension, totals, unreadable items, free space).
//!
//! It is shown from the moment a scan starts: while the scan runs, the [`scan_strip`]
//! (counters, progress, Cancel) sits under the toolbar and the views show the tree scanned
//! so far, refreshed on each live tick (`state/live.rs`); Rescan waits until the scan is
//! done. When the scan finishes the strip goes and the same widgets show the final tree.
//!
//! [`scan_strip`]: super::scan_strip
//!
//! The middle ([`build_centre`]) is a horizontal `Splitter`: on the left a column split
//! again, the folder tree above the extension legend; the treemap fills the right. The left
//! column is a vertical split rather than a legend along the map's bottom so the map keeps
//! the full height and both lists, which read as one sidebar of rows, share its width. Both
//! dividers are bound to `AppState` handles, so they keep their place for the session.
//!
//! The view is built once per screen; the parts that change while it is up rebuild on
//! their own `Rebuilder`s keyed on the finer `AppState` versions (`state/view.rs`): the
//! breadcrumb on the map version, the item and highlight lines on the detail version, the
//! totals on the tree version. The treemap, tree and legend widgets persist and follow the
//! state themselves.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::{
    current_visuals, Button, Checkbox, Container, EllipsisMode, FlexColumn, FlexRow, HAnchor,
    Insets, Label, Rebuilder, SegmentedControl, Splitter, VAnchor, Widget,
};

use super::breadcrumb;
use super::commands::Commands;
use super::describe;
use super::ext_legend::{ExtLegend, EXT_LEGEND_ID, NO_EXTENSION};
use super::folder_tree::{FolderTree, FOLDER_TREE_ID};
use super::scan_strip;
use super::start_screen::start_scan;
use super::treemap_view::{TreemapView, TREEMAP_ID};
use crate::fonts::{fa, Fonts};
use crate::format::{format_bytes, format_count};
use crate::model::SizeMode;
use crate::platform::PlatformServices;
use crate::render::{ColorBy, Shading};
use crate::state::{AppState, ScanTarget};

/// Widget id of the whole view.
pub const MAIN_VIEW_ID: &str = "main-view";
/// Widget id of the back-to-drives button.
pub const BACK_ID: &str = "main-back";
/// Widget id of the Rescan button.
pub const RESCAN_ID: &str = "main-rescan";
/// Widget id of the size-mode switch.
pub const SIZE_MODE_ID: &str = "main-size-mode";
/// Widget id of the colour-scheme switch (file type / top-level folder).
pub const COLOR_BY_ID: &str = "main-color-by";
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
/// Widget id of the status bar's highlighted-extension note.
pub const STATUS_HIGHLIGHT_ID: &str = "status-highlight";
/// Widget id of the status bar's folder-rescan progress.
pub const STATUS_RESCAN_ID: &str = "status-rescan";
/// Widget id of the splitter between the left column and the map.
pub const SIDE_SPLITTER_ID: &str = "side-splitter";
/// Widget id of the splitter between the folder tree and the legend.
pub const TREE_SPLITTER_ID: &str = "tree-splitter";

/// What the status bar's item line says with nothing hovered or selected.
pub const STATUS_IDLE_TEXT: &str = "Point at the map to see what uses the space.";

/// Status-bar text size.
const STATUS_FONT: f64 = 13.0;

/// Build the main view for `target` from the scan in the commands' state (live or finished).
pub fn build(commands: &Commands, target: &ScanTarget, fonts: &Fonts) -> Box<dyn Widget> {
    let state = commands.state();
    let platform = commands.platform();
    Box::new(
        FlexColumn::new()
            .with_panel_bg()
            .with_top_anchor(true)
            .add(toolbar(state, platform, target, fonts))
            .add(scan_strip::build(state, target, fonts))
            .add_flex(build_centre(state, fonts), 1.0)
            .add(status_bar(state, platform, target, fonts))
            .with_name(MAIN_VIEW_ID),
    )
}

/// The middle of the view: folder tree over extension legend, beside the treemap.
pub fn build_centre(state: &AppState, fonts: &Fonts) -> Box<dyn Widget> {
    let tree = FolderTree::new(state, fonts).with_name(FOLDER_TREE_ID);
    let legend = ExtLegend::new(state, &fonts.ui).with_name(EXT_LEGEND_ID);
    let side = Splitter::vertical(Box::new(tree), Box::new(legend))
        .with_ratio_handle(state.tree_split())
        .with_h_anchor(HAnchor::STRETCH)
        .with_v_anchor(VAnchor::STRETCH)
        .with_name(TREE_SPLITTER_ID);
    let map = TreemapView::new(state).with_name(TREEMAP_ID);
    Box::new(
        Splitter::new(Box::new(side), Box::new(map))
            .with_ratio_handle(state.side_split())
            .with_h_anchor(HAnchor::STRETCH)
            .with_v_anchor(VAnchor::STRETCH)
            .with_name(SIDE_SPLITTER_ID),
    )
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

    // Rebuilt when the map changes, so a size mode or shading picked from the View menu
    // shows here too.
    let view_state = state.clone();
    let view_version = state.clone();
    let view_fonts = fonts.clone();
    let view_controls = Rebuilder::new(
        move || view_version.map_version(),
        move || view_toggles(&view_state, &view_fonts),
    )
    .with_v_anchor(VAnchor::CENTER);

    let rescan_state = state.clone();
    let enabled_state = state.clone();
    let rescan_platform = Rc::clone(platform);
    let rescan_target = target.clone();
    let rescan = Button::new("Rescan", Arc::clone(&fonts.ui))
        .with_icon(fa::REFRESH, Arc::clone(&fonts.icons))
        .with_focus_on_click(false)
        .with_enabled_fn(move || !enabled_state.is_scanning())
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
            .add(Box::new(view_controls))
            .add(Box::new(rescan)),
    )
}

/// The size-mode and colour-scheme switches and the cushion checkbox, showing the state as
/// it is now.
fn view_toggles(state: &AppState, fonts: &Fonts) -> Box<dyn Widget> {
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

    let schemes = [ColorBy::Extension, ColorBy::TopFolder];
    let scheme = Rc::new(Cell::new(usize::from(
        state.color_by() == ColorBy::TopFolder,
    )));
    let scheme_state = state.clone();
    let color_by =
        SegmentedControl::new(vec!["By Type", "By Folder"], scheme, Arc::clone(&fonts.ui))
            .with_fit_width(true)
            .on_change(move |i| scheme_state.set_color_by(schemes[i.min(1)]))
            .with_tooltip("Colour the map by file type or by top-level folder")
            .with_v_anchor(VAnchor::CENTER)
            .with_name(COLOR_BY_ID);

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
    Box::new(
        FlexRow::new()
            .with_gap(10.0)
            .with_fit_width(true)
            .add(Box::new(size_mode))
            .add(Box::new(color_by))
            .add(Box::new(shading)),
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

    let highlight_state = state.clone();
    let highlight_version = state.clone();
    let highlight_fonts = fonts.clone();
    let highlight = Rebuilder::new(
        move || highlight_version.detail_version(),
        move || highlight_note(&highlight_state, &highlight_fonts),
    )
    .with_v_anchor(VAnchor::CENTER);

    let rescan_state = state.clone();
    let rescan_version = state.clone();
    let rescan_fonts = fonts.clone();
    let rescan = Rebuilder::new(
        move || rescan_version.progress_version(),
        move || rescan_note(&rescan_state, &rescan_fonts),
    )
    .with_v_anchor(VAnchor::CENTER);

    let errors_state = state.clone();
    let errors_version = state.clone();
    let errors_platform = Rc::clone(platform);
    let errors_fonts = fonts.clone();
    let errors = Rebuilder::new(
        move || errors_version.tree_version(),
        move || {
            errors_note(&errors_state, &errors_platform, &errors_fonts)
                .unwrap_or_else(|| empty_note(&errors_fonts))
        },
    )
    .with_v_anchor(VAnchor::CENTER);

    let mut row = FlexRow::new()
        .with_gap(18.0)
        .with_h_anchor(HAnchor::STRETCH)
        .add(Box::new(rescan))
        .add_flex(Box::new(item), 1.0)
        .add(Box::new(highlight))
        .add(Box::new(totals))
        .add(Box::new(errors));
    let free_state = state.clone();
    let free_version = state.clone();
    let free_target = target.clone();
    let free_fonts = fonts.clone();
    row = row.add(Box::new(
        Rebuilder::new(
            move || free_version.volumes_version(),
            move || free_note(&free_state, &free_target, &free_fonts),
        )
        .with_v_anchor(VAnchor::CENTER),
    ));
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

/// `"12.3 GB free"` on the scanned volume: the chosen volume's figure, else the cached
/// volume list's (never listed here: that can hang on a dead network mount). Nothing
/// while unknown.
fn free_note(state: &AppState, target: &ScanTarget, fonts: &Fonts) -> Box<dyn Widget> {
    let root = state.with_result(|r, _| r.root.to_path_buf());
    let free = target.free_bytes.or_else(|| {
        let path = root.as_deref().unwrap_or(&target.path);
        state.free_space_for(path)
    });
    match free {
        Some(free) => status_label(
            format!("{} free", format_bytes(free)),
            fonts,
            STATUS_FREE_ID,
        ),
        None => empty_note(fonts),
    }
}

/// `"⟳ Rescanning Photos… 1,234 files · 3.4 GB"` while a folder rescan runs; nothing
/// otherwise.
pub fn rescan_text(state: &AppState) -> Option<String> {
    let status = state.rescan_status()?;
    let name = status.path.file_name().map_or_else(
        || status.path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    Some(format!(
        "{}  Rescanning {name}\u{2026} {} \u{b7} {}",
        fa::REFRESH,
        describe::files_phrase(status.progress.files),
        format_bytes(status.progress.bytes)
    ))
}

fn rescan_note(state: &AppState, fonts: &Fonts) -> Box<dyn Widget> {
    match rescan_text(state) {
        Some(text) => Box::new(
            Label::new(text, Arc::clone(&fonts.ui))
                .with_font_size(STATUS_FONT)
                .with_color(current_visuals().accent)
                .with_v_anchor(VAnchor::CENTER)
                .with_name(STATUS_RESCAN_ID),
        ),
        None => empty_note(fonts),
    }
}

/// An empty status-bar slot (an empty label takes no room in the row).
fn empty_note(fonts: &Fonts) -> Box<dyn Widget> {
    Box::new(
        Label::new(String::new(), Arc::clone(&fonts.ui))
            .with_font_size(STATUS_FONT)
            .with_v_anchor(VAnchor::CENTER),
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
            // A removed node (moved to the trash, replaced by a rescan) is not described.
            if !r.tree.is_live(hover.node) {
                return None;
            }
            Some(describe::status_line(r.tree, hover))
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

/// `"Highlighting .mov: 37 files, 120 GB"` (in the zoom root) while an extension is
/// highlighted; nothing otherwise.
pub fn highlight_text(state: &AppState) -> Option<String> {
    let ext = state.highlight_ext()?;
    let name = state.with_result(|r, _| r.tree.ext_name(ext).to_string())?;
    let shown = if name.is_empty() {
        NO_EXTENSION.to_string()
    } else {
        format!(".{name}")
    };
    let table = state.zoom_extensions()?;
    Some(match table.by_ext(ext) {
        Some(row) => format!(
            "Highlighting {shown}: {}, {}",
            describe::files_phrase(row.count),
            format_bytes(row.sizes.get(state.size_mode()))
        ),
        None => format!("Highlighting {shown}: no files here"),
    })
}

/// The highlight note (an empty label while nothing is highlighted).
fn highlight_note(state: &AppState, fonts: &Fonts) -> Box<dyn Widget> {
    Box::new(
        Label::new(
            highlight_text(state).unwrap_or_default(),
            Arc::clone(&fonts.ui),
        )
        .with_font_size(STATUS_FONT)
        .with_v_anchor(VAnchor::CENTER)
        .with_name(STATUS_HIGHLIGHT_ID),
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
