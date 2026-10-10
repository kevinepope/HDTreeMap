//! Unit tests for `AppState`: scan lifecycle (start, pump, finish, cancel), size-mode
//! switching and the scan targets.

use std::time::Duration;

use super::*;
use crate::demo_fs::{DemoPace, DemoProfile, DemoScanner};
use crate::platform::{DemoPlatform, PlatformServices};

fn usb_target() -> ScanTarget {
    let volume = DemoPlatform::default()
        .list_volumes()
        .into_iter()
        .find(|v| v.id == DemoProfile::UsbStick.id())
        .expect("usb stick");
    ScanTarget::volume(&volume)
}

fn usb_scan() -> Box<dyn ScanSource> {
    Box::new(DemoScanner::new(DemoProfile::UsbStick, DemoPace::Unpaced))
}

#[test]
fn a_scan_runs_to_the_results_screen() {
    let state = AppState::new();
    assert_eq!(state.screen(), Screen::Start);
    let v0 = state.version();
    state.start_scan(usb_target(), usb_scan());
    assert_eq!(state.screen(), Screen::Scanning(usb_target()));
    assert!(state.version() > v0);
    assert!(state.is_scanning());
    assert!(state.progress().is_some());

    let mut polls = 0;
    while state.pump(Duration::from_millis(50)) {
        polls += 1;
        assert!(polls < 10_000, "the scan never finished");
    }
    assert_eq!(state.screen(), Screen::Results(usb_target()));
    assert!(!state.is_scanning());
    assert!(state.progress().is_none());
    let files = state.with_result(|r, ext| {
        assert!(!ext.rows().is_empty());
        r.tree.file_count()
    });
    assert!(files.is_some_and(|n| n > 100));
    // Nothing left to pump.
    assert!(!state.pump(Duration::from_millis(1)));
}

#[test]
fn cancelling_returns_to_start_and_stops_the_source() {
    let state = AppState::new();
    let source = DemoScanner::new(DemoProfile::ArchiveServer, DemoPace::Realistic);
    let progress = source.progress();
    state.start_scan(usb_target(), Box::new(source));
    assert!(state.pump(Duration::from_millis(1)));
    state.cancel_scan();
    assert_eq!(state.screen(), Screen::Start);
    assert!(!state.is_scanning());
    assert!(progress.is_cancelled());
    assert!(state.with_result(|_, _| ()).is_none());
}

#[test]
fn progress_version_ticks_while_scanning() {
    let state = AppState::new();
    state.start_scan(
        usb_target(),
        Box::new(DemoScanner::new(
            DemoProfile::ArchiveServer,
            DemoPace::Realistic,
        )),
    );
    let v0 = state.progress_version();
    assert!(state.pump(Duration::from_millis(1)));
    assert!(
        state.progress_version() > v0,
        "the first poll refreshes the counters"
    );
    let v1 = state.progress_version();
    // Within the refresh interval nothing changes.
    assert!(state.pump(Duration::from_millis(1)));
    assert_eq!(state.progress_version(), v1);
    state.back_to_start();
}

#[test]
fn switching_the_size_mode_resorts_the_result() {
    let state = AppState::new();
    state.start_scan(usb_target(), usb_scan());
    while state.pump(Duration::from_secs(5)) {}
    assert_eq!(state.size_mode(), SizeMode::Allocated);
    let v = state.version();
    state.set_size_mode(SizeMode::Apparent);
    assert_eq!(state.size_mode(), SizeMode::Apparent);
    assert!(state.version() > v);
    let v = state.version();
    state.set_size_mode(SizeMode::Apparent);
    assert_eq!(state.version(), v, "no change, no rebuild");
    state.back_to_start();
    assert!(state.with_result(|_, _| ()).is_none());
}

#[test]
fn starting_a_new_scan_cancels_the_old_one() {
    let state = AppState::new();
    let first = DemoScanner::new(DemoProfile::ArchiveServer, DemoPace::Realistic);
    let first_progress = first.progress();
    state.start_scan(usb_target(), Box::new(first));
    state.start_scan(ScanTarget::folder("/other"), usb_scan());
    assert!(first_progress.is_cancelled());
    assert_eq!(
        state.screen(),
        Screen::Scanning(ScanTarget::folder("/other"))
    );
}

#[test]
fn volume_targets_carry_the_volume_identity() {
    let volume = VolumeInfo {
        id: "usb-stick".into(),
        name: "Small USB Stick".into(),
        mount_path: PathBuf::from("/demo/usb-stick"),
        total_bytes: 10,
        free_bytes: 4,
        description: None,
    };
    let target = ScanTarget::volume(&volume);
    assert_eq!(target.path, PathBuf::from("/demo/usb-stick"));
    assert_eq!(target.volume_id.as_deref(), Some("usb-stick"));
    assert_eq!(target.display_name(), "Small USB Stick");
    assert_eq!(target.expected_bytes, Some(6));
    let folder = ScanTarget::folder("/home/me");
    assert_eq!(folder.display_name(), "/home/me");
    assert_eq!(folder.expected_bytes, None);
}

#[test]
fn the_demo_platform_scans_the_chosen_drive() {
    let state = AppState::new();
    let platform = DemoPlatform::unpaced();
    let target = usb_target();
    state.start_scan(target.clone(), platform.scan_source(&target));
    while state.pump(Duration::from_secs(5)) {}
    let root = state.with_result(|r, _| r.root.clone());
    assert_eq!(root, Some(target.path));
}
