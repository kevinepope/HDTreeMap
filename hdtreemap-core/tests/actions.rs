//! Headless UI tests of the M4 actions on a finished scan: the right-click menu opens over
//! the map and over a folder-tree row (also scrolled), and its items reach a recording
//! fake `PlatformServices` (reveal, open) or act in the app (copy path, zoom, rescan); Move
//! to Trash asks first, and cancelling leaves everything while confirming trashes the item
//! and updates the totals, the folder tree and the legend; the scan root and inexact names
//! are refused; the web demo simulates the trash.

mod common;

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use agg_gui::Key;
use common::{fake, fake_map, fake_map_with, Harness};
use hdtreemap_core::model::{ExtTable, NodeFlags, NodeId, SizeMode, Sizes, Tree, TreeBuilder};
use hdtreemap_core::scan::{ScanErrors, ScanPoll, ScanProgress, ScanResult, ScanSource};
use hdtreemap_core::state::AppState;
use hdtreemap_core::state::ScanTarget;
use hdtreemap_core::ui::ext_legend::{ExtLegend, EXT_LEGEND_ID};
use hdtreemap_core::ui::folder_tree::rows::CHILD_CAP;
use hdtreemap_core::ui::folder_tree::{FolderTree, FOLDER_TREE_ID};
use hdtreemap_core::ui::main_view::{rescan_text, STATUS_TOTALS_ID};
use hdtreemap_core::ui::node_actions::{
    COPIED_MESSAGE, LOSSY_NAME_MESSAGE, SIMULATED_TRASH_MESSAGE, TRASH_ROOT_MESSAGE,
};
use hdtreemap_core::ui::sheets::{
    TRASH_CANCEL_ID, TRASH_CONFIRM_ID, TRASH_DETAILS_ID, TRASH_SHEET_ID,
};
use hdtreemap_core::ui::treemap_view::TREEMAP_ID;

/// Find a point on the map whose hovered item satisfies `want`.
fn map_point(h: &mut Harness, want: impl Fn(&AppState, NodeId) -> bool) -> (f64, f64, NodeId) {
    for yi in 1..20 {
        for xi in 1..20 {
            let (x, y) = h.point_in(TREEMAP_ID, xi as f64 / 20.0, yi as f64 / 20.0);
            h.move_to(x, y);
            if let Some(hover) = h.state.hovered() {
                if hover.aggregate.is_none() && want(&h.state, hover.node) {
                    return (x, y, hover.node);
                }
            }
        }
    }
    panic!("no point on the map matched");
}

fn is_file(state: &AppState, node: NodeId) -> bool {
    state
        .with_result(|r, _| r.tree.node(node).is_some_and(|n| !n.is_dir()))
        .unwrap_or(false)
}

fn path_of(state: &AppState, node: NodeId) -> String {
    state
        .with_result(|r, _| r.tree.path(node).display().to_string())
        .expect("path")
}

fn tree(h: &Harness) -> &FolderTree {
    h.widget::<FolderTree>(FOLDER_TREE_ID)
}

#[test]
fn right_clicking_the_map_opens_the_menu_and_its_items_reach_the_platform() {
    let (mut h, calls) = fake_map();
    let (x, y, file) = map_point(&mut h, is_file);
    h.right_click_at(x, y);
    let menu = h.context_menu();
    assert!(menu.is_open());
    assert_eq!(menu.node(), Some(file));
    assert_eq!(h.state.selected(), Some(file), "a right click selects");
    let labels: Vec<String> = menu.items().into_iter().map(|(l, _)| l).collect();
    let reveal = if cfg!(target_os = "macos") {
        "Reveal in Finder"
    } else if cfg!(windows) {
        "Show in Explorer"
    } else {
        "Show in File Manager"
    };
    assert_eq!(
        labels,
        vec![
            reveal,
            "Open",
            "Copy Path",
            "Zoom Here",
            "Zoom to Parent",
            "Rescan Folder",
            "Move to Trash\u{2026}"
        ]
    );
    // The menu opens at the click.
    let panel = menu.panel().expect("panel");
    let click_y_up = h.size.height - y;
    assert!(
        (panel.x - x).abs() < 2.0,
        "{panel:?} vs ({x}, {click_y_up})"
    );
    assert!(
        (panel.y + panel.height - click_y_up).abs() < 2.0,
        "{panel:?}"
    );
    // A file is no folder to zoom into or rescan.
    let enabled: Vec<bool> = h
        .context_menu()
        .items()
        .into_iter()
        .map(|(_, e)| e)
        .collect();
    assert_eq!(enabled, vec![true, true, true, false, true, false, true]);

    h.choose(reveal);
    assert!(!h.context_menu().is_open());
    let path = path_of(&h.state, file);
    assert_eq!(*calls.borrow(), vec![format!("reveal {path}")]);

    h.right_click_at(x, y);
    h.choose("Open");
    assert_eq!(calls.borrow().last(), Some(&format!("open {path}")));
}

#[test]
fn copy_path_sets_the_clipboard() {
    let _clipboard = agg_gui::clipboard::simulate();
    let (mut h, _) = fake_map();
    let (x, y, file) = map_point(&mut h, is_file);
    h.right_click_at(x, y);
    h.choose("Copy Path");
    assert_eq!(
        agg_gui::clipboard::get_text(),
        Some(path_of(&h.state, file))
    );
    assert!(
        h.toasts().contains(&COPIED_MESSAGE.to_string()),
        "{:?}",
        h.toasts()
    );
}

#[test]
fn zoom_items_move_the_map() {
    let (mut h, _) = fake_map();
    let (x, y, file) = map_point(&mut h, |s, n| {
        is_file(s, n) && s.with_result(|r, _| r.tree.depth(n) >= 2).unwrap_or(false)
    });
    let folder = h
        .state
        .with_result(|r, _| r.tree.node(file).map(|n| n.parent()))
        .flatten()
        .expect("folder");
    h.right_click_at(x, y);
    h.choose("Zoom to Parent");
    assert_eq!(h.state.zoom_root(), folder);
    // On the zoomed map, right-click a folder and zoom into it.
    h.frame();
    let found = (1..20)
        .flat_map(|yi| (1..20).map(move |xi| (xi, yi)))
        .find_map(|(xi, yi)| {
            let (x, y) = h.point_in(TREEMAP_ID, xi as f64 / 20.0, yi as f64 / 20.0);
            h.move_to(x, y);
            let node = h.state.hovered()?.node;
            let dir = h
                .state
                .with_result(|r, _| r.tree.node(node).is_some_and(|n| n.is_dir()))
                .unwrap_or(false);
            (dir && node != folder).then_some((x, y, node))
        });
    if let Some((x, y, sub)) = found {
        h.right_click_at(x, y);
        h.choose("Zoom Here");
        assert_eq!(h.state.zoom_root(), sub);
    }
}

#[test]
fn right_clicking_a_scrolled_tree_row_opens_the_menu_for_that_row() {
    let _clipboard = agg_gui::clipboard::simulate();
    let mut h = common::demo_map("dev-workstation");
    // The smallest listed child of the folder with the most children: far down the tree.
    let deep = h
        .state
        .with_result(|r, _| {
            let tree = &r.tree;
            let folder = (0..tree.arena_len() as u32)
                .map(NodeId)
                .filter(|&id| tree.node(id).is_some_and(|n| n.is_dir()))
                .max_by_key(|&id| tree.children(id).count().min(CHILD_CAP))?;
            tree.children(folder).take(CHILD_CAP).last()
        })
        .flatten()
        .expect("a crowded folder");
    h.state.set_selected(Some(deep));
    h.frame();
    h.frame();
    assert!(
        tree(&h).scroll_offset() > 100.0,
        "the tree scrolled: {}",
        tree(&h).scroll_offset()
    );
    let rect = tree(&h).row_rect(deep).expect("the row is in view");
    let (x, y) = h.point_at_local(FOLDER_TREE_ID, rect);
    h.state.set_selected(None);
    h.frame();
    h.move_to(1.0, 1.0);
    h.right_click_at(x, y);
    let menu = h.context_menu();
    assert!(menu.is_open());
    assert_eq!(
        menu.node(),
        Some(deep),
        "the row under the click, at any scroll"
    );
    assert_eq!(h.state.selected(), Some(deep));
    let panel = menu.panel().expect("panel");
    let click_y_up = h.size.height - y;
    assert!((panel.x - x).abs() < 2.0, "{panel:?} vs {x}");
    assert!(
        (panel.y + panel.height - click_y_up).abs() < 2.0,
        "{panel:?} vs {click_y_up}"
    );
    h.choose("Copy Path");
    assert_eq!(
        agg_gui::clipboard::get_text(),
        Some(path_of(&h.state, deep))
    );
}

#[test]
fn move_to_trash_asks_first_then_updates_totals_tree_and_legend() {
    let (mut h, calls) = fake_map();
    let (x, y, file) = map_point(&mut h, is_file);
    let ext = h
        .state
        .with_result(|r, _| r.tree.node(file).map(|n| n.ext()))
        .flatten()
        .expect("ext");
    let totals = h.label_text(STATUS_TOTALS_ID);
    let count_of = |h: &Harness| {
        h.state
            .with_result(|r, _| {
                ExtTable::from_tree(r.tree)
                    .by_ext(ext)
                    .map_or(0, |row| row.count)
            })
            .expect("result")
    };
    let ext_files = count_of(&h);

    // Cancel: nothing happens.
    h.right_click_at(x, y);
    h.choose("Move to Trash\u{2026}");
    assert!(h.visible(TRASH_SHEET_ID), "the confirmation opens");
    let details = h.label_text(TRASH_DETAILS_ID);
    assert!(details.contains("1 file"), "{details}");
    h.click(TRASH_CANCEL_ID);
    assert!(!h.visible(TRASH_SHEET_ID));
    assert!(calls.borrow().is_empty());
    assert!(h
        .state
        .with_result(|r, _| r.tree.is_live(file))
        .unwrap_or(false));

    // Escape cancels too.
    h.right_click_at(x, y);
    h.choose("Move to Trash\u{2026}");
    h.key(Key::Escape);
    assert!(!h.visible(TRASH_SHEET_ID));
    assert!(calls.borrow().is_empty());

    // Confirm: trashed through the platform and gone from the tree.
    h.right_click_at(x, y);
    h.choose("Move to Trash\u{2026}");
    let path = path_of(&h.state, file);
    h.click(TRASH_CONFIRM_ID);
    assert!(!h.visible(TRASH_SHEET_ID));
    assert_eq!(*calls.borrow(), vec![format!("trash {path}")]);
    assert!(!h
        .state
        .with_result(|r, _| r.tree.is_live(file))
        .unwrap_or(true));
    h.frame();
    assert_ne!(h.label_text(STATUS_TOTALS_ID), totals, "the totals changed");
    assert_eq!(count_of(&h), ext_files - 1);
    let legend_rows = h.widget::<ExtLegend>(EXT_LEGEND_ID).rows().len();
    let expected_rows = h
        .state
        .zoom_extensions()
        .map(|t| t.rows().len())
        .expect("table");
    assert_eq!(legend_rows, expected_rows, "the legend follows");
    assert!(tree(&h).row(file).is_none(), "the tree row is gone");
    assert!(
        h.toasts().iter().any(|t| t.starts_with("Moved ")),
        "{:?}",
        h.toasts()
    );
}

#[test]
fn a_failed_trash_is_reported_and_changes_nothing() {
    let (mut platform, _) = fake(Vec::new(), None);
    platform.pace = hdtreemap_core::demo_fs::DemoPace::Unpaced;
    platform.trash_error = Some("Could not move it: permission denied".into());
    let (mut h, _) = fake_map_with(platform);
    let (x, y, file) = map_point(&mut h, is_file);
    h.right_click_at(x, y);
    h.choose("Move to Trash\u{2026}");
    h.click(TRASH_CONFIRM_ID);
    assert!(h
        .state
        .with_result(|r, _| r.tree.is_live(file))
        .unwrap_or(false));
    assert!(h
        .toasts()
        .contains(&"Could not move it: permission denied".to_string()));
}

#[test]
fn the_scan_root_cannot_be_trashed() {
    let (mut h, calls) = fake_map();
    h.state.set_selected(Some(NodeId(0)));
    h.frame();
    let delete = if agg_gui::platform::current_platform() == agg_gui::platform::Platform::MacOS {
        (
            Key::Backspace,
            agg_gui::Modifiers {
                meta: true,
                ..Default::default()
            },
        )
    } else {
        (Key::Delete, agg_gui::Modifiers::default())
    };
    h.key_with(delete.0, delete.1);
    assert!(!h.visible(TRASH_SHEET_ID));
    assert!(calls.borrow().is_empty());
    assert!(
        h.toasts().contains(&TRASH_ROOT_MESSAGE.to_string()),
        "{:?}",
        h.toasts()
    );
}

/// A scan source that finishes at once with `tree`.
struct FixedScan(Option<Tree>, Arc<ScanProgress>);

impl ScanSource for FixedScan {
    fn progress(&self) -> Arc<ScanProgress> {
        Arc::clone(&self.1)
    }
    fn poll(&mut self, _budget: Duration) -> ScanPoll {
        let tree = self
            .0
            .take()
            .unwrap_or_else(|| TreeBuilder::new("/r").finish(SizeMode::Allocated));
        ScanPoll::Finished(Box::new(ScanResult {
            root: "/r".into(),
            tree,
            errors: ScanErrors::default(),
            elapsed: Duration::ZERO,
        }))
    }
}

#[test]
fn inexact_names_are_refused() {
    // /r/caf\u{fffd}/inside.bin: the folder's name was stored lossily, as the scanner does
    // for a non-UTF-8 name.
    let mut b = TreeBuilder::new("/r");
    let root = b.root();
    let odd = b.add_dir(root, "caf\u{fffd}");
    b.mark(odd, NodeFlags::LOSSY_NAME);
    let inside = b.add_file(
        odd,
        "inside.bin",
        Sizes::new(5000, 5000),
        NodeFlags::empty(),
    );
    b.add_file(
        root,
        "plain.bin",
        Sizes::new(3000, 3000),
        NodeFlags::empty(),
    );
    let state = AppState::new();
    state.start_scan(
        ScanTarget::folder("/r"),
        Box::new(FixedScan(
            Some(b.finish(SizeMode::Allocated)),
            Arc::new(ScanProgress::new()),
        )),
    );
    while state.pump(Duration::from_millis(10)) {}
    let (platform, _) = fake(Vec::new(), None);
    let calls = Rc::clone(&platform.calls);
    let mut h = Harness::with_state(state, Rc::new(platform));
    let (x, y, node) = map_point(&mut h, |_, n| n == inside);
    assert_eq!(node, inside);
    h.right_click_at(x, y);
    h.choose("Move to Trash\u{2026}");
    assert!(
        !h.visible(TRASH_SHEET_ID),
        "no confirmation for an inexact name"
    );
    h.right_click_at(x, y);
    h.choose("Open");
    h.right_click_at(x, y);
    let reveal = h.context_menu().items()[0].0.clone();
    h.choose(&reveal);
    assert!(calls.borrow().is_empty(), "{:?}", calls.borrow());
    assert!(
        h.toasts().contains(&LOSSY_NAME_MESSAGE.to_string()),
        "{:?}",
        h.toasts()
    );
}

#[test]
fn rescan_folder_splices_the_folder_back_in() {
    let (mut h, calls) = fake_map();
    let (_, _, file) = map_point(&mut h, |s, n| {
        is_file(s, n) && s.with_result(|r, _| r.tree.depth(n) >= 2).unwrap_or(false)
    });
    let folder = h
        .state
        .with_result(|r, _| r.tree.node(file).map(|n| n.parent()))
        .flatten()
        .expect("folder");
    let before = h
        .state
        .with_result(|r, _| r.tree.sizes(folder))
        .expect("sizes");
    h.state.remove_node(file).expect("remove");
    h.frame();
    // Right-click the folder in the tree.
    h.state.set_selected(Some(folder));
    h.frame();
    h.frame();
    let rect = tree(&h).row_rect(folder).expect("row");
    let (tx, ty) = h.point_at_local(FOLDER_TREE_ID, rect);
    h.right_click_at(tx, ty);
    h.choose("Rescan Folder");
    assert!(
        calls.borrow().iter().any(|c| c.starts_with("scan ")),
        "{:?}",
        calls.borrow()
    );
    assert!(
        h.state.is_rescanning() || h.state.with_result(|r, _| r.tree.sizes(folder)) == Some(before)
    );
    if h.state.is_rescanning() {
        assert!(rescan_text(&h.state).is_some_and(|t| t.contains("Rescanning")));
    }
    let done = h.frames_until(500, |h| !h.state.is_rescanning());
    assert!(done);
    assert_eq!(
        h.state.with_result(|r, _| r.tree.sizes(folder)),
        Some(before)
    );
    assert!(
        h.toasts().iter().any(|t| t.starts_with("Rescanned")),
        "{:?}",
        h.toasts()
    );
}

#[test]
fn the_demo_trash_removes_from_the_simulated_tree_only() {
    let mut h = common::demo_map("usb-stick");
    let (x, y, file) = map_point(&mut h, is_file);
    let total = h.state.with_result(|r, _| r.tree.total()).expect("total");
    h.right_click_at(x, y);
    let labels: Vec<String> = h
        .context_menu()
        .items()
        .into_iter()
        .map(|(l, _)| l)
        .collect();
    assert_eq!(labels[0], "Reveal (demo)");
    h.choose("Move to Trash\u{2026}");
    h.click(TRASH_CONFIRM_ID);
    assert!(!h
        .state
        .with_result(|r, _| r.tree.is_live(file))
        .unwrap_or(true));
    assert!(
        h.state
            .with_result(|r, _| r.tree.total())
            .expect("total")
            .allocated
            < total.allocated
    );
    assert!(
        h.toasts().contains(&SIMULATED_TRASH_MESSAGE.to_string()),
        "{:?}",
        h.toasts()
    );
    // Reveal on the demo says it is simulated.
    let (x, y, _) = map_point(&mut h, is_file);
    h.right_click_at(x, y);
    h.choose("Reveal (demo)");
    assert!(
        h.toasts().iter().any(|t| t.contains("browser demo")),
        "{:?}",
        h.toasts()
    );
}
