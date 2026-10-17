//! The cityscape painter ([`Shading::Cityscape`](super::Shading::Cityscape)): the treemap
//! as a city seen from above and a little to the lower right. Every painted rectangle is a
//! building. Its footprint is the rectangle less a street along its right and bottom edges,
//! wider where a folder boundary runs (one step per enclosing folder that ends on that
//! edge), so folders read as city blocks between avenues. Its height grows with the cube
//! root of its size, up to [`max_height`] for the biggest item in view, so the space hogs
//! stand up as towers and the small fry stay low. Buildings are drawn in an oblique
//! projection: the roof is the footprint moved by [`RISE`] per pixel of height (up and a
//! little to the left), and the right and bottom walls fill in below it, shaded darker as
//! they face away from the light. A per-band depth buffer settles which building shows at
//! a pixel, so the bands stay independent and the paint order does not matter.
//!
//! Only the pixels are 3D: hit testing, hover and the outlines still use the footprint.

use super::palette::Rgb;
use super::raster::{to_byte, Band, FLAT_INTENSITY};
use crate::layout::{pixel_span, TreemapLayout, NO_PARENT};
use crate::model::Tree;

/// Where a roof moves per pixel of height: half a pixel left and a full pixel up.
pub const RISE: [f32; 2] = [-0.5, -1.0];
/// The tallest building is this share of the raster's shorter side, within the limits.
const HEIGHT_SHARE: f32 = 0.1;
/// The tallest building is never shorter than this (a small window still shows relief)…
pub const MIN_MAX_HEIGHT: f32 = 8.0;
/// …nor taller than this (a 4K map does not become a forest of spires).
pub const MAX_MAX_HEIGHT: f32 = 56.0;
/// Footprints narrower than this on either side stay flat: ground tiles, not needles.
const FLAT_BELOW: f32 = 2.0;
/// A plain street between neighbours, in pixels.
pub const STREET: f32 = 1.0;
/// Extra street width per folder boundary that runs along an edge.
pub const STREET_PER_FOLDER: f32 = 1.5;
/// A street never gets wider than this…
const STREET_MAX: f32 = 6.0;
/// …nor takes more than this share of a rectangle's side.
const STREET_SHARE: f32 = 0.4;
/// Wall intensities relative to the roof: the right wall is lit a little by the ambient
/// light, the bottom wall is in shadow.
pub const RIGHT_WALL: f32 = 0.7;
pub const BOTTOM_WALL: f32 = 0.5;

/// One building: its footprint in raster pixels (the rectangle less its streets) and its
/// height in pixels of rise.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Building {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub height: f32,
}

impl Building {
    /// The pixel rows the building's projection covers, clamped to `0..limit`.
    pub fn pixel_rows(&self, limit: u32) -> (u32, u32) {
        let top = self.y0 + self.height * RISE[1];
        pixel_span(f64::from(top), f64::from(self.y1), limit)
    }

    /// The pixel columns the building's projection covers, clamped to `0..limit`.
    pub fn pixel_cols(&self, limit: u32) -> (u32, u32) {
        let left = self.x0 + self.height * RISE[0];
        pixel_span(f64::from(left), f64::from(self.x1), limit)
    }
}

/// The height of the biggest item in a raster of this size.
pub fn max_height(width: u32, height: u32) -> f32 {
    (width.min(height) as f32 * HEIGHT_SHARE).clamp(MIN_MAX_HEIGHT, MAX_MAX_HEIGHT)
}

/// A building for each of `leaves` (indices of painted rectangles in `layout.rects()`), in
/// the same order.
pub fn plan(
    layout: &TreemapLayout,
    tree: &Tree,
    leaves: &[usize],
    width: u32,
    height: u32,
) -> Vec<Building> {
    let rects = layout.rects();
    let size_of = |i: usize| match layout.aggregate(i) {
        Some(merged) => merged.bytes,
        None => rects.get(i).map_or(0, |r| tree.size(r.node)),
    };
    let biggest = leaves.iter().map(|&i| size_of(i)).max().unwrap_or(0).max(1) as f64;
    let tallest = max_height(width, height);
    leaves
        .iter()
        .filter_map(|&i| rects.get(i).map(|r| (i, r)))
        .map(|(i, r)| {
            // Every enclosing folder that ends on an edge widens that edge's street; the
            // zoom root's edges are the map's and get none.
            let (mut right, mut bottom) = (STREET, STREET);
            for a in layout.ancestors(i) {
                let Some(folder) = rects.get(a) else { break };
                if folder.parent == NO_PARENT {
                    break;
                }
                if (folder.rect.right() - r.rect.right()).abs() < 1e-6 {
                    right += STREET_PER_FOLDER;
                }
                if (folder.rect.bottom() - r.rect.bottom()).abs() < 1e-6 {
                    bottom += STREET_PER_FOLDER;
                }
            }
            let (w, h) = (r.rect.w as f32, r.rect.h as f32);
            let right = right.min(STREET_MAX).min(w * STREET_SHARE);
            let bottom = bottom.min(STREET_MAX).min(h * STREET_SHARE);
            let (x0, y0) = (r.rect.x as f32, r.rect.y as f32);
            let (x1, y1) = (x0 + w - right, y0 + h - bottom);
            let height = if (x1 - x0).min(y1 - y0) < FLAT_BELOW {
                0.0
            } else {
                tallest * (size_of(i) as f64 / biggest).cbrt() as f32
            };
            Building {
                x0,
                y0,
                x1,
                y1,
                height,
            }
        })
        .collect()
}

/// The depth of a point `height` above ground point `(x, y)`: larger is nearer the viewer.
/// Points that land on the same pixel lie on one line of sight, along which this grows.
fn depth_key(x: f32, y: f32, height: f32) -> f32 {
    -RISE[0] * x - RISE[1] * y + height
}

/// Paint `building` in `color` into the band's rows, keeping whatever is nearer in `depth`
/// (one entry per band pixel, `NEG_INFINITY` where nothing has been painted).
pub(super) fn paint(mut band: Band<'_>, building: &Building, color: Rgb, depth: &mut [f32]) {
    let (cx0, cx1) = building.pixel_cols(band.width);
    let (ry0, ry1) = building.pixel_rows(band.height);
    let y0 = ry0.max(band.first_row);
    let y1 = ry1.min(band.first_row + band.rows);
    if cx0 >= cx1 || y0 >= y1 {
        return;
    }
    let roof = color.map(|c| to_byte(c * FLAT_INTENSITY));
    let right_wall = color.map(|c| to_byte(c * FLAT_INTENSITY * RIGHT_WALL));
    let bottom_wall = color.map(|c| to_byte(c * FLAT_INTENSITY * BOTTOM_WALL));
    let (ox, oy) = (building.height * RISE[0], building.height * RISE[1]);
    let b = building;
    for y in y0..y1 {
        let py = y as f32 + 0.5;
        let from = ((y - band.first_row) * band.width + cx0) as usize;
        let to = ((y - band.first_row) * band.width + cx1) as usize;
        let row = band.row(y, cx0, cx1);
        let depths = &mut depth[from..to];
        for (i, (px, d)) in row
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(depths)
            .enumerate()
        {
            let sx = (cx0 + i as u32) as f32 + 0.5;
            let on_roof = sx >= b.x0 + ox && sx < b.x1 + ox && py >= b.y0 + oy && py < b.y1 + oy;
            let (gx, gy, z, rgb) = if on_roof {
                (sx - ox, py - oy, b.height, roof)
            } else if b.height <= 0.0 {
                continue;
            } else {
                // The right wall sweeps the footprint's right edge up along the rise; a
                // pixel on it is `s` of the way up. The bottom wall likewise.
                let s = (sx - b.x1) / ox;
                let gy = py - s * oy;
                if (0.0..1.0).contains(&s) && gy >= b.y0 && gy < b.y1 {
                    (b.x1, gy, s * b.height, right_wall)
                } else {
                    let s = (py - b.y1) / oy;
                    let gx = sx - s * ox;
                    if (0.0..1.0).contains(&s) && gx >= b.x0 && gx < b.x1 {
                        (gx, b.y1, s * b.height, bottom_wall)
                    } else {
                        continue;
                    }
                }
            };
            let key = depth_key(gx, gy, z);
            if key > *d {
                *d = key;
                px[..3].copy_from_slice(&rgb);
                px[3] = 255;
            }
        }
    }
}
