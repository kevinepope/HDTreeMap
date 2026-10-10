//! Unit tests for `AppState`: scan lifecycle (start, pump, finish, cancel), size-mode
//! switching, the scan targets, and the treemap's view state (zoom, hover, selection,
//! shading and the versions the widgets watch).

use std::rc::Rc;
use std::time::Duration;

use super::*;
use crate::demo_fs::{DemoPace, DemoProfile, DemoScanner};
use crate::model::NodeId;
use crate::platform::{DemoPlatform, PlatformServices};
use crate::render::Shading;

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
    let (screen, tree) = (state.version(), state.tree_version());
    state.set_size_mode(SizeMode::Apparent);
    assert_eq!(state.size_mode(), SizeMode::Apparent);
    assert!(state.tree_version() > tree, "the map re-lays out");
    assert_eq!(state.version(), screen, "the screen is not rebuilt");
    let tree = state.tree_version();
    state.set_size_mode(SizeMode::Apparent);
    assert_eq!(state.tree_version(), tree, "no change, no re-layout");
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
    assert_eq!(target.free_bytes, Some(4));
    assert_eq!(folder.free_bytes, None);
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

fn finished_usb() -> AppState {
    let state = AppState::new();
    state.start_scan(usb_target(), usb_scan());
    while state.pump(Duration::from_secs(5)) {}
    state
}

/// The first subfolder of the scan root.
fn first_dir(state: &AppState) -> NodeId {
    state
        .with_result(|r, _| {
            let tree = &r.tree;
            tree.children(tree.root())
                .find(|&c| tree.node(c).is_some_and(|n| n.is_dir()))
        })
        .flatten()
        .expect("a folder")
}

#[test]
fn zooming_moves_the_root_and_back() {
    let state = finished_usb();
    assert_eq!(state.zoom_root(), NodeId(0));
    let dir = first_dir(&state);
    let map = state.map_version();
    state.set_hovered(Some(Hover {
        node: dir,
        aggregate: None,
    }));
    assert!(state.set_zoom_root(dir));
    assert_eq!(state.zoom_root(), dir);
    assert!(state.map_version() > map);
    assert_eq!(state.hovered(), None, "zooming clears the hover");
    assert!(!state.set_zoom_root(dir), "already there");
    assert!(!state.set_zoom_root(NodeId(u32::MAX - 1)), "unknown node");
    assert!(state.zoom_out());
    assert_eq!(state.zoom_root(), NodeId(0));
    assert!(!state.zoom_out(), "the scan root is as far out as it goes");
}

#[test]
fn files_cannot_be_zoom_roots() {
    let state = finished_usb();
    let file = state
        .with_result(|r, _| r.tree.largest_files(1)[0])
        .expect("a file");
    assert!(!state.set_zoom_root(file));
    assert_eq!(state.zoom_root(), NodeId(0));
}

#[test]
fn hover_and_selection_only_touch_the_detail_version() {
    let state = finished_usb();
    let dir = first_dir(&state);
    let (tree, map, detail) = (
        state.tree_version(),
        state.map_version(),
        state.detail_version(),
    );
    let hover = Some(Hover {
        node: dir,
        aggregate: None,
    });
    state.set_hovered(hover);
    assert_eq!(state.hovered(), hover);
    state.set_selected(Some(dir));
    assert_eq!(state.selected(), Some(dir));
    assert_eq!((state.tree_version(), state.map_version()), (tree, map));
    assert!(state.detail_version() >= detail + 2);
    let detail = state.detail_version();
    state.set_hovered(hover);
    state.set_selected(Some(dir));
    assert_eq!(state.detail_version(), detail, "no change, no bump");
    state.set_shading(Shading::Flat);
    assert_eq!(state.shading(), Shading::Flat);
    assert!(state.map_version() > map);
    assert_eq!(state.tree_version(), tree);
}

#[test]
fn a_new_result_resets_the_view() {
    let state = finished_usb();
    let dir = first_dir(&state);
    state.set_zoom_root(dir);
    state.set_selected(Some(dir));
    state.set_shading(Shading::Flat);
    let tree = state.tree_version();
    state.start_scan(usb_target(), usb_scan());
    while state.pump(Duration::from_secs(5)) {}
    assert_eq!(state.zoom_root(), NodeId(0));
    assert_eq!(state.selected(), None);
    assert_eq!(state.shading(), Shading::Flat, "shading is a preference");
    assert!(state.tree_version() > tree);
}

#[test]
fn the_extension_highlight_toggles_and_resets_with_a_new_scan() {
    let state = finished_usb();
    let top = state
        .with_result(|_, exts| exts.rows()[0].ext)
        .expect("an extension");
    let (map, detail) = (state.map_version(), state.detail_version());
    state.toggle_highlight_ext(top);
    assert_eq!(state.highlight_ext(), Some(top));
    assert_eq!(state.map_version(), map, "a highlight never re-lays out");
    assert!(state.detail_version() > detail);
    state.toggle_highlight_ext(top);
    assert_eq!(state.highlight_ext(), None, "a second toggle clears it");
    state.set_highlight_ext(Some(u16::MAX));
    assert_eq!(state.highlight_ext(), None, "unknown ids are ignored");
    state.set_highlight_ext(Some(top));
    state.start_scan(usb_target(), usb_scan());
    while state.pump(Duration::from_secs(5)) {}
    assert_eq!(
        state.highlight_ext(),
        None,
        "a new scan starts unhighlighted"
    );

    // A highlight asked for before the scan applies when it finishes.
    let name = state
        .with_result(|_, exts| exts.rows()[1].name.clone())
        .expect("a second extension");
    state.highlight_on_finish(Some(name.clone()));
    state.start_scan(usb_target(), usb_scan());
    while state.pump(Duration::from_secs(5)) {}
    let highlighted = state
        .highlight_ext()
        .and_then(|e| state.with_result(|r, _| r.tree.ext_name(e).to_string()));
    assert_eq!(highlighted, Some(name));
}

#[test]
fn the_palette_is_cached_until_the_tree_changes() {
    let state = finished_usb();
    let a = state.palette().expect("palette");
    let b = state.palette().expect("palette");
    assert!(Rc::ptr_eq(&a, &b), "one palette per tree version");
    state.set_size_mode(SizeMode::Apparent);
    let c = state.palette().expect("palette");
    assert!(!Rc::ptr_eq(&a, &c), "a size-mode switch rebuilds it");
    assert!(AppState::new().palette().is_none());
}

#[test]
fn zoom_extensions_follow_the_zoom_root() {
    let state = finished_usb();
    let whole = state.zoom_extensions().expect("table");
    let scan_rows = state
        .with_result(|_, exts| exts.rows().to_vec())
        .expect("rows");
    assert_eq!(whole.rows(), &scan_rows[..]);
    assert!(Rc::ptr_eq(&whole, &state.zoom_extensions().expect("table")));
    let dir = first_dir(&state);
    state.set_zoom_root(dir);
    let zoomed = state.zoom_extensions().expect("table");
    let expected = state
        .with_result(|r, _| ExtTable::from_subtree(&r.tree, dir).rows().to_vec())
        .expect("rows");
    assert_eq!(zoomed.rows(), &expected[..]);
    let files: u64 = zoomed.rows().iter().map(|r| r.count).sum();
    let dir_files = state
        .with_result(|r, _| r.tree.node(dir).map(|n| u64::from(n.file_count())))
        .flatten();
    assert_eq!(Some(files), dir_files);
}

#[test]
fn splitter_ratios_survive_rebuilds_of_the_view() {
    let state = finished_usb();
    assert_eq!(state.side_split().get(), SIDE_SPLIT_DEFAULT);
    assert_eq!(state.tree_split().get(), TREE_SPLIT_DEFAULT);
    // A divider drag writes the shared handle; a later view reads the same value.
    state.side_split().set(0.4);
    assert_eq!(state.side_split().get(), 0.4);
    state.start_scan(usb_target(), usb_scan());
    while state.pump(Duration::from_secs(5)) {}
    assert_eq!(state.side_split().get(), 0.4, "kept for the session");
}
