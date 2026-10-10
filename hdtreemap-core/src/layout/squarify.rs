//! The two geometric pieces of the treemap layout (`layout/mod.rs` drives them per
//! directory):
//!
//! - [`squarify`] — Bruls, Huizing and van Wijk's squarified treemap: children go into rows
//!   laid along the rectangle's shorter side, and a row takes the next child only while that
//!   does not make its worst aspect ratio worse, so rectangles stay close to square.
//! - [`add_ridge`] — van Wijk and van de Wetering's cushion: adds a parabolic ridge of a
//!   given height over a rectangle to a surface `z = a·x² + b·y² + c·x + d·y`, stored as
//!   `[a, b, c, d]` (the `AddRidge` of WinDirStat's and QDirStat's cushion renderers).

use super::PxRect;

/// Tile `rect` with one rectangle per entry of `sizes`, each with an area proportional to
/// its size, appended to `out` in the order of `sizes`. Sizes should be largest first (the
/// algorithm's assumption; any order still tiles exactly) and non-zero; a zero size gets an
/// empty rectangle. The last rectangle of every row ends exactly on the row's edge and the
/// last row fills what is left, so the result tiles `rect` without gaps.
pub fn squarify(sizes: &[u64], rect: PxRect, out: &mut Vec<PxRect>) {
    let mut rest: u128 = sizes.iter().map(|&s| u128::from(s)).sum();
    let mut free = rect;
    let mut i = 0;
    while i < sizes.len() {
        if rest == 0 || free.w <= 0.0 || free.h <= 0.0 {
            // Nothing left to share (zero sizes) or no room: empty rectangles at the edge.
            out.extend(
                sizes[i..]
                    .iter()
                    .map(|_| PxRect::new(free.x, free.y, 0.0, 0.0)),
            );
            return;
        }
        // Square pixels per byte of what is left, and the side the row is laid along.
        let per_byte = free.w * free.h / rest as f64;
        let wide = free.w >= free.h;
        let side = if wide { free.h } else { free.w };

        let mut row_sum = u128::from(sizes[i]);
        let mut largest = sizes[i];
        let mut smallest = sizes[i];
        let mut worst = worst_ratio(largest, smallest, row_sum, side, per_byte);
        let mut end = i + 1;
        while end < sizes.len() {
            let s = sizes[end];
            let next = worst_ratio(
                largest.max(s),
                smallest.min(s),
                row_sum + u128::from(s),
                side,
                per_byte,
            );
            if next > worst {
                break;
            }
            worst = next;
            row_sum += u128::from(s);
            largest = largest.max(s);
            smallest = smallest.min(s);
            end += 1;
        }

        // How deep the row reaches into the free rectangle; the last row takes all of it.
        let last_row = end == sizes.len();
        let depth_room = if wide { free.w } else { free.h };
        let thickness = if last_row {
            depth_room
        } else {
            (row_sum as f64 * per_byte / side).min(depth_room)
        };
        let mut offset = 0.0;
        let mut placed: u128 = 0;
        for (k, &s) in sizes[i..end].iter().enumerate() {
            placed += u128::from(s);
            // Positions from running totals, so rounding never accumulates; the last
            // rectangle of the row ends exactly at the far edge.
            let to = if i + k + 1 == end {
                side
            } else {
                side * (placed as f64 / row_sum as f64)
            };
            let length = (to - offset).max(0.0);
            out.push(if wide {
                PxRect::new(free.x, free.y + offset, thickness, length)
            } else {
                PxRect::new(free.x + offset, free.y, length, thickness)
            });
            offset = to;
        }
        if wide {
            free.x += thickness;
            free.w -= thickness;
        } else {
            free.y += thickness;
            free.h -= thickness;
        }
        rest -= row_sum;
        i = end;
    }
}

/// The worst aspect ratio (≥ 1) of a row whose sizes total `sum`, with largest `max` and
/// smallest `min`, laid along a side of length `side`.
fn worst_ratio(max: u64, min: u64, sum: u128, side: f64, per_byte: f64) -> f64 {
    let row_area = sum as f64 * per_byte;
    if row_area <= 0.0 || side <= 0.0 || min == 0 {
        return f64::INFINITY;
    }
    let side2 = side * side;
    let area2 = row_area * row_area;
    let max_area = max as f64 * per_byte;
    let min_area = min as f64 * per_byte;
    (side2 * max_area / area2).max(area2 / (side2 * min_area))
}

/// Add a cushion ridge of `height` over `rect` to `surface` (`[a, b, c, d]` of
/// `z = a·x² + b·y² + c·x + d·y`): in each axis a parabola that is zero at the rectangle's
/// edges and peaks mid-way, so its slope at the edges is `±4·height` whatever the size.
/// A rectangle with no width (or height) adds nothing in that axis.
pub fn add_ridge(surface: &mut [f64; 4], rect: PxRect, height: f64) {
    let h4 = 4.0 * height;
    if rect.w > 0.0 {
        let wf = h4 / rect.w;
        surface[2] += wf * (rect.x + rect.right());
        surface[0] -= wf;
    }
    if rect.h > 0.0 {
        let hf = h4 / rect.h;
        surface[3] += hf * (rect.y + rect.bottom());
        surface[1] -= hf;
    }
}
