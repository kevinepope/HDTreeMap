//! [`HitGrid`]: point → the painted rectangle under it, for the treemap's hover, click and
//! tooltip (`ui/treemap_view.rs`), fast enough to run on every mouse move.
//!
//! The painted rectangles (files, folder blocks, aggregates) tile the layout, so a point
//! lies in exactly one of them. The grid buckets them by [`CELL`]-pixel squares
//! ([`super::bins::Bins`]); a lookup tests only the rectangles touching the point's cell. The
//! enclosing directories come from each rectangle's `parent` index
//! ([`super::TreemapLayout::ancestors`]).

use super::bins::Bins;
use super::{LayoutRect, PxRect};

/// Side of a grid cell, in physical pixels.
pub const CELL: f64 = 32.0;

/// Painted rectangles bucketed by grid cell. See the module docs.
#[derive(Clone, Debug, Default)]
pub struct HitGrid {
    origin_x: f64,
    origin_y: f64,
    cols: usize,
    rows: usize,
    bins: Bins,
}

impl HitGrid {
    /// Index the painted rectangles of `rects`, laid out within `bounds`.
    pub fn build(rects: &[LayoutRect], bounds: PxRect) -> Self {
        let cols = cells_along(bounds.w);
        let rows = cells_along(bounds.h);
        let span = |from: f64, len: f64, origin: f64, count: usize| {
            let first = ((from - origin) / CELL).floor().max(0.0) as usize;
            let last = ((from + len - origin) / CELL).ceil().max(0.0) as usize;
            (first.min(count), last.min(count))
        };
        let bins = Bins::build(rects.len(), cols * rows, |i| {
            let r = &rects[i];
            let (c0, c1, r0, r1) = if r.kind.is_painted() && r.rect.area() > 0.0 {
                let (c0, c1) = span(r.rect.x, r.rect.w, bounds.x, cols);
                let (r0, r1) = span(r.rect.y, r.rect.h, bounds.y, rows);
                (c0, c1, r0, r1)
            } else {
                (0, 0, 0, 0)
            };
            (r0..r1).flat_map(move |row| (c0..c1).map(move |col| row * cols + col))
        });
        Self {
            origin_x: bounds.x,
            origin_y: bounds.y,
            cols,
            rows,
            bins,
        }
    }

    /// The painted rectangle of `rects` (the slice this grid was built from) containing
    /// `(x, y)`.
    pub fn hit(&self, rects: &[LayoutRect], x: f64, y: f64) -> Option<usize> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let col = ((x - self.origin_x) / CELL).floor();
        let row = ((y - self.origin_y) / CELL).floor();
        if col < 0.0 || row < 0.0 || col >= self.cols as f64 || row >= self.rows as f64 {
            return None;
        }
        let cell = row as usize * self.cols + col as usize;
        self.bins
            .cell(cell)
            .iter()
            .map(|&i| i as usize)
            .find(|&i| rects.get(i).is_some_and(|r| r.rect.contains(x, y)))
    }
}

fn cells_along(len: f64) -> usize {
    if len.is_finite() && len > 0.0 {
        (len / CELL).ceil() as usize
    } else {
        0
    }
}
