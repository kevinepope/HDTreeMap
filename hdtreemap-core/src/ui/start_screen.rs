//! The start screen: app title, one card per mounted volume (name, optional description,
//! mount path, used/free bar, used/total text and a Scan button), and below the list either
//! "Choose Folder…" (native) or, on the browser demo, a note that the drives are simulated
//! plus a banner at the top.
//!
//! Picking a volume or a folder records a `ScanTarget` in `AppState`; the root `Rebuilder`
//! (`ui/mod.rs`) then shows the target screen.

use std::rc::Rc;
use std::sync::Arc;

use agg_gui::{
    current_visuals, Button, Color, Container, FlexColumn, FlexRow, HAnchor, Insets, Label,
    ProgressBar, ScrollView, Size, Spacer, VAnchor, Widget,
};

use crate::fonts::{fa, Fonts};
use crate::format::{format_bytes, used_fraction};
use crate::platform::{PlatformServices, VolumeInfo};
use crate::state::{AppState, ScanTarget};
use crate::APP_TITLE;

/// Widget id of the whole screen.
pub const START_SCREEN_ID: &str = "start-screen";
/// Widget id of the "Choose Folder…" button (native only).
pub const CHOOSE_FOLDER_ID: &str = "choose-folder";
/// Widget id of the browser-demo banner.
pub const DEMO_BANNER_ID: &str = "demo-banner";
/// Widget id of the browser-demo note that replaces "Choose Folder…".
pub const DEMO_NOTE_ID: &str = "demo-note";
/// Widget id of the label shown when no volume could be listed.
pub const NO_VOLUMES_ID: &str = "no-volumes";

/// Text of the browser-demo note that replaces "Choose Folder…".
pub const DEMO_NOTE_TEXT: &str = "Browser demo: pick a simulated drive.";
/// Text of the browser-demo banner.
pub const DEMO_BANNER_TEXT: &str =
    "Browser demo. These drives are simulated; nothing on your computer is read.";

/// Used fraction above which a volume's bar turns red.
const NEARLY_FULL: f64 = 0.9;

/// Widget id of the Scan button on the `index`-th volume card.
pub fn volume_scan_id(index: usize) -> String {
    format!("volume-scan-{index}")
}

/// Widget id of the `index`-th volume card.
pub fn volume_card_id(index: usize) -> String {
    format!("volume-card-{index}")
}

/// "120.0 GB used of 512.0 GB · 392.0 GB free".
pub fn usage_text(volume: &VolumeInfo) -> String {
    format!(
        "{} used of {} \u{b7} {} free",
        format_bytes(volume.used_bytes()),
        format_bytes(volume.total_bytes),
        format_bytes(volume.free_bytes)
    )
}

/// Build the start screen, listing the platform's volumes as of now.
pub fn build(
    state: &AppState,
    platform: &Rc<dyn PlatformServices>,
    fonts: &Fonts,
) -> Box<dyn Widget> {
    let demo = platform.is_demo();
    let volumes = platform.list_volumes();
    let mut column = FlexColumn::new()
        .with_panel_bg()
        .with_top_anchor(true)
        .with_gap(14.0)
        .with_inner_padding(Insets::all(28.0))
        .add(build_header(fonts));
    if demo {
        column = column.add(build_demo_banner(fonts));
    }
    column = column
        .add(Box::new(
            Label::new("Drives", Arc::clone(&fonts.ui))
                .with_font_size(16.0)
                .with_strong(true),
        ))
        .add_flex(build_volume_list(state, &volumes, fonts), 1.0)
        .add(build_footer(state, platform, demo, fonts));
    Box::new(column.with_name(START_SCREEN_ID))
}

fn build_header(fonts: &Fonts) -> Box<dyn Widget> {
    let visuals = current_visuals();
    let titles = FlexColumn::new()
        .with_gap(2.0)
        .with_fit_width(true)
        .with_top_anchor(true)
        .add(Box::new(
            Label::new(APP_TITLE, Arc::clone(&fonts.ui))
                .with_font_size(28.0)
                .with_strong(true),
        ))
        .add(Box::new(
            Label::new(
                "Choose a drive or folder to see what is using the space.",
                Arc::clone(&fonts.ui),
            )
            .with_font_size(14.0)
            .with_dim(true),
        ));
    Box::new(
        FlexRow::new()
            .with_gap(14.0)
            .with_h_anchor(HAnchor::STRETCH)
            .add(Box::new(
                Label::new(fa::TH_LARGE.to_string(), Arc::clone(&fonts.ui))
                    .with_font_size(40.0)
                    .with_color(visuals.accent)
                    .with_v_anchor(VAnchor::CENTER),
            ))
            .add(Box::new(titles.with_v_anchor(VAnchor::CENTER))),
    )
}

fn build_demo_banner(fonts: &Fonts) -> Box<dyn Widget> {
    let visuals = current_visuals();
    Box::new(
        Container::new()
            .with_fit_height(true)
            .with_background(visuals.accent.with_alpha(0.18))
            .with_border(visuals.accent, 1.0)
            .with_corner_radius(6.0)
            .with_inner_padding(Insets::symmetric(12.0, 8.0))
            .with_h_anchor(HAnchor::STRETCH)
            .add(Box::new(
                Label::new(
                    format!("{}  {}", fa::INFO_CIRCLE, DEMO_BANNER_TEXT),
                    Arc::clone(&fonts.ui),
                )
                .with_font_size(14.0),
            ))
            .with_name(DEMO_BANNER_ID),
    )
}

fn build_volume_list(state: &AppState, volumes: &[VolumeInfo], fonts: &Fonts) -> Box<dyn Widget> {
    let mut list = FlexColumn::new().with_gap(10.0);
    if volumes.is_empty() {
        list = list.add(Box::new(
            Label::new(
                "No drives could be listed. Use Choose Folder\u{2026} instead.",
                Arc::clone(&fonts.ui),
            )
            .with_dim(true)
            .with_name(NO_VOLUMES_ID),
        ));
    }
    for (index, volume) in volumes.iter().enumerate() {
        list = list.add(build_volume_card(state, index, volume, fonts));
    }
    Box::new(ScrollView::new(Box::new(list)).with_h_anchor(HAnchor::STRETCH))
}

fn build_volume_card(
    state: &AppState,
    index: usize,
    volume: &VolumeInfo,
    fonts: &Fonts,
) -> Box<dyn Widget> {
    let visuals = current_visuals();
    let fraction = used_fraction(volume.total_bytes, volume.free_bytes);
    let mut details = FlexColumn::new()
        .with_gap(4.0)
        .with_top_anchor(true)
        .with_h_anchor(HAnchor::STRETCH)
        .add(Box::new(
            Label::new(volume.name.clone(), Arc::clone(&fonts.ui))
                .with_font_size(16.0)
                .with_strong(true),
        ));
    if let Some(description) = &volume.description {
        details = details.add(Box::new(
            Label::new(description.clone(), Arc::clone(&fonts.ui)).with_font_size(13.0),
        ));
    }
    let mut bar = ProgressBar::new(fraction, Arc::clone(&fonts.ui)).with_h_anchor(HAnchor::STRETCH);
    if fraction >= NEARLY_FULL {
        bar = bar.with_fill_color(Color::rgb(0.86, 0.30, 0.27));
    }
    details = details
        .add(Box::new(
            Label::new(
                volume.mount_path.to_string_lossy().into_owned(),
                Arc::clone(&fonts.ui),
            )
            .with_font_size(12.0)
            .with_dim(true),
        ))
        .add(Box::new(bar))
        .add(Box::new(
            Label::new(usage_text(volume), Arc::clone(&fonts.ui)).with_font_size(13.0),
        ));

    let target = ScanTarget::volume(volume);
    let scan_state = state.clone();
    let scan = Button::new("Scan", Arc::clone(&fonts.ui))
        .with_icon(fa::SEARCH, Arc::clone(&fonts.icons))
        .with_v_anchor(VAnchor::CENTER)
        .on_click(move || scan_state.choose(target.clone()))
        .with_name(volume_scan_id(index));

    // The card is a FlexRow with a background rather than a rounded `Container`: inside a
    // `ScrollView` a fit-height `Container` is measured against an effectively infinite
    // height and loses its content height to float precision (an agg-gui issue).
    Box::new(
        FlexRow::new()
            .with_gap(16.0)
            .with_background(visuals.widget_bg)
            .with_inner_padding(Insets::symmetric(16.0, 12.0))
            .with_h_anchor(HAnchor::STRETCH)
            .add(Box::new(
                Label::new(fa::HDD.to_string(), Arc::clone(&fonts.ui))
                    .with_font_size(30.0)
                    .with_dim(true)
                    .with_v_anchor(VAnchor::CENTER),
            ))
            .add_flex(Box::new(details), 1.0)
            .add(Box::new(scan))
            .with_name(volume_card_id(index)),
    )
}

fn build_footer(
    state: &AppState,
    platform: &Rc<dyn PlatformServices>,
    demo: bool,
    fonts: &Fonts,
) -> Box<dyn Widget> {
    let row = FlexRow::new()
        .with_gap(12.0)
        .with_h_anchor(HAnchor::STRETCH);
    let row = if demo {
        row.add(Box::new(
            Label::new(DEMO_NOTE_TEXT, Arc::clone(&fonts.ui))
                .with_font_size(14.0)
                .with_dim(true)
                .with_v_anchor(VAnchor::CENTER)
                .with_name(DEMO_NOTE_ID),
        ))
    } else {
        let pick_state = state.clone();
        let pick_platform = Rc::clone(platform);
        let choose = Button::new("Choose Folder\u{2026}", Arc::clone(&fonts.ui))
            .with_icon(fa::FOLDER_OPEN, Arc::clone(&fonts.icons))
            .with_default_action()
            .on_click(move || {
                if let Some(path) = pick_platform.pick_folder() {
                    pick_state.choose(ScanTarget::folder(path));
                }
            })
            .with_name(CHOOSE_FOLDER_ID);
        row.add(Box::new(choose))
    };
    Box::new(row.add_flex(
        Box::new(Spacer::new().with_max_size(Size::new(f64::MAX, 0.0))),
        1.0,
    ))
}
