//! Headless UI test of the whole flow on the demo platform: start screen → click a drive →
//! scan screen with live counters → straight to the treemap view (toolbar, map, status bar
//! with totals and the unreadable-items note) → size-mode switch → Rescan → back to the
//! drive list. The harness is in `common/mod.rs`; the treemap's own interactions are tested
//! in `treemap_view.rs`.

mod common;

use std::rc::Rc;
use std::time::Duration;

use common::Harness;
use hdtreemap_core::model::SizeMode;
use hdtreemap_core::platform::{profile, DemoPlatform, PlatformServices};
use hdtreemap_core::state::Screen;
use hdtreemap_core::ui::breadcrumb::BREADCRUMB_ID;
use hdtreemap_core::ui::main_view::{
    BACK_ID, MAIN_VIEW_ID, RESCAN_ID, SIZE_MODE_ID, STATUS_ERRORS_ID, STATUS_FREE_ID,
    STATUS_ITEM_ID, STATUS_TOTALS_ID,
};
use hdtreemap_core::ui::scan_screen::{
    CURRENT_PATH_ID, SCAN_COUNTERS_ID, SCAN_PROGRESS_ID, SCAN_SCREEN_ID,
};
use hdtreemap_core::ui::start_screen::{volume_card_id, START_SCREEN_ID};
use hdtreemap_core::ui::treemap_view::TREEMAP_ID;

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
fn start_scan_treemap_rescan_and_back() {
    let platform = DemoPlatform::default();
    let usb_index = platform
        .list_volumes()
        .iter()
        .position(|v| v.id == profile::USB_STICK)
        .expect("usb stick listed");
    let mut h = Harness::new(Rc::new(platform));
    assert!(h.visible(START_SCREEN_ID));
    // Tall enough that the USB stick's card is on screen below the menu bar.
    h.resize(1200.0, 1000.0);

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

    // A finished scan goes straight to the treemap.
    wait_for_results(&mut h);
    assert!(h.visible(MAIN_VIEW_ID));
    assert!(!h.visible(SCAN_SCREEN_ID));
    assert!(h.visible(TREEMAP_ID));
    assert!(h.visible(BREADCRUMB_ID));
    assert!(h.state.map_counters().rasters >= 1, "the map was drawn");
    let totals = h.label_text(STATUS_TOTALS_ID);
    assert!(
        totals.contains("files") && totals.contains("folders"),
        "{totals}"
    );
    assert!(
        h.label_text(STATUS_ERRORS_ID).contains("unreadable"),
        "the stick has an unreadable system folder"
    );
    assert!(
        !h.label_text(STATUS_ERRORS_ID).contains("Full Disk Access"),
        "no macOS hint on the demo"
    );
    assert!(h.label_text(STATUS_FREE_ID).ends_with("free"));
    assert!(h.visible(STATUS_ITEM_ID));

    // The size-mode switch re-sorts by apparent size: click its right half.
    let (x, y) = h.point_in(SIZE_MODE_ID, 0.75, 0.5);
    h.click_at(x, y);
    assert_eq!(h.state.size_mode(), SizeMode::Apparent);
    assert!(h.visible(MAIN_VIEW_ID));

    h.click(RESCAN_ID);
    assert!(matches!(h.state.screen(), Screen::Scanning(_)));
    wait_for_results(&mut h);

    h.click(BACK_ID);
    assert_eq!(h.state.screen(), Screen::Start);
    assert!(h.visible(START_SCREEN_ID));
}

#[test]
fn the_main_view_survives_small_windows() {
    let mut h = Harness::new(Rc::new(DemoPlatform::unpaced()));
    // Tall enough that the fourth card is on screen below the menu bar.
    h.resize(1200.0, 1000.0);
    h.click(&volume_card_id(3));
    wait_for_results(&mut h);
    for (w, ht) in [(640.0, 480.0), (320.0, 240.0), (40.0, 30.0), (1.0, 1.0)] {
        h.resize(w, ht);
    }
    h.app.layout(agg_gui::Size::new(0.0, 0.0));
    h.resize(800.0, 600.0);
    assert!(h.visible(TREEMAP_ID));
}

#[test]
fn the_results_view_never_lists_volumes_itself() {
    // Listing volumes can hang on a dead network mount, so the results view only reads
    // a list the start screen or a background lookup already made.
    let (platform, _) = common::fake(Vec::new(), None);
    let calls = Rc::clone(&platform.calls);
    let state = common::scanning_state("/somewhere");
    let mut h = Harness::with_state(state, Rc::new(platform));
    wait_for_results(&mut h);
    assert!(
        !calls.borrow().iter().any(|c| c == "list_volumes"),
        "{:?}",
        calls.borrow()
    );
    assert!(!h.visible(STATUS_FREE_ID), "no free space known yet");

    // A background lookup delivers the list; the status bar picks it up.
    let (tx, rx) = std::sync::mpsc::channel();
    h.state.receive_volumes(rx);
    tx.send(vec![common::volume(
        "Demo",
        "/demo",
        100 * common::GB,
        7 * common::GB,
    )])
    .expect("send");
    h.frame();
    assert_eq!(h.label_text(STATUS_FREE_ID), "7.0 GB free");
}
