//! Headless UI tests of the linked views (M3) on finished demo scans: the folder tree and
//! the map share one selection (each way, the tree expanding and scrolling to a node chosen
//! on the map), a double click or Enter on a folder row zooms, hovering a row hovers it on
//! the map, the tree stays lazy on the archive server's 1.5 M files, the extension legend
//! lists the zoom root's extensions and re-sorts on a header click, and a legend click
//! highlights the extension's files on the map (again, or Escape, clears it).

mod common;

use agg_gui::Key;
use common::{demo_map, Harness};
use hdtreemap_core::model::{ExtTable, NodeId};
use hdtreemap_core::state::AppState;
use hdtreemap_core::ui::ext_legend::{Column, ExtLegend, EXT_LEGEND_ID, NO_EXTENSION};
use hdtreemap_core::ui::folder_tree::rows::CHILD_CAP;
use hdtreemap_core::ui::folder_tree::{FolderTree, FOLDER_TREE_ID};
use hdtreemap_core::ui::main_view::{
    highlight_text, STATUS_HIGHLIGHT_ID, STATUS_ITEM_ID, STATUS_TOTALS_ID,
};
use hdtreemap_core::ui::treemap_view::{TreemapView, TREEMAP_ID};

fn tree(h: &Harness) -> &FolderTree {
    h.widget::<FolderTree>(FOLDER_TREE_ID)
}

fn legend(h: &Harness) -> &ExtLegend {
    h.widget::<ExtLegend>(EXT_LEGEND_ID)
}

/// The root's children, largest first.
fn root_children(state: &AppState) -> Vec<NodeId> {
    state
        .with_result(|r, _| r.tree.children(r.tree.root()).collect())
        .expect("result")
}

fn is_dir(state: &AppState, node: NodeId) -> bool {
    state
        .with_result(|r, _| r.tree.node(node).is_some_and(|n| n.is_dir()))
        .unwrap_or(false)
}

fn name(state: &AppState, node: NodeId) -> String {
    state
        .with_result(|r, _| r.tree.name(node).to_string())
        .unwrap_or_default()
}

/// Click `node`'s row in the folder tree.
fn click_row(h: &mut Harness, node: NodeId) {
    let rect = tree(h).row_rect(node).expect("the row is in view");
    let (x, y) = h.point_at_local(FOLDER_TREE_ID, rect);
    h.click_at(x, y);
}

#[test]
fn the_status_bar_stays_on_screen_beside_the_linked_views() {
    let h = demo_map("usb-stick");
    for id in [
        STATUS_ITEM_ID,
        STATUS_TOTALS_ID,
        FOLDER_TREE_ID,
        EXT_LEGEND_ID,
    ] {
        let r = h.rect(id);
        assert!(
            r.x >= 0.0 && r.y >= 0.0 && r.x + r.width <= h.size.width + 0.5,
            "{id} is off screen: {r:?}"
        );
    }
    let totals = h.rect(STATUS_TOTALS_ID);
    assert!(totals.width > 50.0, "the totals have room: {totals:?}");
    // The tree is left of the map, the legend under the tree.
    let (t, l, m) = (
        h.rect(FOLDER_TREE_ID),
        h.rect(EXT_LEGEND_ID),
        h.rect(TREEMAP_ID),
    );
    assert!(t.x + t.width <= m.x + 0.5);
    assert!(l.y + l.height <= t.y + 0.5);
    let share = (t.width + 6.0) / h.size.width;
    assert!((share - 0.28).abs() < 0.02, "side column share {share}");
}

#[test]
fn selecting_a_tree_row_selects_it_on_the_map() {
    let mut h = demo_map("usb-stick");
    let movies = root_children(&h.state)[0];
    assert_eq!(name(&h.state, movies), "Movies");
    click_row(&mut h, movies);
    assert_eq!(h.state.selected(), Some(movies));
    assert_eq!(tree(&h).selected_node(), Some(movies));
    let line = h.label_text(STATUS_ITEM_ID);
    assert!(line.contains("Movies"), "{line}");
}

#[test]
fn selecting_on_the_map_expands_the_tree_to_it() {
    let mut h = demo_map("usb-stick");
    // A file two folders down, under the pointer somewhere on the map.
    let mut target = None;
    'grid: for yi in 1..20 {
        for xi in 1..20 {
            let (x, y) = h.point_in(TREEMAP_ID, xi as f64 / 20.0, yi as f64 / 20.0);
            h.move_to(x, y);
            if let Some(hover) = h.state.hovered() {
                let deep = h
                    .state
                    .with_result(|r, _| r.tree.depth(hover.node) >= 2)
                    .unwrap_or(false);
                if hover.aggregate.is_none() && !is_dir(&h.state, hover.node) && deep {
                    h.click_at(x, y);
                    target = Some(hover.node);
                    break 'grid;
                }
            }
        }
    }
    let file = target.expect("a nested file on the map");
    assert_eq!(h.state.selected(), Some(file));
    assert_eq!(tree(&h).selected_node(), Some(file));
    let ancestors: Vec<NodeId> = h
        .state
        .with_result(|r, _| r.tree.ancestors(file).collect())
        .expect("ancestors");
    for a in ancestors {
        assert!(tree(&h).is_expanded(a), "{} is expanded", name(&h.state, a));
    }
    assert!(tree(&h).row_rect(file).is_some(), "the row is in view");
}

#[test]
fn a_selection_far_down_the_tree_scrolls_into_view() {
    let mut h = demo_map("dev-workstation");
    // The smallest listed child of the biggest folder with many children.
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
        .expect("a folder with children");
    h.state.set_selected(Some(deep));
    h.frame();
    assert_eq!(tree(&h).selected_node(), Some(deep));
    let row = tree(&h).row_rect(deep).expect("scrolled into view");
    let height = h.rect(FOLDER_TREE_ID).height;
    assert!(row.y >= 0.0 && row.y + row.height <= height);
}

#[test]
fn double_clicking_or_enter_on_a_folder_row_zooms() {
    let mut h = demo_map("usb-stick");
    let children = root_children(&h.state);
    let (first, second) = (children[0], children[1]);
    let rect = tree(&h).row_rect(first).expect("row");
    let (x, y) = h.point_at_local(FOLDER_TREE_ID, rect);
    h.double_click_at(x, y);
    h.frame();
    assert_eq!(h.state.zoom_root(), first);
    assert!(tree(&h).is_expanded(first), "the zoom path is expanded");

    click_row(&mut h, second);
    assert_eq!(h.state.selected(), Some(second));
    h.key(Key::Enter);
    assert_eq!(h.state.zoom_root(), second);
}

#[test]
fn hovering_a_row_hovers_it_on_the_map() {
    let mut h = demo_map("usb-stick");
    let installers = root_children(&h.state)[1];
    let rect = tree(&h).row_rect(installers).expect("row");
    let (x, y) = h.point_at_local(FOLDER_TREE_ID, rect);
    h.move_to(x, y);
    h.frame();
    assert_eq!(h.state.hovered().map(|hv| hv.node), Some(installers));
    let line = h.label_text(STATUS_ITEM_ID);
    assert!(line.contains("Installers"), "{line}");
    // Onto the map: the map's own hover takes over.
    let (mx, my) = h.point_in(TREEMAP_ID, 0.5, 0.5);
    h.move_to(mx, my);
    let on_map = h.state.hovered().expect("map hover").node;
    assert_ne!(on_map, installers);
    // Off everything: no hover.
    h.move_to(1.0, 1.0);
    assert_eq!(h.state.hovered(), None);
}

#[test]
fn the_lazy_tree_only_builds_rows_for_open_folders() {
    let mut h = demo_map("archive-server");
    let files = h
        .state
        .with_result(|r, _| r.tree.file_count())
        .expect("count");
    assert!(files > 1_000_000, "the archive server is big: {files}");
    let count = tree(&h).node_count();
    assert!(count < 2 * CHILD_CAP + 10, "{count} rows for a fresh tree");
    // Opening the biggest folder adds at most its capped children (and a "more" row).
    let biggest = root_children(&h.state)[0];
    click_row(&mut h, biggest);
    h.key(Key::ArrowRight);
    assert!(tree(&h).is_expanded(biggest));
    let opened = tree(&h).node_count();
    assert!(
        opened > count && opened <= count + 2 * CHILD_CAP + 1,
        "{opened}"
    );
    // A folder with more than the cap lists a "more" row.
    let crowded = h
        .state
        .with_result(|r, _| {
            let tree = &r.tree;
            (0..tree.arena_len() as u32)
                .map(NodeId)
                .find(|&id| tree.children(id).count() > CHILD_CAP)
        })
        .flatten()
        .expect("a crowded folder");
    h.state.set_zoom_root(crowded);
    h.frame();
    let labels = tree(&h).child_labels(crowded);
    assert_eq!(labels.len(), CHILD_CAP + 1);
    assert!(
        labels[CHILD_CAP].contains(" more "),
        "{}",
        labels[CHILD_CAP]
    );
}

#[test]
fn rows_show_size_and_share_of_the_parent() {
    let h = demo_map("usb-stick");
    let movies = root_children(&h.state)[0];
    let row = tree(&h).row(movies).expect("row");
    assert_eq!(row.label, "Movies");
    assert_eq!(row.secondary.as_deref(), Some("10.5 GB"));
    let share = row.fraction.expect("a share bar");
    assert!((share - 0.5).abs() < 0.001, "{share}");
    let root = h.state.with_result(|r, _| r.tree.root()).expect("root");
    let root_row = tree(&h).row(root).expect("root row");
    assert_eq!(
        root_row.fraction, None,
        "the scan root has no parent to share"
    );
}

#[test]
fn legend_rows_follow_the_zoom_root() {
    let mut h = demo_map("usb-stick");
    let expect = |state: &AppState, root: NodeId| -> Vec<(String, u64)> {
        state
            .with_result(|r, _| {
                ExtTable::from_subtree(&r.tree, root)
                    .rows()
                    .iter()
                    .map(|row| (row.name.clone(), row.count))
                    .collect()
            })
            .expect("rows")
    };
    let shown = |h: &Harness| -> Vec<(String, u64)> {
        legend(h)
            .rows()
            .into_iter()
            .map(|r| (r.name, r.count))
            .collect()
    };
    assert_eq!(shown(&h), expect(&h.state, NodeId(0)));
    let photos = root_children(&h.state)
        .into_iter()
        .find(|&c| name(&h.state, c) == "Photos")
        .expect("Photos");
    h.state.set_zoom_root(photos);
    h.frame();
    assert_eq!(shown(&h), expect(&h.state, photos));
    // Swatches are the map's palette colours.
    let palette = h.state.palette().expect("palette");
    for row in legend(&h).rows() {
        assert_eq!(row.color, palette.ext_color(row.ext));
    }
}

#[test]
fn a_header_click_resorts_the_legend() {
    let mut h = demo_map("usb-stick");
    let sizes: Vec<u64> = legend(&h).rows().iter().map(|r| r.bytes).collect();
    assert!(
        sizes.windows(2).all(|w| w[0] >= w[1]),
        "size, largest first"
    );

    let header = legend(&h).header_rect(Column::Files).expect("header");
    let (x, y) = h.point_at_local(EXT_LEGEND_ID, header);
    h.click_at(x, y);
    let counts: Vec<u64> = legend(&h).rows().iter().map(|r| r.count).collect();
    assert!(counts.windows(2).all(|w| w[0] >= w[1]), "{counts:?}");
    h.click_at(x, y);
    let counts: Vec<u64> = legend(&h).rows().iter().map(|r| r.count).collect();
    assert!(
        counts.windows(2).all(|w| w[0] <= w[1]),
        "reversed: {counts:?}"
    );

    let header = legend(&h).header_rect(Column::Extension).expect("header");
    let (x, y) = h.point_at_local(EXT_LEGEND_ID, header);
    h.click_at(x, y);
    let names: Vec<String> = legend(&h).rows().into_iter().map(|r| r.name).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}

#[test]
fn clicking_a_legend_row_highlights_its_files_on_the_map() {
    let mut h = demo_map("usb-stick");
    let jpg = legend(&h)
        .rows()
        .iter()
        .position(|r| r.name == "jpg")
        .expect("jpg row");
    let row = legend(&h).row_rect(jpg).expect("row in view");
    let (x, y) = h.point_at_local(EXT_LEGEND_ID, row);
    h.click_at(x, y);
    let ext = legend(&h).rows()[jpg].ext;
    assert_eq!(h.state.highlight_ext(), Some(ext));
    h.frame();
    let painted = h.widget::<TreemapView>(TREEMAP_ID).highlighted_rects();
    assert!(painted > 50, "the photos are highlighted: {painted}");
    // The dimmed map is made once; hovering it never re-rasterises.
    let rasters = h.state.map_counters().rasters;
    for f in [0.2, 0.5, 0.8] {
        let (mx, my) = h.point_in(TREEMAP_ID, f, f);
        h.move_to(mx, my);
    }
    assert_eq!(h.state.map_counters().rasters, rasters);
    assert_eq!(
        h.widget::<TreemapView>(TREEMAP_ID).highlighted_rects(),
        painted
    );
    let text = h.label_text(STATUS_HIGHLIGHT_ID);
    assert!(text.starts_with("Highlighting .jpg: 170 files, "), "{text}");
    assert_eq!(Some(text), highlight_text(&h.state));

    // Clicking the row again clears it.
    h.click_at(x, y);
    assert_eq!(h.state.highlight_ext(), None);
    h.frame();
    assert_eq!(h.widget::<TreemapView>(TREEMAP_ID).highlighted_rects(), 0);
    assert_eq!(h.label_text(STATUS_HIGHLIGHT_ID), "");

    // Escape clears it too.
    h.click_at(x, y);
    assert_eq!(h.state.highlight_ext(), Some(ext));
    h.key(Key::Escape);
    assert_eq!(h.state.highlight_ext(), None);
}

#[test]
fn files_without_an_extension_have_their_own_legend_row() {
    let h = demo_map("laptop-ssd");
    let rows = legend(&h).rows();
    let none = rows
        .iter()
        .position(|r| r.name.is_empty())
        .expect("a no-extension row");
    assert_eq!(
        legend(&h).cell_text(none, Column::Extension).as_deref(),
        Some(NO_EXTENSION)
    );
}

#[test]
fn selecting_around_the_map_keeps_the_tree_small() {
    // Folders the tree opened for an earlier selection are closed and their rows dropped,
    // so the tree holds about one path's worth of folders, whatever was clicked before.
    const MOST_ROWS: usize = 1500;
    let mut h = demo_map("dev-workstation");
    let mut peak = 0;
    let mut selected = 0;
    for yi in 0..6 {
        for xi in 0..8 {
            let (x, y) = h.point_in(TREEMAP_ID, xi as f64 / 8.0 + 0.05, yi as f64 / 6.0 + 0.05);
            h.click_at(x, y);
            if let Some(node) = h.state.selected() {
                selected += 1;
                // Is it, and every folder above it, among its parent's listed children?
                let listed = h
                    .state
                    .with_result(|r, _| {
                        std::iter::once(node)
                            .chain(r.tree.ancestors(node))
                            .all(|n| {
                                let parent = r.tree.node(n).map_or(NodeId::NONE, |x| x.parent());
                                parent.is_none()
                                    || r.tree.children(parent).take(CHILD_CAP).any(|c| c == n)
                            })
                    })
                    .expect("result");
                // Past a cap, that folder's "N more" row is selected (it has no node).
                let expect = listed.then_some(node);
                assert_eq!(tree(&h).selected_node(), expect, "the tree follows");
            }
            peak = peak.max(tree(&h).node_count());
        }
    }
    assert!(selected > 30, "{selected} selections");
    assert!(
        peak <= MOST_ROWS + 2 * CHILD_CAP + 2 * CHILD_CAP,
        "the tree grew to {peak} rows"
    );
    // Folders opened only to show an earlier selection close again.
    let open = h
        .state
        .with_result(|r, _| {
            (0..r.tree.arena_len() as u32)
                .map(NodeId)
                .filter(|&id| tree(&h).is_expanded(id))
                .count()
        })
        .expect("result");
    let depth = h
        .state
        .with_result(|r, _| r.tree.depth(h.state.selected().expect("selection")))
        .expect("depth");
    assert!(
        open <= depth + 1,
        "{open} folders open for a selection {depth} deep"
    );
}
