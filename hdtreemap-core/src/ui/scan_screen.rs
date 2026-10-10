//! The scan screen, shown while a scan runs: what is being scanned, a spinner, a progress
//! bar when the expected size is known (a volume's used space), live counters (files,
//! folders, bytes, errors, elapsed), the folder being read, and Cancel (back to the start
//! screen).
//!
//! The live part sits in a `Rebuilder` keyed on `AppState::progress_version`, which the scan
//! pump (`state/pump.rs`) ticks at most ten times a second, so the counters refresh without
//! rebuilding (and restarting the spinner of) the rest of the screen.

use std::sync::Arc;

use agg_gui::{
    Button, FlexColumn, FlexRow, HAnchor, Insets, Label, ProgressBar, Rebuilder, Spacer, Spinner,
    VAnchor, Widget,
};

use crate::fonts::{fa, Fonts};
use crate::format::{format_bytes, format_count, format_elapsed};
use crate::state::{AppState, ScanTarget};

/// Widget id of the whole screen.
pub const SCAN_SCREEN_ID: &str = "scan-screen";
/// Widget id of the Cancel button.
pub const CANCEL_BUTTON_ID: &str = "scan-cancel";
/// Widget id of the live counters row.
pub const SCAN_COUNTERS_ID: &str = "scan-counters";
/// Widget id of the label showing the folder being read.
pub const CURRENT_PATH_ID: &str = "scan-current-path";
/// Widget id of the estimated-progress bar (volumes only).
pub const SCAN_PROGRESS_ID: &str = "scan-progress";

/// The bar never claims to be done before the scan is: estimates can be off.
const MAX_ESTIMATED_FRACTION: f64 = 0.99;

/// Build the scan screen for `target`.
pub fn build(state: &AppState, target: &ScanTarget, fonts: &Fonts) -> Box<dyn Widget> {
    let cancel_state = state.clone();
    let cancel = Button::new("Cancel", Arc::clone(&fonts.ui))
        .with_icon(fa::TIMES, Arc::clone(&fonts.icons))
        .with_cancel_action()
        .on_click(move || cancel_state.cancel_scan())
        .with_name(CANCEL_BUTTON_ID);

    let title = FlexRow::new()
        .with_gap(14.0)
        .with_h_anchor(HAnchor::STRETCH)
        .add(Box::new(Spinner::new().with_v_anchor(VAnchor::CENTER)))
        .add_flex(
            Box::new(
                Label::new(
                    format!("Scanning {}", target.display_name()),
                    Arc::clone(&fonts.ui),
                )
                .with_font_size(22.0)
                .with_strong(true)
                .with_ellipsis_if_clipped(true)
                .with_v_anchor(VAnchor::CENTER),
            ),
            1.0,
        );

    let version_state = state.clone();
    let live_state = state.clone();
    let live_target = target.clone();
    let live_fonts = fonts.clone();
    let live = Rebuilder::new(
        move || version_state.progress_version(),
        move || build_live(&live_state, &live_target, &live_fonts),
    )
    .with_h_anchor(HAnchor::STRETCH);

    let mut column = FlexColumn::new()
        .with_panel_bg()
        .with_top_anchor(true)
        .with_gap(14.0)
        .with_inner_padding(Insets::all(32.0))
        .add(Box::new(title));
    if target.volume_name.is_some() {
        column = column.add(Box::new(
            Label::new(
                target.path.to_string_lossy().into_owned(),
                Arc::clone(&fonts.ui),
            )
            .with_font_size(13.0)
            .with_dim(true),
        ));
    }
    Box::new(
        column
            .add(Box::new(live))
            .add_flex(Box::new(Spacer::new()), 1.0)
            .add(Box::new(
                FlexRow::new()
                    .with_h_anchor(HAnchor::STRETCH)
                    .add(Box::new(cancel)),
            ))
            .with_name(SCAN_SCREEN_ID),
    )
}

/// The counters, bar and current path as of now.
fn build_live(state: &AppState, target: &ScanTarget, fonts: &Fonts) -> Box<dyn Widget> {
    let snapshot = state.progress().unwrap_or_default();
    let mut column = FlexColumn::new()
        .with_gap(14.0)
        .with_top_anchor(true)
        .with_h_anchor(HAnchor::STRETCH);
    if let Some(expected) = target.expected_bytes.filter(|&e| e > 0) {
        let fraction = (snapshot.bytes as f64 / expected as f64).min(MAX_ESTIMATED_FRACTION);
        column = column.add(Box::new(
            ProgressBar::new(fraction, Arc::clone(&fonts.ui))
                .with_h_anchor(HAnchor::STRETCH)
                .with_name(SCAN_PROGRESS_ID),
        ));
    }
    let counters = FlexRow::new()
        .with_gap(28.0)
        .with_h_anchor(HAnchor::STRETCH)
        .add(stat(&format_count(snapshot.files), "files", fonts))
        .add(stat(&format_count(snapshot.dirs), "folders", fonts))
        .add(stat(&format_bytes(snapshot.bytes), "on disk", fonts))
        .add(stat(&format_count(snapshot.errors), "unreadable", fonts))
        .add(stat(&format_elapsed(snapshot.elapsed), "elapsed", fonts))
        .with_name(SCAN_COUNTERS_ID);
    let current = if snapshot.current_path.is_empty() {
        "Starting\u{2026}".to_string()
    } else {
        snapshot.current_path.clone()
    };
    Box::new(
        column.add(Box::new(counters)).add(Box::new(
            Label::new(current, Arc::clone(&fonts.ui))
                .with_font_size(13.0)
                .with_dim(true)
                .with_ellipsis_if_clipped(true)
                .with_h_anchor(HAnchor::STRETCH)
                .with_name(CURRENT_PATH_ID),
        )),
    )
}

/// A big number over a small caption.
pub(super) fn stat(value: &str, caption: &str, fonts: &Fonts) -> Box<dyn Widget> {
    Box::new(
        FlexColumn::new()
            .with_gap(2.0)
            .with_fit_width(true)
            .with_top_anchor(true)
            .add(Box::new(
                Label::new(value, Arc::clone(&fonts.ui))
                    .with_font_size(22.0)
                    .with_strong(true),
            ))
            .add(Box::new(
                Label::new(caption, Arc::clone(&fonts.ui))
                    .with_font_size(12.0)
                    .with_dim(true),
            )),
    )
}
