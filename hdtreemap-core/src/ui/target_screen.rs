//! The screen shown once a scan target is chosen: what will be scanned, and a Back button to
//! the start screen. Milestone M1 replaces it with the scan-progress screen.

use std::sync::Arc;

use agg_gui::{
    current_visuals, Button, FlexColumn, FlexRow, HAnchor, Insets, Label, Spacer, Widget,
};

use crate::fonts::{fa, Fonts};
use crate::state::{AppState, ScanTarget};

/// Widget id of the whole screen.
pub const TARGET_SCREEN_ID: &str = "target-screen";
/// Widget id of the label showing the chosen path.
pub const TARGET_PATH_ID: &str = "target-path";
/// Widget id of the Back button.
pub const BACK_BUTTON_ID: &str = "target-back";

/// Build the target screen for `target`.
pub fn build(state: &AppState, target: &ScanTarget, fonts: &Fonts) -> Box<dyn Widget> {
    let visuals = current_visuals();
    let back_state = state.clone();
    let back = Button::new("Back", Arc::clone(&fonts.ui))
        .with_icon(fa::ARROW_LEFT, Arc::clone(&fonts.icons))
        .with_cancel_action()
        .on_click(move || back_state.back_to_start())
        .with_name(BACK_BUTTON_ID);
    let heading = match &target.volume_name {
        Some(name) => format!("Selected drive: {name}"),
        None => "Selected folder".to_string(),
    };
    let path = target.path.to_string_lossy().into_owned();
    Box::new(
        FlexColumn::new()
            .with_panel_bg()
            .with_top_anchor(true)
            .with_gap(12.0)
            .with_inner_padding(Insets::all(32.0))
            .add(Box::new(
                FlexRow::new()
                    .with_h_anchor(HAnchor::STRETCH)
                    .add(Box::new(back)),
            ))
            .add(Box::new(
                Label::new(heading, Arc::clone(&fonts.ui))
                    .with_font_size(22.0)
                    .with_strong(true),
            ))
            .add(Box::new(
                Label::new(path, Arc::clone(&fonts.ui))
                    .with_font_size(16.0)
                    .with_color(visuals.text_color)
                    .with_name(TARGET_PATH_ID),
            ))
            .add_flex(Box::new(Spacer::new()), 1.0)
            .with_name(TARGET_SCREEN_ID),
    )
}
