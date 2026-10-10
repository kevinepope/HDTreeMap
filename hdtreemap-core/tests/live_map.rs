//! Headless tests of the live map (`state/live.rs`): while a scan runs the results view
//! shows the partial tree and refreshes it on live ticks (the tree version bumps, the map
//! re-rasterises, the legend fills); hover, tooltips and zoom work on it, while Rescan and
//! Move to Trash wait; the finished tree is exactly what a scan run straight to the end
//! makes, the view is not rebuilt when it lands, and Cancel goes back to the start screen.

mod common;

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use common::Harness;
use hdtreemap_core::demo_fs::{DemoPace, DemoProfile, DemoScanner};
use hdtreemap_core::model::{NodeId, SizeMode, Tree};
use hdtreemap_core::platform::{DemoPlatform, PlatformServices};
use hdtreemap_core::scan::{PartialScan, ScanPoll, ScanProgress, ScanSource};
use hdtreemap_core::state::{AppState, ScanTarget, Screen, LIVE_INTERVAL_MIN};
use hdtreemap_core::ui::commands::{Command, Commands};
use hdtreemap_core::ui::ext_legend::{ExtLegend, EXT_LEGEND_ID};
use hdtreemap_core::ui::main_view::MAIN_VIEW_ID;
use hdtreemap_core::ui::node_actions::NodeAction;
use hdtreemap_core::ui::scan_strip::{CANCEL_BUTTON_ID, SCAN_STRIP_ID};
use hdtreemap_core::ui::start_screen::START_SCREEN_ID;
use hdtreemap_core::ui::treemap_view::TREEMAP_ID;

/// A demo scan that adds a fixed number of entries per poll, whatever the time budget, so
/// a test controls how far it gets in a frame.
struct Stepped {
    inner: DemoScanner,
    per_poll: u64,
}

impl Stepped {
    fn laptop(per_poll: u64) -> Self {
        Self {
            inner: DemoScanner::new(DemoProfile::LaptopSsd, DemoPace::Unpaced),
            per_poll,
        }
    }
}

impl ScanSource for Stepped {
    fn progress(&self) -> Arc<ScanProgress> {
        self.inner.progress()
    }
    fn poll(&mut self, _budget: Duration) -> ScanPoll {
        if self.inner.step(self.per_poll) {
            if let Some(result) = self.inner.take_result() {
                return ScanPoll::Finished(Box::new(result));
            }
        }
        ScanPoll::Running
    }
    fn partial(&self) -> Option<PartialScan<'_>> {
        self.inner.partial()
    }
    fn set_size_mode(&mut self, mode: SizeMode) {
        self.inner.set_size_mode(mode);
    }
}

fn laptop_target() -> ScanTarget {
    let volume = DemoPlatform::default()
        .list_volumes()
        .into_iter()
        .find(|v| v.id == DemoProfile::LaptopSsd.id())
        .expect("laptop drive");
    ScanTarget::volume(&volume)
}

/// A harness whose state is scanning the laptop drive `per_poll` entries a frame.
fn live_harness(per_poll: u64) -> Harness {
    let state = AppState::new();
    state.start_scan(laptop_target(), Box::new(Stepped::laptop(per_poll)));
    Harness::with_state(state, Rc::new(DemoPlatform::unpaced()))
}

fn file_count(state: &AppState) -> u64 {
    state.with_result(|r, _| r.tree.file_count()).unwrap_or(0)
}

/// Wait out the live throttle and run a frame, so the next live tick happens.
fn next_tick(h: &mut Harness) {
    std::thread::sleep(LIVE_INTERVAL_MIN.max(h.state.live_interval()));
    h.frame();
}

#[test]
fn the_map_fills_in_while_the_scan_runs() {
    let mut h = live_harness(1500);
    h.frame();
    assert!(h.state.is_scanning());
    assert!(h.state.is_live(), "the first poll's tree is shown at once");
    assert!(h.visible(MAIN_VIEW_ID));
    assert!(h.visible(SCAN_STRIP_ID));
    assert!(h.visible(TREEMAP_ID));
    assert!(h.state.with_result(|r, _| r.live).unwrap_or(false));
    let rasters = h.state.map_counters().rasters;
    assert!(rasters >= 1, "the partial tree was drawn");
    assert!(!h.widget::<ExtLegend>(EXT_LEGEND_ID).rows().is_empty());

    let (version, files, ticks) = (
        h.state.tree_version(),
        file_count(&h.state),
        h.state.live_ticks(),
    );
    // Frames within the throttle add entries but do not redraw the map.
    h.frame();
    assert_eq!(h.state.tree_version(), version);
    assert_eq!(h.state.map_counters().rasters, rasters);

    next_tick(&mut h);
    assert!(h.state.is_scanning(), "still running");
    assert_eq!(h.state.live_ticks(), ticks + 1);
    assert!(
        h.state.tree_version() > version,
        "a tick bumps the tree version"
    );
    assert!(file_count(&h.state) > files, "the tree grew");
    assert!(
        h.state.map_counters().rasters > rasters,
        "the map was redrawn"
    );
    h.state.cancel_scan();
}

#[test]
fn hover_tooltips_and_zoom_work_while_scanning_but_trash_and_rescan_wait() {
    use agg_gui::widget::find_widget_by_id;
    let mut h = live_harness(1500);
    for _ in 0..3 {
        next_tick(&mut h);
    }
    assert!(h.state.is_live());
    // Hover a file inside a folder: the status bar and the tooltip follow.
    let mut found = None;
    'search: for yi in 1..16 {
        for xi in 1..16 {
            let (x, y) = h.point_in(TREEMAP_ID, xi as f64 / 16.0, yi as f64 / 16.0);
            h.move_to(x, y);
            if let Some(hover) = h.state.hovered() {
                let nested = h
                    .state
                    .with_result(|r, _| {
                        r.tree.node(hover.node).is_some_and(|n| !n.is_dir())
                            && r.tree.depth(hover.node) >= 2
                    })
                    .unwrap_or(false);
                if hover.aggregate.is_none() && nested {
                    found = Some((x, y, hover.node));
                    break 'search;
                }
            }
        }
    }
    let (x, y, file) = found.expect("a nested file on the live map");
    let tip = find_widget_by_id(h.app.root(), TREEMAP_ID)
        .and_then(|w| w.tooltip_text().map(str::to_string));
    assert!(tip.is_some_and(|t| t.lines().count() >= 3));

    let commands = Commands::new(&h.state, &(Rc::new(DemoPlatform::unpaced()) as _));
    assert!(
        !commands.enabled(Command::Rescan),
        "rescan waits for the scan"
    );
    h.state.set_selected(Some(file));
    assert!(
        !commands.enabled(Command::Trash),
        "trash waits for the scan"
    );
    assert!(!commands.nodes().enabled(file, NodeAction::Trash));
    assert!(!commands.nodes().enabled(file, NodeAction::Rescan));
    assert!(commands.nodes().enabled(file, NodeAction::CopyPath));
    assert!(commands.enabled(Command::SizeMode(SizeMode::Apparent)));

    // Double-click zooms into the file's folder.
    let folder = h
        .state
        .with_result(|r, _| r.tree.node(file).map(|n| n.parent()))
        .flatten()
        .expect("parent");
    h.double_click_at(x, y);
    assert_eq!(h.state.zoom_root(), folder);
    h.frame();
    assert!(h.state.with_result(|r, _| r.live).unwrap_or(false));
    h.state.cancel_scan();
}

/// Node by node: names, sizes, flags, parents and the order of every child list.
fn assert_same_tree(a: &Tree, b: &Tree) {
    assert_eq!(a.arena_len(), b.arena_len());
    assert_eq!(a.total(), b.total());
    assert_eq!(a.size_mode(), b.size_mode());
    for i in 0..a.arena_len() {
        let id = NodeId(i as u32);
        let (na, nb) = (a.node(id).expect("a"), b.node(id).expect("b"));
        assert_eq!(a.name(id), b.name(id), "name of {i}");
        assert_eq!(na.sizes(), nb.sizes(), "sizes of {i}");
        assert_eq!(na.flags(), nb.flags(), "flags of {i}");
        assert_eq!(na.parent(), nb.parent(), "parent of {i}");
        let ka: Vec<NodeId> = a.children(id).collect();
        let kb: Vec<NodeId> = b.children(id).collect();
        assert_eq!(ka, kb, "children of {i}");
    }
}

#[test]
fn the_finished_tree_is_the_one_a_scan_run_to_the_end_makes_and_the_view_stays() {
    let state = AppState::new();
    state.start_scan(laptop_target(), Box::new(Stepped::laptop(20_000)));
    let screen_version = state.version();
    let mut polls = 0;
    let mut saw_live = false;
    while state.pump(Duration::from_millis(1)) {
        saw_live |= state.is_live();
        polls += 1;
        if polls == 3 {
            // The user switches the size mode mid-scan.
            state.set_size_mode(SizeMode::Apparent);
        }
        assert!(polls < 1000);
    }
    assert!(saw_live && polls > 3, "the scan ran over several polls");
    assert_eq!(state.screen(), Screen::Results(laptop_target()));
    assert_eq!(
        state.version(),
        screen_version,
        "the live view becomes the results view without a rebuild"
    );
    let mut reference = DemoScanner::new(DemoProfile::LaptopSsd, DemoPace::Unpaced)
        .run_to_end()
        .expect("result");
    reference.tree.set_size_mode(SizeMode::Apparent);
    state
        .with_result(|r, _| {
            assert!(!r.live);
            assert_same_tree(r.tree, &reference.tree);
            assert_eq!(r.errors.total(), reference.errors.total());
        })
        .expect("result");
}

#[test]
fn cancelling_a_live_scan_returns_to_the_start_screen() {
    let mut h = live_harness(1000);
    h.frame();
    assert!(h.state.is_live());
    let progress = {
        // The scan's progress, to check it was told to stop.
        let target = laptop_target();
        assert!(matches!(h.state.screen(), Screen::Scanning(t) if t == target));
        h.state.progress().expect("progress")
    };
    assert!(!progress.done);
    h.click(CANCEL_BUTTON_ID);
    assert_eq!(h.state.screen(), Screen::Start);
    assert!(!h.state.is_scanning() && !h.state.is_live());
    assert!(h.state.with_result(|_, _| ()).is_none());
    h.frame();
    assert!(h.visible(START_SCREEN_ID));
    assert!(!h.visible(MAIN_VIEW_ID));
}
