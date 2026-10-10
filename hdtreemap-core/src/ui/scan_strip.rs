//! The scan strip: a compact bar across the top of the results view while a scan runs and
//! the map fills in below it (the live map, `state/live.rs`). It shows a spinner, what is
//! being scanned, the live counters (files, folders, bytes on disk, unreadable, elapsed),
//! a progress bar when the expected size is known (a volume's used space; blended with
//! the entry count when the source estimates one, `ProgressSnapshot::fraction`), the folder
//! being read, and Cancel (back to the start screen).
//!
//! [`build`] returns the strip while a scan runs and nothing once it is done, keyed on
//! `AppState::is_scanning`, so the strip goes away when the scan finishes without the rest
//! of the view being rebuilt. The counters, bar and path sit in their own `Rebuilder` keyed
//! on `AppState::progress_version` (ticked at most ten times a second by the scan pump), so
//! they refresh without restarting the spinner.

use std::sync::Arc;

use agg_gui::{
    current_visuals, Button, Container, EllipsisMode, FlexColumn, FlexRow, HAnchor, Insets, Label,
    ProgressBar, Rebuilder, Size, Spinner, VAnchor, Widget,
};

use crate::fonts::{fa, Fonts};
use crate::format::{format_bytes, format_count, format_elapsed};
use crate::state::{AppState, ScanTarget};

/// Widget id of the strip.
pub const SCAN_STRIP_ID: &str = "scan-strip";
/// Widget id of the Cancel button.
pub const CANCEL_BUTTON_ID: &str = "scan-cancel";
/// Widget id of the live counters.
pub const SCAN_COUNTERS_ID: &str = "scan-counters";
/// Widget id of the label showing the folder being read.
pub const CURRENT_PATH_ID: &str = "scan-current-path";
/// Widget id of the estimated-progress bar (volumes only).
pub const SCAN_PROGRESS_ID: &str = "scan-progress";

/// The bar never claims to be done before the scan is: estimates can be off.
const MAX_ESTIMATED_FRACTION: f64 = 0.99;
/// Width of the progress bar (logical pixels).
const BAR_WIDTH: f64 = 180.0;
/// Text size of the counters and path.
const FONT_SIZE: f64 = 13.0;

/// The strip while a scan runs, nothing otherwise.
pub fn build(state: &AppState, target: &ScanTarget, fonts: &Fonts) -> Box<dyn Widget> {
    let key_state = state.clone();
    let strip_state = state.clone();
    let strip_target = target.clone();
    let strip_fonts = fonts.clone();
    Box::new(
        Rebuilder::new(
            // Rebuilt when the scan ends, and with a new look (its tint is the theme's).
            move || u64::from(key_state.is_scanning()) | key_state.theme_version() << 1,
            move || {
                if strip_state.is_scanning() {
                    strip(&strip_state, &strip_target, &strip_fonts)
                } else {
                    // Nothing: an empty fit-height container takes no height.
                    Box::new(Container::new().with_fit_height(true))
                }
            },
        )
        .with_h_anchor(HAnchor::STRETCH),
    )
}

fn strip(state: &AppState, target: &ScanTarget, fonts: &Fonts) -> Box<dyn Widget> {
    let cancel_state = state.clone();
    let cancel = Button::new("Cancel", Arc::clone(&fonts.ui))
        .with_icon(fa::TIMES, Arc::clone(&fonts.icons))
        .with_cancel_action()
        .with_focus_on_click(false)
        .on_click(move || cancel_state.cancel_scan())
        .with_v_anchor(VAnchor::CENTER)
        .with_name(CANCEL_BUTTON_ID);

    let title = Label::new(
        format!("Scanning {}", target.display_name()),
        Arc::clone(&fonts.ui),
    )
    .with_font_size(15.0)
    .with_strong(true)
    .with_v_anchor(VAnchor::CENTER);

    let version_state = state.clone();
    let live_state = state.clone();
    let live_target = target.clone();
    let live_fonts = fonts.clone();
    let live = Rebuilder::new(
        move || version_state.progress_version(),
        move || build_live(&live_state, &live_target, &live_fonts),
    )
    .with_h_anchor(HAnchor::STRETCH)
    .with_v_anchor(VAnchor::CENTER);

    let row = FlexRow::new()
        .with_gap(12.0)
        .with_h_anchor(HAnchor::STRETCH)
        .add(Box::new(Spinner::small().with_v_anchor(VAnchor::CENTER)))
        .add(Box::new(title))
        .add_flex(Box::new(live), 1.0)
        .add(Box::new(cancel));
    let visuals = current_visuals();
    Box::new(
        Container::new()
            .with_fit_height(true)
            .with_background(visuals.accent.with_alpha(0.12))
            .with_inner_padding(Insets::symmetric(12.0, 6.0))
            .with_h_anchor(HAnchor::STRETCH)
            .add(Box::new(row))
            .with_name(SCAN_STRIP_ID),
    )
}

/// `"12,345 files · 678 folders · 4.5 GB · 3 unreadable · 0:12"`.
pub fn counters_text(state: &AppState) -> String {
    let s = state.progress().unwrap_or_default();
    let mut text = format!(
        "{} files \u{b7} {} folders \u{b7} {}",
        format_count(s.files),
        format_count(s.dirs),
        format_bytes(s.bytes)
    );
    if s.errors > 0 {
        text.push_str(&format!(" \u{b7} {} unreadable", format_count(s.errors)));
    }
    text.push_str(&format!(" \u{b7} {}", format_elapsed(s.elapsed)));
    text
}

/// The counters, bar and current path as of now.
fn build_live(state: &AppState, target: &ScanTarget, fonts: &Fonts) -> Box<dyn Widget> {
    let snapshot = state.progress().unwrap_or_default();
    let counters = Label::new(counters_text(state), Arc::clone(&fonts.ui))
        .with_font_size(FONT_SIZE)
        .with_v_anchor(VAnchor::CENTER)
        .with_name(SCAN_COUNTERS_ID);
    let current = if snapshot.current_path.is_empty() {
        "Starting\u{2026}".to_string()
    } else {
        snapshot.current_path.clone()
    };
    let path = Label::new(current, Arc::clone(&fonts.ui))
        .with_font_size(12.0)
        .with_dim(true)
        .with_ellipsis_mode(EllipsisMode::PathMiddle)
        .with_h_anchor(HAnchor::STRETCH)
        .with_name(CURRENT_PATH_ID);
    let mut top = FlexRow::new()
        .with_gap(12.0)
        .with_h_anchor(HAnchor::STRETCH)
        .add(Box::new(counters));
    if let Some(fraction) = target
        .expected_bytes
        .and_then(|expected| snapshot.fraction(expected))
    {
        let fraction = fraction.min(MAX_ESTIMATED_FRACTION);
        top = top.add(Box::new(
            ProgressBar::new(fraction, Arc::clone(&fonts.ui))
                .with_min_size(Size::new(BAR_WIDTH, 0.0))
                .with_max_size(Size::new(BAR_WIDTH, f64::MAX))
                .with_v_anchor(VAnchor::CENTER)
                .with_name(SCAN_PROGRESS_ID),
        ));
    }
    Box::new(
        FlexColumn::new()
            .with_gap(2.0)
            .with_top_anchor(true)
            .with_h_anchor(HAnchor::STRETCH)
            .add(Box::new(top))
            .add(Box::new(path)),
    )
}
