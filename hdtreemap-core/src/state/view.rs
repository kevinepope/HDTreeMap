//! The treemap view's shared state: zoom root, hovered and selected items, shading, and the
//! counters the views key their caches on. It lives in `AppState` (not in the widget) so
//! the status bar, the breadcrumb and the linked views of M3 (folder tree, extension legend)
//! all read and write the same selection.
//!
//! Three counters tell the widgets what changed:
//!
//! - [`AppState::tree_version`] — the finished tree changed (a new scan, a size-mode
//!   re-sort; later, Move to Trash). The treemap rebuilds its palette and layout.
//! - [`AppState::map_version`] — anything that changes the map's picture: the tree, the
//!   zoom root, the shading. The breadcrumb rebuilds on it.
//! - [`AppState::detail_version`] — anything the status bar shows: hover, selection, and
//!   every map change. Mouse moves bump only this one, so they never re-rasterise the map.
//!
//! [`MapCounters`] count the treemap's layout and raster passes, for `HDTREEMAP_STATS` and
//! for the tests that check hovering does not re-rasterise.

use std::cell::Cell;

use super::AppState;
use crate::layout::AggregateInfo;
use crate::model::NodeId;
use crate::render::Shading;

/// What the pointer is over on the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hover {
    /// The file, folder block, or (for an aggregate) the folder whose leftovers it is.
    pub node: NodeId,
    /// Set when the pointer is over a block of merged small items.
    pub aggregate: Option<AggregateInfo>,
}

/// How often the treemap laid out and rasterised (since the app started).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapCounters {
    pub layouts: u64,
    pub rasters: u64,
}

/// See the module docs.
pub(super) struct ViewState {
    zoom_root: Cell<NodeId>,
    hovered: Cell<Option<Hover>>,
    selected: Cell<Option<NodeId>>,
    shading: Cell<Shading>,
    tree_version: Cell<u64>,
    map_version: Cell<u64>,
    detail_version: Cell<u64>,
    counters: Cell<MapCounters>,
}

impl ViewState {
    pub(super) fn new() -> Self {
        Self {
            zoom_root: Cell::new(NodeId(0)),
            hovered: Cell::new(None),
            selected: Cell::new(None),
            shading: Cell::new(Shading::default()),
            tree_version: Cell::new(0),
            map_version: Cell::new(0),
            detail_version: Cell::new(0),
            counters: Cell::new(MapCounters::default()),
        }
    }
}

fn bump(cell: &Cell<u64>) {
    cell.set(cell.get().wrapping_add(1));
}

impl AppState {
    fn view(&self) -> &ViewState {
        &self.inner.view
    }

    /// The finished tree changed: start over at its root with nothing hovered or selected.
    pub(super) fn reset_view(&self) {
        let view = self.view();
        view.zoom_root.set(NodeId(0));
        view.hovered.set(None);
        view.selected.set(None);
        self.tree_changed();
    }

    /// The tree was re-sorted or edited in place; zoom and selection stay.
    pub(super) fn tree_changed(&self) {
        bump(&self.view().tree_version);
        self.map_changed();
    }

    fn map_changed(&self) {
        bump(&self.view().map_version);
        self.detail_changed();
    }

    fn detail_changed(&self) {
        bump(&self.view().detail_version);
        agg_gui::animation::request_draw();
    }

    pub fn tree_version(&self) -> u64 {
        self.view().tree_version.get()
    }

    pub fn map_version(&self) -> u64 {
        self.view().map_version.get()
    }

    pub fn detail_version(&self) -> u64 {
        self.view().detail_version.get()
    }

    /// The folder the map shows (the scan root unless zoomed in).
    pub fn zoom_root(&self) -> NodeId {
        self.view().zoom_root.get()
    }

    /// Show `node` (a live folder of the finished tree) on the map. Returns whether the zoom
    /// changed. Hover is cleared: the map under the pointer is about to change.
    pub fn set_zoom_root(&self, node: NodeId) -> bool {
        let ok = self
            .with_result(|r, _| {
                r.tree.is_live(node) && r.tree.node(node).is_some_and(|n| n.is_dir())
            })
            .unwrap_or(false);
        if !ok || node == self.zoom_root() {
            return false;
        }
        self.view().zoom_root.set(node);
        self.view().hovered.set(None);
        self.map_changed();
        true
    }

    /// Zoom out one level. Returns whether it did (false at the scan root).
    pub fn zoom_out(&self) -> bool {
        let parent = self
            .with_result(|r, _| r.tree.node(self.zoom_root()).map(|n| n.parent()))
            .flatten();
        match parent {
            Some(parent) if !parent.is_none() => self.set_zoom_root(parent),
            _ => false,
        }
    }

    pub fn hovered(&self) -> Option<Hover> {
        self.view().hovered.get()
    }

    /// Set what the pointer is over (no-op if unchanged).
    pub fn set_hovered(&self, hover: Option<Hover>) {
        if self.hovered() != hover {
            self.view().hovered.set(hover);
            self.detail_changed();
        }
    }

    pub fn selected(&self) -> Option<NodeId> {
        self.view().selected.get()
    }

    /// Select `node` (or clear the selection). No-op if unchanged.
    pub fn set_selected(&self, node: Option<NodeId>) {
        if self.selected() != node {
            self.view().selected.set(node);
            self.detail_changed();
        }
    }

    pub fn shading(&self) -> Shading {
        self.view().shading.get()
    }

    /// Cushion or flat shading (kept across scans).
    pub fn set_shading(&self, shading: Shading) {
        if self.shading() != shading {
            self.view().shading.set(shading);
            self.map_changed();
        }
    }

    pub fn map_counters(&self) -> MapCounters {
        self.view().counters.get()
    }

    /// Count a treemap layout pass.
    pub fn count_layout(&self) {
        let mut c = self.map_counters();
        c.layouts += 1;
        self.view().counters.set(c);
    }

    /// Count a treemap raster pass.
    pub fn count_raster(&self) {
        let mut c = self.map_counters();
        c.rasters += 1;
        self.view().counters.set(c);
    }
}
