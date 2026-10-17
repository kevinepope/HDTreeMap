//! Treemap layout (`docs/PLAN.md` §3.3): turns the scanned [`Tree`] below a zoom root into
//! a flat list of rectangles, pure code with no drawing.
//!
//! - [`squarify`] — the squarified treemap of Bruls, Huizing and van Wijk (2000) for one
//!   directory's children, and the cushion ridges of van Wijk and van de Wetering (1999).
//! - [`bins`] — a compact rect → grid-cell index, shared by hit testing here and by the
//!   rasteriser's row bands (`render/`).
//! - [`hit`] — point → deepest rectangle, and the chain of enclosing directories.
//!
//! Coordinates are **physical pixels, Y down** (row 0 at the top), the layout of the RGBA
//! raster `render/` fills: `TreemapView` (`ui/treemap_view.rs`) maps agg-gui's Y-up logical
//! coordinates into this space for hit testing and back out for its outlines.
//!
//! [`layout`] walks the tree from the zoom root. Every directory becomes a [`RectKind::Dir`]
//! rectangle that its children tile exactly; recursion stops at files, and at children whose
//! share of the parent is smaller than [`LayoutParams::min_area`]: those are merged into one
//! [`RectKind::Aggregate`] block per directory (or, when nothing in a directory is big
//! enough, the directory itself is drawn as one [`RectKind::FolderBlock`]), so every byte is
//! still accounted for. The painted rectangles (everything but `Dir`) tile the root exactly.
//!
//! Each rectangle carries its cushion surface `z = a·x² + b·y² + c·x + d·y`, stored as
//! `[a, b, c, d]` (the coefficient order WinDirStat and QDirStat use): the sum of one
//! parabolic ridge per nesting level, each [`CushionParams::falloff`] times lower than its
//! parent's. The rasteriser lights it per pixel.

pub mod bins;
pub mod hit;
pub mod squarify;

use crate::model::{NodeId, Tree};

pub use hit::HitGrid;
pub use squarify::{add_ridge, squarify};

/// "No rectangle": the parent index of the root rectangle.
pub const NO_PARENT: u32 = u32::MAX;

/// An axis-aligned rectangle in physical pixels, Y down.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PxRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl PxRect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    pub fn area(&self) -> f64 {
        self.w.max(0.0) * self.h.max(0.0)
    }

    /// Half-open containment (`x <= px < right`), so neighbours that share an edge never
    /// both claim a point on it.
    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }

    /// The pixel columns `[first, end)` whose centres fall inside, clamped to `0..limit`.
    pub fn pixel_cols(&self, limit: u32) -> (u32, u32) {
        pixel_span(self.x, self.right(), limit)
    }

    /// The pixel rows `[first, end)` whose centres fall inside, clamped to `0..limit`.
    pub fn pixel_rows(&self, limit: u32) -> (u32, u32) {
        pixel_span(self.y, self.bottom(), limit)
    }
}

/// Pixels whose centre `i + 0.5` lies in `[from, to)`. Adjacent spans never overlap and leave
/// no gap, which is what makes the painted rectangles tile the raster exactly.
pub(crate) fn pixel_span(from: f64, to: f64, limit: u32) -> (u32, u32) {
    let first = (from - 0.5).ceil().clamp(0.0, f64::from(limit));
    let end = (to - 0.5).ceil().clamp(0.0, f64::from(limit));
    (first as u32, (end as u32).max(first as u32))
}

/// What a rectangle stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RectKind {
    /// A directory whose children tile it; not painted itself.
    Dir,
    /// A file.
    File,
    /// A directory drawn as one block: none of its contents is big enough to show.
    FolderBlock,
    /// The merged leftovers of a directory (`node`): children too small to show apart.
    Aggregate,
}

impl RectKind {
    /// Whether the rasteriser paints it (everything but an interior directory).
    pub fn is_painted(self) -> bool {
        self != RectKind::Dir
    }
}

/// One laid-out node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutRect {
    /// The node; for an [`RectKind::Aggregate`], the directory whose leftovers it holds.
    pub node: NodeId,
    pub rect: PxRect,
    /// Cushion surface `[a, b, c, d]` of `z = a·x² + b·y² + c·x + d·y` (see module docs).
    pub cushion: [f32; 4],
    /// Nesting below the zoom root (the root is 0).
    pub depth: u16,
    pub kind: RectKind,
    /// Index of the enclosing [`RectKind::Dir`] rectangle; [`NO_PARENT`] for the root.
    pub parent: u32,
}

/// What an [`RectKind::Aggregate`] block merged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AggregateInfo {
    /// How many children were merged.
    pub count: u32,
    /// Their total size in the active mode.
    pub bytes: u64,
}

/// The cushion surface parameters (van Wijk and van de Wetering 1999).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CushionParams {
    /// Ridge height of the zoom root's cushion. The slope at a cushion's edge is `4·height`
    /// whatever its size.
    pub height: f64,
    /// Each nesting level's ridge is this fraction of its parent's (the paper's `f`).
    pub falloff: f64,
}

impl Default for CushionParams {
    fn default() -> Self {
        Self {
            height: 0.4,
            falloff: 0.75,
        }
    }
}

/// Layout tuning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutParams {
    /// Children whose rectangle would be smaller than this many square pixels are merged
    /// into their directory's aggregate block instead of being laid out.
    pub min_area: f64,
    pub cushion: CushionParams,
}

impl Default for LayoutParams {
    fn default() -> Self {
        Self {
            min_area: 1.0,
            cushion: CushionParams::default(),
        }
    }
}

/// A laid-out treemap: the rectangles in pre-order (a directory before its contents), what
/// each aggregate merged, and a grid for hit testing.
#[derive(Clone, Debug)]
pub struct TreemapLayout {
    rects: Vec<LayoutRect>,
    /// `(rect index, info)` for every aggregate, by index.
    aggregates: Vec<(u32, AggregateInfo)>,
    bounds: PxRect,
    root: NodeId,
    grid: HitGrid,
}

impl TreemapLayout {
    /// Every rectangle, in pre-order. Empty when the zoom root has no size.
    pub fn rects(&self) -> &[LayoutRect] {
        &self.rects
    }

    /// The rectangle laid out into.
    pub fn bounds(&self) -> PxRect {
        self.bounds
    }

    /// The zoom root this layout starts from.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Number of painted rectangles (files, folder blocks, aggregates).
    pub fn painted_count(&self) -> usize {
        self.rects.iter().filter(|r| r.kind.is_painted()).count()
    }

    /// What the aggregate at `index` merged (`None` if it is not an aggregate).
    pub fn aggregate(&self, index: usize) -> Option<AggregateInfo> {
        let index = u32::try_from(index).ok()?;
        self.aggregates
            .binary_search_by_key(&index, |(i, _)| *i)
            .ok()
            .map(|at| self.aggregates[at].1)
    }

    /// The painted rectangle under `(x, y)` (physical pixels, Y down), if any.
    pub fn hit(&self, x: f64, y: f64) -> Option<usize> {
        self.grid.hit(&self.rects, x, y)
    }

    /// Indices of the directories enclosing rectangle `index`, nearest first, ending at the
    /// root.
    pub fn ancestors(&self, index: usize) -> impl Iterator<Item = usize> + '_ {
        let mut next = self.rects.get(index).map_or(NO_PARENT, |r| r.parent);
        std::iter::from_fn(move || {
            if next == NO_PARENT {
                return None;
            }
            let here = next as usize;
            next = self.rects.get(here).map_or(NO_PARENT, |r| r.parent);
            Some(here)
        })
    }

    /// The rectangle showing `node` itself (a directory, folder block or file; not an
    /// aggregate that merely contains it).
    pub fn index_of(&self, node: NodeId) -> Option<usize> {
        self.rects
            .iter()
            .position(|r| r.node == node && r.kind != RectKind::Aggregate)
    }

    /// The rectangle showing `node`, or else its nearest ancestor that has one (a file
    /// merged into an aggregate shows as its folder). `None` when `node` is not below the
    /// layout's root. One pass over the rectangles.
    pub fn index_of_nearest(&self, tree: &Tree, node: NodeId) -> Option<usize> {
        let mut chain = vec![node];
        if node != self.root {
            for a in tree.ancestors(node) {
                chain.push(a);
                if a == self.root {
                    break;
                }
            }
            if chain.last() != Some(&self.root) {
                return None;
            }
        }
        let mut best: Option<(usize, usize)> = None;
        for (index, r) in self.rects.iter().enumerate() {
            if r.kind == RectKind::Aggregate {
                continue;
            }
            if let Some(level) = chain.iter().position(|&c| c == r.node) {
                if best.is_none_or(|(l, _)| level < l) {
                    best = Some((level, index));
                    if level == 0 {
                        break;
                    }
                }
            }
        }
        best.map(|(_, index)| index)
    }
}

/// Lay out the subtree of `root` (by the tree's active size mode) into `bounds`.
pub fn layout(tree: &Tree, root: NodeId, bounds: PxRect, params: &LayoutParams) -> TreemapLayout {
    let mut builder = Builder {
        tree,
        params,
        rects: Vec::new(),
        aggregates: Vec::new(),
    };
    let usable = bounds.w > 0.0 && bounds.h > 0.0 && bounds.w.is_finite() && bounds.h.is_finite();
    if usable && tree.is_live(root) && tree.size(root) > 0 {
        builder.lay(root, bounds, [0.0; 4], params.cushion.height, 0, NO_PARENT);
    }
    let Builder {
        rects, aggregates, ..
    } = builder;
    let grid = HitGrid::build(&rects, bounds);
    TreemapLayout {
        rects,
        aggregates,
        bounds,
        root,
        grid,
    }
}

/// One child to place: a node, or a directory's merged leftovers.
#[derive(Clone, Copy)]
enum Item {
    Node(NodeId),
    Leftovers(AggregateInfo),
}

struct Builder<'a> {
    tree: &'a Tree,
    params: &'a LayoutParams,
    rects: Vec<LayoutRect>,
    aggregates: Vec<(u32, AggregateInfo)>,
}

impl Builder<'_> {
    /// Lay out `node` into `rect`. `surface` is the parent's cushion, `height` this level's
    /// ridge height.
    fn lay(
        &mut self,
        node: NodeId,
        rect: PxRect,
        surface: [f64; 4],
        height: f64,
        depth: u16,
        parent: u32,
    ) {
        let mut surface = surface;
        add_ridge(&mut surface, rect, height);
        let index = self.rects.len() as u32;
        let is_dir = self.tree.node(node).is_some_and(|n| n.is_dir());
        let kind = if is_dir {
            RectKind::Dir
        } else {
            RectKind::File
        };
        self.rects.push(LayoutRect {
            node,
            rect,
            cushion: surface.map(|c| c as f32),
            depth,
            kind,
            parent,
        });
        if !is_dir {
            return;
        }

        // Children big enough to show, largest first; the rest merged. One walk of the
        // child list (it is linked through the arena, so each step can miss the cache).
        let children: Vec<(u64, NodeId)> = self
            .tree
            .children(node)
            .map(|c| (self.tree.size(c), c))
            .filter(|(size, _)| *size > 0)
            .collect();
        // u128: a folder's children can add up past u64 (its own size saturates).
        let total: u128 = children.iter().map(|&(size, _)| u128::from(size)).sum();
        if total == 0 {
            self.rects[index as usize].kind = RectKind::FolderBlock;
            return;
        }
        let per_byte = rect.area() / total as f64;
        let mut items: Vec<(u64, Item)> = Vec::new();
        let mut leftovers = AggregateInfo::default();
        for (size, child) in children {
            if size as f64 * per_byte < self.params.min_area {
                leftovers.count = leftovers.count.saturating_add(1);
                leftovers.bytes = leftovers.bytes.saturating_add(size);
            } else {
                items.push((size, Item::Node(child)));
            }
        }
        if items.is_empty() {
            self.rects[index as usize].kind = RectKind::FolderBlock;
            return;
        }
        // Stable, so equal sizes keep the tree's (name) order: layouts are deterministic.
        items.sort_by_key(|item| std::cmp::Reverse(item.0));
        if leftovers.count > 0 {
            let at = items.partition_point(|(size, _)| *size >= leftovers.bytes);
            items.insert(at, (leftovers.bytes, Item::Leftovers(leftovers)));
        }

        let sizes: Vec<u64> = items.iter().map(|(size, _)| *size).collect();
        let mut placed = Vec::with_capacity(sizes.len());
        squarify(&sizes, rect, &mut placed);
        let child_height = height * self.params.cushion.falloff;
        let child_depth = depth.saturating_add(1);
        for ((_, item), child_rect) in items.iter().zip(placed) {
            match *item {
                Item::Node(child) => {
                    self.lay(child, child_rect, surface, child_height, child_depth, index);
                }
                Item::Leftovers(info) => {
                    let mut leaf = surface;
                    add_ridge(&mut leaf, child_rect, child_height);
                    self.aggregates.push((self.rects.len() as u32, info));
                    self.rects.push(LayoutRect {
                        node,
                        rect: child_rect,
                        cushion: leaf.map(|c| c as f32),
                        depth: child_depth,
                        kind: RectKind::Aggregate,
                        parent: index,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
