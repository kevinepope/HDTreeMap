//! Headless UI test of the whole M1 flow on the demo platform: start screen → click a drive
//! → scan screen with live counters → results screen (totals, largest folders, top file
//! types, size-mode switch) → Rescan → Back to the start screen. The harness is in
//! `common/mod.rs`.

mod common;

use std::rc::Rc;
use std::time::Duration;

use common::Harness;
use hdtreemap_core::model::SizeMode;
use hdtreemap_core::platform::{profile, DemoPlatform, PlatformServices};
use hdtreemap_core::state::Screen;
use hdtreemap_core::ui::results_screen::{
    ERRORS_ID, LARGEST_FOLDERS_ID, RESCAN_ID, RESULTS_BACK_ID, RESULTS_SCREEN_ID, SIZE_MODE_ID,
    TOP_EXTENSIONS_ID,
};
use hdtreemap_core::ui::scan_screen::{
    CURRENT_PATH_ID, SCAN_COUNTERS_ID, SCAN_PROGRESS_ID, SCAN_SCREEN_ID,
};
use hdtreemap_core::ui::start_screen::{volume_card_id, START_SCREEN_ID};

/// Frames to wait for a 1.5 s paced scan (each frame sleeps a little, like a display).
const MAX_FRAMES: usize = 2000;

fn wait_for_results(h: &mut Harness) {
    let done = h.frames_until(MAX_FRAMES, |h| {
        std::thread::sleep(Duration::from_millis(5));
        matches!(h.state.screen(), Screen::Results(_))
    });
    assert!(done, "the scan never finished: {:?}", h.state.screen());
}

#[test]
fn start_scan_results_rescan_and_back() {
    let platform = DemoPlatform::default();
    let usb_index = platform
        .list_volumes()
        .iter()
        .position(|v| v.id == profile::USB_STICK)
        .expect("usb stick listed");
    let mut h = Harness::new(Rc::new(platform));
    assert!(h.visible(START_SCREEN_ID));

    h.click(&volume_card_id(usb_index));
    assert!(matches!(h.state.screen(), Screen::Scanning(_)));
    assert!(h.visible(SCAN_SCREEN_ID));
    assert!(h.visible(SCAN_COUNTERS_ID));
    assert!(h.visible(CURRENT_PATH_ID));
    assert!(h.visible(SCAN_PROGRESS_ID), "a drive has an expected size");

    // Counters move while the scan runs.
    let moved = h.frames_until(MAX_FRAMES, |h| {
        std::thread::sleep(Duration::from_millis(5));
        h.state.progress().is_some_and(|p| p.files > 0) || !h.state.is_scanning()
    });
    assert!(moved);

    wait_for_results(&mut h);
    assert!(h.visible(RESULTS_SCREEN_ID));
    assert!(!h.visible(SCAN_SCREEN_ID));
    assert!(h.visible(LARGEST_FOLDERS_ID));
    assert!(h.visible(TOP_EXTENSIONS_ID));
    assert!(
        h.visible(ERRORS_ID),
        "the stick has an unreadable system folder"
    );
    assert!(h.visible(SIZE_MODE_ID));

    // The size-mode switch re-sorts by apparent size: click its right half.
    let rect = agg_gui::widget::find_widget_screen_rect(h.app.root(), SIZE_MODE_ID)
        .expect("size mode switch");
    let x = rect.x + rect.width * 0.75;
    let y = common::HEIGHT - (rect.y + rect.height * 0.5);
    h.click_at(x, y);
    assert_eq!(h.state.size_mode(), SizeMode::Apparent);
    assert!(h.visible(RESULTS_SCREEN_ID));

    h.click(RESCAN_ID);
    assert!(matches!(h.state.screen(), Screen::Scanning(_)));
    wait_for_results(&mut h);

    h.click(RESULTS_BACK_ID);
    assert_eq!(h.state.screen(), Screen::Start);
    assert!(h.visible(START_SCREEN_ID));
}

#[test]
fn the_results_screen_survives_small_windows() {
    let mut h = Harness::new(Rc::new(DemoPlatform::unpaced()));
    h.click(&volume_card_id(3));
    wait_for_results(&mut h);
    for (w, ht) in [(640.0, 480.0), (320.0, 240.0), (1.0, 1.0)] {
        h.app.layout(agg_gui::Size::new(w, ht));
    }
    h.frame();
}
