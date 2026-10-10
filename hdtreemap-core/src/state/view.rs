//! The treemap view's shared state: zoom root, hovered and selected items, the highlighted
//! extension, shading, the splitter positions, and the counters the views key their caches
//! on. It lives in `AppState` (not in a widget) so the map, the status bar, the breadcrumb,
//! the folder tree and the extension legend all read and write the same selection. The
//! palette and the zoom root's extension table, derived from the tree and cached for every
//! view to share, are in `state/derived.rs`.
//!
//! Three counters tell the widgets what changed:
//!
//! - [`AppState::tree_version`] — the finished tree changed (a new scan, a size-mode
//!   re-sort; later, Move to Trash). The treemap rebuilds its palette and layout.
//! - [`AppState::map_version`] — anything that changes the map's picture: the tree, the
//!   zoom root, the shading, the colour scheme. The breadcrumb rebuilds on it.
//! - [`AppState::detail_version`] — anything the status bar shows: hover, selection, the
//!   highlighted extension, and every map change. Mouse moves bump only this one, so they
//!   never re-rasterise the map.
//!
//! [`MapCounters`] count the treemap's layout and raster passes, for `HDTREEMAP_STATS` and
//! for the tests that check hovering does not re-rasterise.

use std::cell::{Cell, RefCell};

use agg_gui::SplitterRatio;

use super::derived::DerivedCache;
use super::AppState;
use crate::layout::AggregateInfo;
use crate::model::{NodeId, SizeMode};
use crate::render::{ColorBy, Shading};

/// Default share of the window width for the left column (folder tree and legend).
pub const SIDE_SPLIT_DEFAULT: f64 = 0.28;
/// Default share of the left column's height for the folder tree (the legend gets the rest).
pub const TREE_SPLIT_DEFAULT: f64 = 0.6;

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
    highlight_ext: Cell<Option<u16>>,
    /// Extension name to highlight when the next scan finishes (`--highlight`).
    pending_highlight: RefCell<Option<String>>,
    shading: Cell<Shading>,
    size_mode: Cell<SizeMode>,
    color_by: Cell<ColorBy>,
    side_split: SplitterRatio,
    tree_split: SplitterRatio,
    pub(super) derived: DerivedCache,
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
            highlight_ext: Cell::new(None),
            pending_highlight: RefCell::new(None),
            shading: Cell::new(Shading::default()),
            size_mode: Cell::new(SizeMode::default()),
            color_by: Cell::new(ColorBy::default()),
            side_split: SplitterRatio::new(SIDE_SPLIT_DEFAULT),
            tree_split: SplitterRatio::new(TREE_SPLIT_DEFAULT),
            derived: DerivedCache::default(),
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

    /// A new scan starts: show its root with nothing hovered, selected or highlighted.
    pub(super) fn reset_view(&self) {
        let view = self.view();
        view.zoom_root.set(NodeId(0));
        view.hovered.set(None);
        view.selected.set(None);
        view.highlight_ext.set(None);
        self.tree_changed();
    }

    /// The scan finished. Its tree is the one the live map showed, sorted, so the zoom,
    /// selection and highlight made while it ran stay; an extension asked for with
    /// [`AppState::highlight_on_finish`] is highlighted now.
    pub(super) fn finish_view(&self) {
        let pending = self.view().pending_highlight.borrow_mut().take();
        let ext = pending.and_then(|name| {
            self.with_result(|_, exts| exts.get(&name).map(|row| row.ext))
                .flatten()
        });
        if ext.is_some() {
            self.view().highlight_ext.set(ext);
        }
        self.tree_changed();
    }

    pub(super) fn view_size_mode(&self) -> SizeMode {
        self.view().size_mode.get()
    }

    pub(super) fn set_view_size_mode(&self, mode: SizeMode) {
        self.view().size_mode.set(mode);
    }

    /// The tree was re-sorted or edited in place; zoom and selection stay.
    pub(super) fn tree_changed(&self) {
        bump(&self.view().tree_version);
        self.map_changed();
    }

    /// The look flipped between light and dark: the map's greys change with it.
    pub(super) fn look_changed(&self) {
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

    /// The extension whose files the map highlights (the legend's chosen row).
    pub fn highlight_ext(&self) -> Option<u16> {
        self.view().highlight_ext.get()
    }

    /// Highlight the files of extension id `ext` (or clear it). Ids the finished tree does
    /// not have are ignored. No-op if unchanged.
    pub fn set_highlight_ext(&self, ext: Option<u16>) {
        let ok = ext.is_none_or(|e| {
            self.with_result(|r, _| usize::from(e) < r.tree.ext_count())
                .unwrap_or(false)
        });
        if ok && self.highlight_ext() != ext {
            self.view().highlight_ext.set(ext);
            self.detail_changed();
        }
    }

    /// Highlight `ext`, or clear the highlight if it is the highlighted one already (a
    /// second click on the same legend row).
    pub fn toggle_highlight_ext(&self, ext: u16) {
        let next = if self.highlight_ext() == Some(ext) {
            None
        } else {
            Some(ext)
        };
        self.set_highlight_ext(next);
    }

    /// Highlight extension `name` (lower case, no dot) once the next scan finishes.
    pub fn highlight_on_finish(&self, name: Option<String>) {
        *self.view().pending_highlight.borrow_mut() = name;
    }

    /// Where the divider between the left column and the map sits (kept for the session:
    /// the main view hands the same handle to every `Splitter` it builds).
    pub fn side_split(&self) -> SplitterRatio {
        self.view().side_split.clone()
    }

    /// Where the divider between the folder tree and the extension legend sits.
    pub fn tree_split(&self) -> SplitterRatio {
        self.view().tree_split.clone()
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

    /// What the map colours by (kept across scans).
    pub fn color_by(&self) -> ColorBy {
        self.view().color_by.get()
    }

    /// Colour the map by file type or by top-level folder. The map re-rasterises and the
    /// legend lists extensions or top folders.
    pub fn set_color_by(&self, color_by: ColorBy) {
        if self.color_by() != color_by {
            self.view().color_by.set(color_by);
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
