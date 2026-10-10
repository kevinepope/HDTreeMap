//! The results screen, shown when a scan finishes (milestone M2 adds the treemap above it):
//! totals (size, files, folders, unreadable items, scan time), a size-mode switch, the ten
//! largest folders and the ten file types using the most space, the first unreadable paths,
//! and Back / Rescan.
//!
//! Everything shown is read from the finished scan in `AppState`; switching the size mode
//! re-sorts the tree there and bumps the state version, so the root `Rebuilder` rebuilds
//! this screen with the new numbers.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use agg_gui::{
    Button, FlexColumn, FlexRow, HAnchor, Insets, Label, LabelAlign, SegmentedControl, Size,
    SizedBox, Spacer, VAnchor, Widget,
};

use super::scan_screen::stat;
use super::start_screen::start_scan;
use crate::fonts::{fa, Fonts};
use crate::format::{format_bytes, format_count, format_duration, format_percent};
use crate::model::SizeMode;
use crate::platform::PlatformServices;
use crate::scan::ScanResult;
use crate::state::{AppState, ScanTarget};

/// Widget id of the whole screen.
pub const RESULTS_SCREEN_ID: &str = "results-screen";
/// Widget id of the Back button.
pub const RESULTS_BACK_ID: &str = "results-back";
/// Widget id of the Rescan button.
pub const RESCAN_ID: &str = "results-rescan";
/// Widget id of the size-mode switch.
pub const SIZE_MODE_ID: &str = "results-size-mode";
/// Widget id of the largest-folders list.
pub const LARGEST_FOLDERS_ID: &str = "results-largest-folders";
/// Widget id of the top-extensions list.
pub const TOP_EXTENSIONS_ID: &str = "results-top-extensions";
/// Widget id of the unreadable-items note.
pub const ERRORS_ID: &str = "results-errors";

/// Rows in each top-ten list.
const TOP_N: usize = 10;
/// Unreadable paths listed by name.
const ERROR_PATHS_SHOWN: usize = 3;
/// Width of the size column in the lists.
const SIZE_COLUMN: f64 = 90.0;

/// Build the results screen for `target` from the finished scan in `state`.
pub fn build(
    state: &AppState,
    platform: &Rc<dyn PlatformServices>,
    target: &ScanTarget,
    fonts: &Fonts,
) -> Box<dyn Widget> {
    let back_state = state.clone();
    let back = Button::new("Back", Arc::clone(&fonts.ui))
        .with_icon(fa::ARROW_LEFT, Arc::clone(&fonts.icons))
        .with_cancel_action()
        .on_click(move || back_state.back_to_start())
        .with_name(RESULTS_BACK_ID);
    let rescan_state = state.clone();
    let rescan_platform = Rc::clone(platform);
    let rescan_target = target.clone();
    let rescan = Button::new("Rescan", Arc::clone(&fonts.ui))
        .with_icon(fa::REFRESH, Arc::clone(&fonts.icons))
        .on_click(move || start_scan(&rescan_state, &rescan_platform, rescan_target.clone()))
        .with_name(RESCAN_ID);

    let mut column = FlexColumn::new()
        .with_panel_bg()
        .with_top_anchor(true)
        .with_gap(12.0)
        .with_inner_padding(Insets::all(28.0))
        .add(Box::new(
            FlexRow::new()
                .with_gap(10.0)
                .with_h_anchor(HAnchor::STRETCH)
                .add(Box::new(back))
                .add_flex(
                    Box::new(Spacer::new().with_max_size(Size::new(f64::MAX, 0.0))),
                    1.0,
                )
                .add(Box::new(rescan)),
        ))
        .add(Box::new(
            Label::new(
                format!("Scan of {}", target.display_name()),
                Arc::clone(&fonts.ui),
            )
            .with_font_size(22.0)
            .with_strong(true)
            .with_ellipsis_if_clipped(true)
            .with_h_anchor(HAnchor::STRETCH),
        ));
    let body = state.with_result(|result, extensions| {
        let mode = result.tree.size_mode();
        let mut body = FlexColumn::new()
            .with_gap(12.0)
            .with_top_anchor(true)
            .with_h_anchor(HAnchor::STRETCH)
            .add(Box::new(
                Label::new(
                    result.root.to_string_lossy().into_owned(),
                    Arc::clone(&fonts.ui),
                )
                .with_font_size(13.0)
                .with_dim(true)
                .with_ellipsis_if_clipped(true)
                .with_h_anchor(HAnchor::STRETCH),
            ))
            .add(summary(result, fonts))
            .add(size_mode_switch(state, mode, fonts))
            .add(Box::new(
                FlexRow::new()
                    .with_gap(32.0)
                    .with_h_anchor(HAnchor::STRETCH)
                    .add_flex(largest_folders(result, fonts), 1.0)
                    .add_flex(top_extensions(result, extensions, mode, fonts), 1.0),
            ));
        if !result.errors.is_empty() {
            body = body.add(errors_note(result, fonts));
        }
        body
    });
    if let Some(body) = body {
        column = column.add(Box::new(body));
    }
    Box::new(
        column
            .add_flex(Box::new(Spacer::new()), 1.0)
            .with_name(RESULTS_SCREEN_ID),
    )
}

fn summary(result: &ScanResult, fonts: &Fonts) -> Box<dyn Widget> {
    let tree = &result.tree;
    Box::new(
        FlexRow::new()
            .with_gap(28.0)
            .with_h_anchor(HAnchor::STRETCH)
            .add(stat(
                &format_bytes(tree.size(tree.root())),
                &tree.size_mode().label().to_lowercase(),
                fonts,
            ))
            .add(stat(&format_count(tree.file_count()), "files", fonts))
            .add(stat(&format_count(tree.dir_count()), "folders", fonts))
            .add(stat(
                &format_count(result.errors.total()),
                "unreadable",
                fonts,
            ))
            .add(stat(&format_duration(result.elapsed), "scan time", fonts)),
    )
}

fn size_mode_switch(state: &AppState, mode: SizeMode, fonts: &Fonts) -> Box<dyn Widget> {
    let modes = [SizeMode::Allocated, SizeMode::Apparent];
    let selected = Rc::new(Cell::new(usize::from(mode == SizeMode::Apparent)));
    let switch_state = state.clone();
    Box::new(
        SegmentedControl::new(
            modes.iter().map(|m| m.label()).collect(),
            selected,
            Arc::clone(&fonts.ui),
        )
        .with_fit_width(true)
        .on_change(move |i| switch_state.set_size_mode(modes[i.min(1)]))
        .with_name(SIZE_MODE_ID),
    )
}

fn largest_folders(result: &ScanResult, fonts: &Fonts) -> Box<dyn Widget> {
    let tree = &result.tree;
    let total = tree.size(tree.root());
    let rows: Vec<(String, String, String)> = tree
        .largest_folders(TOP_N)
        .into_iter()
        .map(|id| {
            let size = tree.size(id);
            (
                tree.relative_path(id),
                format_bytes(size),
                format_percent(size, total),
            )
        })
        .collect();
    list(
        "Largest folders",
        rows,
        "No folders.",
        fonts,
        LARGEST_FOLDERS_ID,
    )
}

fn top_extensions(
    result: &ScanResult,
    extensions: &crate::model::ExtTable,
    mode: SizeMode,
    fonts: &Fonts,
) -> Box<dyn Widget> {
    let total = result.tree.size(result.tree.root());
    let rows: Vec<(String, String, String)> = extensions
        .top(TOP_N)
        .iter()
        .map(|row| {
            let name = if row.name.is_empty() {
                "(no extension)".to_string()
            } else {
                format!(".{}", row.name)
            };
            let size = row.sizes.get(mode);
            (
                format!(
                    "{name}  \u{b7}  {} {}",
                    format_count(row.count),
                    if row.count == 1 { "file" } else { "files" }
                ),
                format_bytes(size),
                format_percent(size, total),
            )
        })
        .collect();
    list(
        "Top file types",
        rows,
        "No files.",
        fonts,
        TOP_EXTENSIONS_ID,
    )
}

/// A titled list of `(name, size, percent)` rows.
fn list(
    title: &str,
    rows: Vec<(String, String, String)>,
    empty: &str,
    fonts: &Fonts,
    id: &str,
) -> Box<dyn Widget> {
    let mut column = FlexColumn::new()
        .with_gap(5.0)
        .with_top_anchor(true)
        .with_h_anchor(HAnchor::STRETCH)
        .add(Box::new(
            Label::new(title, Arc::clone(&fonts.ui))
                .with_font_size(16.0)
                .with_strong(true),
        ));
    if rows.is_empty() {
        column = column.add(Box::new(
            Label::new(empty, Arc::clone(&fonts.ui)).with_dim(true),
        ));
    }
    for (name, size, percent) in rows {
        column = column.add(Box::new(
            FlexRow::new()
                .with_gap(10.0)
                .with_h_anchor(HAnchor::STRETCH)
                .add_flex(
                    Box::new(
                        Label::new(name, Arc::clone(&fonts.ui))
                            .with_font_size(13.0)
                            .with_ellipsis_if_clipped(true)
                            .with_h_anchor(HAnchor::STRETCH)
                            .with_v_anchor(VAnchor::CENTER),
                    ),
                    1.0,
                )
                .add(right_aligned(&size, SIZE_COLUMN, false, fonts))
                .add(right_aligned(&percent, 54.0, true, fonts)),
        ));
    }
    Box::new(column.with_name(id))
}

fn right_aligned(text: &str, width: f64, dim: bool, fonts: &Fonts) -> Box<dyn Widget> {
    Box::new(
        SizedBox::new().with_width(width).with_child(Box::new(
            Label::new(text, Arc::clone(&fonts.ui))
                .with_font_size(13.0)
                .with_dim(dim)
                .with_align(LabelAlign::Right)
                .with_h_anchor(HAnchor::STRETCH),
        )),
    )
}

fn errors_note(result: &ScanResult, fonts: &Fonts) -> Box<dyn Widget> {
    let total = result.errors.total();
    let noun = if total == 1 { "item" } else { "items" };
    let mut column = FlexColumn::new()
        .with_gap(3.0)
        .with_top_anchor(true)
        .with_h_anchor(HAnchor::STRETCH)
        .add(Box::new(
            Label::new(
                format!(
                    "{}  {} {noun} could not be read",
                    fa::WARNING,
                    format_count(total)
                ),
                Arc::clone(&fonts.ui),
            )
            .with_font_size(14.0)
            .with_strong(true),
        ));
    for error in result.errors.entries().iter().take(ERROR_PATHS_SHOWN) {
        column = column.add(Box::new(
            Label::new(
                format!(
                    "{} \u{2014} {}",
                    error.path.to_string_lossy(),
                    error.message
                ),
                Arc::clone(&fonts.ui),
            )
            .with_font_size(12.0)
            .with_dim(true)
            .with_ellipsis_if_clipped(true)
            .with_h_anchor(HAnchor::STRETCH),
        ));
    }
    Box::new(column.with_name(ERRORS_ID))
}
