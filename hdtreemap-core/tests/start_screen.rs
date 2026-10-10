//! Headless UI tests for the start screen (`hdtreemap_core::ui::start_screen`): volume
//! cards, the folder picker, the demo banner, and that clicking a card or picking a folder
//! starts a scan. The harness is in `common/mod.rs`.

mod common;

use std::path::PathBuf;
use std::rc::Rc;

use agg_gui::widget::find_widget_by_id;
use agg_gui::Size;
use common::{fake, scanning_state, volume, Harness, GB};
use hdtreemap_core::platform::DemoPlatform;
use hdtreemap_core::state::{ScanTarget, Screen};
use hdtreemap_core::ui::main_view::MAIN_VIEW_ID;
use hdtreemap_core::ui::scan_strip::{CANCEL_BUTTON_ID, SCAN_STRIP_ID};
use hdtreemap_core::ui::start_screen::{
    volume_card_id, CHOOSE_FOLDER_ID, DEMO_BANNER_ID, DEMO_NOTE_ID, NO_VOLUMES_ID, START_SCREEN_ID,
};

#[test]
fn native_start_screen_lists_volumes_and_offers_choose_folder() {
    let (platform, _) = fake(
        vec![
            volume("Macintosh HD", "/", 500 * GB, 120 * GB),
            volume("Backup", "/Volumes/Backup", 2000 * GB, 1900 * GB),
        ],
        None,
    );
    let h = Harness::new(Rc::new(platform));
    assert!(h.visible(START_SCREEN_ID));
    assert!(h.visible(&volume_card_id(0)));
    assert!(h.visible(&volume_card_id(1)));
    assert!(!h.visible(&volume_card_id(2)));
    assert!(h.visible(CHOOSE_FOLDER_ID));
    assert!(!h.visible(DEMO_BANNER_ID), "no demo banner natively");
    assert!(!h.visible(DEMO_NOTE_ID));
    assert!(!h.visible(NO_VOLUMES_ID));
}

#[test]
fn clicking_a_volume_card_scans_it_and_cancel_returns() {
    let (platform, _) = fake(
        vec![
            volume("Macintosh HD", "/", 500 * GB, 120 * GB),
            volume("Backup", "/Volumes/Backup", 2000 * GB, 1900 * GB),
        ],
        None,
    );
    let mut h = Harness::new(Rc::new(platform));
    h.click(&volume_card_id(1));
    let screen = h.state.screen();
    let (Screen::Scanning(target) | Screen::Results(target)) = screen else {
        panic!("clicking the card did not start a scan: {screen:?}");
    };
    assert_eq!(target.path, PathBuf::from("/Volumes/Backup"));
    assert_eq!(target.volume_name.as_deref(), Some("Backup"));
    assert!(h.visible(MAIN_VIEW_ID), "the map fills in while it scans");
    assert!(h.visible(SCAN_STRIP_ID));
    assert!(!h.visible(START_SCREEN_ID));

    h.click(CANCEL_BUTTON_ID);
    assert_eq!(h.state.screen(), Screen::Start);
    assert!(h.visible(START_SCREEN_ID));
}

#[test]
fn choose_folder_uses_the_picker() {
    let (platform, calls) = fake(vec![volume("Disk", "/", 10 * GB, 5 * GB)], Some("/picked"));
    let mut h = Harness::new(Rc::new(platform));
    h.click(CHOOSE_FOLDER_ID);
    assert_eq!(calls.get(), 1);
    assert_eq!(
        h.state.screen(),
        Screen::Scanning(ScanTarget::folder("/picked"))
    );
    assert!(h.visible(SCAN_STRIP_ID));
}

#[test]
fn a_cancelled_picker_stays_on_the_start_screen() {
    let (platform, calls) = fake(vec![volume("Disk", "/", 10 * GB, 5 * GB)], None);
    let mut h = Harness::new(Rc::new(platform));
    h.click(CHOOSE_FOLDER_ID);
    assert_eq!(calls.get(), 1);
    assert_eq!(h.state.screen(), Screen::Start);
    assert!(h.visible(START_SCREEN_ID));
}

#[test]
fn no_volumes_shows_a_hint() {
    let (platform, _) = fake(Vec::new(), None);
    let h = Harness::new(Rc::new(platform));
    assert!(h.visible(NO_VOLUMES_ID));
    assert!(h.visible(CHOOSE_FOLDER_ID));
}

#[test]
fn demo_start_screen_shows_simulated_drives_and_no_folder_picker() {
    let mut h = Harness::new(Rc::new(DemoPlatform::default()));
    assert!(h.visible(DEMO_BANNER_ID));
    assert!(h.visible(DEMO_NOTE_ID));
    assert!(
        !h.visible(CHOOSE_FOLDER_ID),
        "the browser cannot pick folders"
    );
    for i in 0..6 {
        assert!(
            find_widget_by_id(h.app.root(), &volume_card_id(i)).is_some(),
            "card {i}"
        );
    }
    h.click(&volume_card_id(0));
    let Screen::Scanning(target) = h.state.screen() else {
        panic!("clicking the card did not start a scan");
    };
    assert_eq!(target.volume_id.as_deref(), Some("laptop-ssd"));
    h.state.cancel_scan();
}

#[test]
fn a_command_line_path_opens_on_the_live_scan() {
    let (platform, _) = fake(vec![volume("Disk", "/", 10 * GB, 5 * GB)], None);
    let h = Harness::with_state(scanning_state("/from/cli"), Rc::new(platform));
    assert!(h.visible(SCAN_STRIP_ID));
    assert!(h.visible(MAIN_VIEW_ID));
    assert!(!h.visible(START_SCREEN_ID));
}

#[test]
fn small_windows_lay_out_without_panicking() {
    let mut h = Harness::new(Rc::new(DemoPlatform::default()));
    for (w, ht) in [(640.0, 480.0), (320.0, 240.0), (1.0, 1.0)] {
        h.app.layout(Size::new(w, ht));
    }
    h.frame();
}
