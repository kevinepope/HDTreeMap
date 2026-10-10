//! Headless UI tests of the treemap view on a finished demo scan (the small USB stick):
//! hover sets the status line without re-rasterising, click selects, Escape clears, double
//! click zooms in (the breadcrumb gains a segment), Backspace and the mouse back button zoom
//! out, breadcrumb segments zoom out to their folder, the size-mode switch re-lays out, the
//! cushion toggle re-rasterises, a resize re-rasterises, and an empty scan draws safely.

mod common;

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use agg_gui::{Key, Modifiers, MouseButton};
use common::{fake, Harness};
use hdtreemap_core::model::{NodeId, SizeMode, TreeBuilder};
use hdtreemap_core::platform::{DemoPlatform, PlatformServices};
use hdtreemap_core::render::Shading;
use hdtreemap_core::scan::{ScanErrors, ScanPoll, ScanProgress, ScanResult, ScanSource};
use hdtreemap_core::state::{AppState, ScanTarget, Screen};
use hdtreemap_core::ui::breadcrumb::segment_id;
use hdtreemap_core::ui::main_view::{SHADING_ID, SIZE_MODE_ID, STATUS_IDLE_TEXT, STATUS_ITEM_ID};
use hdtreemap_core::ui::treemap_view::TREEMAP_ID;

/// A harness on the main view of a finished (unpaced) USB-stick scan.
fn usb_map() -> Harness {
    let state = AppState::new();
    let platform = DemoPlatform::unpaced();
    let volume = platform
        .list_volumes()
        .into_iter()
        .find(|v| v.id == "usb-stick")
        .expect("usb stick");
    let target = ScanTarget::volume(&volume);
    state.start_scan(target.clone(), platform.scan_source(&target));
    while state.pump(Duration::from_secs(5)) {}
    let mut h = Harness::with_state(state, Rc::new(platform));
    h.frame();
    assert!(matches!(h.state.screen(), Screen::Results(_)));
    assert!(h.visible(TREEMAP_ID));
    h
}

/// Scan a grid over the map for a point whose hovered item satisfies `want`.
fn find_point(h: &mut Harness, want: impl Fn(&AppState, NodeId) -> bool) -> (f64, f64) {
    for yi in 1..20 {
        for xi in 1..20 {
            let (x, y) = h.point_in(TREEMAP_ID, xi as f64 / 20.0, yi as f64 / 20.0);
            h.move_to(x, y);
            if let Some(hover) = h.state.hovered() {
                if hover.aggregate.is_none() && want(&h.state, hover.node) {
                    return (x, y);
                }
            }
        }
    }
    panic!("no point on the map matched");
}

/// A file at least two levels below the scan root (its folder is not the root).
fn nested_file(state: &AppState, node: NodeId) -> bool {
    state
        .with_result(|r, _| {
            let tree = &r.tree;
            tree.node(node).is_some_and(|n| !n.is_dir()) && tree.depth(node) >= 2
        })
        .unwrap_or(false)
}

fn parent_of(state: &AppState, node: NodeId) -> NodeId {
    state
        .with_result(|r, _| r.tree.node(node).map(|n| n.parent()))
        .flatten()
        .expect("parent")
}

#[test]
fn hovering_sets_the_status_line_without_rerasterising() {
    let mut h = usb_map();
    assert_eq!(h.label_text(STATUS_ITEM_ID), STATUS_IDLE_TEXT);
    let rasters = h.state.map_counters().rasters;
    let layouts = h.state.map_counters().layouts;
    let mut seen = Vec::new();
    for f in [0.2, 0.4, 0.6, 0.8] {
        let (x, y) = h.point_in(TREEMAP_ID, f, f);
        h.move_to(x, y);
        let hover = h.state.hovered().expect("the map is under the pointer");
        seen.push(hover.node);
        let line = h.label_text(STATUS_ITEM_ID);
        assert!(line.starts_with("/demo/usb-stick"), "{line}");
        assert!(line.contains('\u{2014}'), "path and size: {line}");
    }
    assert!(seen.windows(2).any(|w| w[0] != w[1]), "different items");
    assert_eq!(
        h.state.map_counters().rasters,
        rasters,
        "hover never re-rasterises"
    );
    assert_eq!(h.state.map_counters().layouts, layouts);

    // Leaving the map clears the hover.
    h.move_to(1.0, 1.0);
    assert_eq!(h.state.hovered(), None);
    assert_eq!(h.label_text(STATUS_ITEM_ID), STATUS_IDLE_TEXT);
}

#[test]
fn clicking_selects_and_escape_clears() {
    let mut h = usb_map();
    let (x, y) = h.point_in(TREEMAP_ID, 0.3, 0.3);
    h.click_at(x, y);
    let hovered = h.state.hovered().expect("hovered").node;
    assert_eq!(h.state.selected(), Some(hovered));
    // Moving off keeps the selection, and the status line shows it.
    h.move_to(1.0, 1.0);
    assert_eq!(h.state.selected(), Some(hovered));
    assert_ne!(h.label_text(STATUS_ITEM_ID), STATUS_IDLE_TEXT);
    h.key(Key::Escape);
    assert_eq!(h.state.selected(), None);
    assert_eq!(h.label_text(STATUS_ITEM_ID), STATUS_IDLE_TEXT);
}

#[test]
fn double_click_zooms_in_and_backspace_zooms_out() {
    let mut h = usb_map();
    assert!(h.visible(&segment_id(0)));
    assert!(!h.visible(&segment_id(1)));
    let (x, y) = find_point(&mut h, nested_file);
    let file = h.state.hovered().expect("hovered").node;
    let folder = parent_of(&h.state, file);
    let depth = h
        .state
        .with_result(|r, _| r.tree.depth(folder))
        .expect("depth");
    let layouts = h.state.map_counters().layouts;

    h.double_click_at(x, y);
    assert_eq!(h.state.zoom_root(), folder);
    assert!(
        h.visible(&segment_id(depth)),
        "the breadcrumb gains the folder"
    );
    assert!(
        h.state.map_counters().layouts > layouts,
        "zooming re-lays out"
    );

    h.key(Key::Backspace);
    let up = parent_of(&h.state, folder);
    assert_eq!(h.state.zoom_root(), up);
    assert!(!h.visible(&segment_id(depth)));
}

#[test]
fn the_mouse_back_button_zooms_out() {
    let mut h = usb_map();
    let (x, y) = find_point(&mut h, nested_file);
    h.double_click_at(x, y);
    let zoomed = h.state.zoom_root();
    assert_ne!(zoomed, NodeId(0));
    let (x, y) = h.point_in(TREEMAP_ID, 0.5, 0.5);
    h.app
        .on_mouse_down(x, y, MouseButton::Other(3), Modifiers::default());
    h.app
        .on_mouse_up(x, y, MouseButton::Other(3), Modifiers::default());
    h.frame();
    assert_eq!(h.state.zoom_root(), parent_of(&h.state, zoomed));
}

#[test]
fn breadcrumb_segments_zoom_out_to_their_folder() {
    let mut h = usb_map();
    let deep = h
        .state
        .with_result(|r, _| {
            let tree = &r.tree;
            (0..tree.arena_len() as u32)
                .map(NodeId)
                .find(|&id| tree.node(id).is_some_and(|n| n.is_dir()) && tree.depth(id) == 2)
        })
        .flatten()
        .expect("a folder two levels down");
    assert!(h.state.set_zoom_root(deep));
    h.frame();
    for depth in 0..=2 {
        assert!(h.visible(&segment_id(depth)), "segment {depth}");
    }
    let middle = parent_of(&h.state, deep);
    h.click(&segment_id(1));
    assert_eq!(h.state.zoom_root(), middle);
    assert!(!h.visible(&segment_id(2)));
    h.click(&segment_id(0));
    assert_eq!(h.state.zoom_root(), NodeId(0));
    assert!(!h.visible(&segment_id(1)));
}

#[test]
fn the_size_mode_switch_relays_the_map_out() {
    let mut h = usb_map();
    let before = h.state.map_counters();
    let (x, y) = h.point_in(SIZE_MODE_ID, 0.75, 0.5);
    h.click_at(x, y);
    assert_eq!(h.state.size_mode(), SizeMode::Apparent);
    let after = h.state.map_counters();
    assert!(after.layouts > before.layouts);
    assert!(after.rasters > before.rasters);
}

#[test]
fn the_cushion_toggle_rerasterises_without_relayout() {
    let mut h = usb_map();
    assert_eq!(h.state.shading(), Shading::Cushion);
    let before = h.state.map_counters();
    h.click(SHADING_ID);
    assert_eq!(h.state.shading(), Shading::Flat);
    let after = h.state.map_counters();
    assert_eq!(after.layouts, before.layouts);
    assert_eq!(after.rasters, before.rasters + 1);
    h.click(SHADING_ID);
    assert_eq!(h.state.shading(), Shading::Cushion);
}

#[test]
fn resizing_rerasterises_and_a_steady_window_does_not() {
    let mut h = usb_map();
    let before = h.state.map_counters();
    for _ in 0..5 {
        h.frame();
    }
    assert_eq!(
        h.state.map_counters(),
        before,
        "nothing changed, nothing redone"
    );
    h.resize(900.0, 700.0);
    let after = h.state.map_counters();
    assert_eq!(after.layouts, before.layouts + 1);
    assert_eq!(after.rasters, before.rasters + 1);
}

/// A scan source that finishes at once with an empty folder.
struct EmptyScan(Arc<ScanProgress>);

impl ScanSource for EmptyScan {
    fn progress(&self) -> Arc<ScanProgress> {
        Arc::clone(&self.0)
    }
    fn poll(&mut self, _budget: Duration) -> ScanPoll {
        ScanPoll::Finished(Box::new(ScanResult {
            root: "/empty".into(),
            tree: TreeBuilder::new("/empty").finish(SizeMode::Allocated),
            errors: ScanErrors::default(),
            elapsed: Duration::ZERO,
        }))
    }
}

#[test]
fn an_empty_scan_shows_an_empty_map() {
    let state = AppState::new();
    state.start_scan(
        ScanTarget::folder("/empty"),
        Box::new(EmptyScan(Arc::new(ScanProgress::new()))),
    );
    while state.pump(Duration::from_millis(10)) {}
    let (platform, _) = fake(Vec::new(), None);
    let mut h = Harness::with_state(state, Rc::new(platform));
    assert!(h.visible(TREEMAP_ID));
    let (x, y) = h.point_in(TREEMAP_ID, 0.5, 0.5);
    h.move_to(x, y);
    assert_eq!(h.state.hovered(), None);
    h.double_click_at(x, y);
    h.key(Key::Backspace);
    assert_eq!(h.state.zoom_root(), NodeId(0));
    h.resize(2.0, 2.0);
}

#[test]
fn the_tooltip_is_multi_line_and_keyed_on_the_hovered_rectangle() {
    use agg_gui::widget::find_widget_by_id;
    let mut h = usb_map();
    let key_and_tip = |h: &Harness| {
        let w = find_widget_by_id(h.app.root(), TREEMAP_ID).expect("treemap");
        (w.tooltip_key(), w.tooltip_text().map(str::to_string))
    };
    let (x, y) = h.point_in(TREEMAP_ID, 0.1, 0.1);
    h.move_to(x, y);
    let (k1, t1) = key_and_tip(&h);
    let (x, y) = h.point_in(TREEMAP_ID, 0.9, 0.9);
    h.move_to(x, y);
    let (k2, t2) = key_and_tip(&h);
    assert!(
        k1.is_some() && k2.is_some(),
        "a hovered rectangle has a key"
    );
    assert_ne!(k1, k2, "different rectangles, different keys");
    assert_ne!(h.state.hovered(), None);
    for tip in [t1, t2] {
        let tip = tip.expect("tooltip");
        assert!(
            tip.lines().count() >= 3,
            "name / size / path lines: {tip:?}"
        );
    }
    // Moving a pixel within the same (large) rectangle keeps the key.
    let (x, y) = h.point_in(TREEMAP_ID, 0.1, 0.1);
    h.move_to(x, y);
    h.move_to(x + 1.0, y + 1.0);
    assert_eq!(key_and_tip(&h).0, k1);
    // Off the map: no key.
    h.move_to(1.0, 1.0);
    assert_eq!(key_and_tip(&h).0, None);
}
