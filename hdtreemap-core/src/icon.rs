//! The app icon, drawn by the app's own treemap renderer: a small made-up tree (a few
//! folders of differently typed files) laid out by `layout` and cushion-shaded by
//! `render::rasterise`, inside a dark rounded square.
//!
//! The native shell rasterises it at start-up for the window icon
//! (`ShellConfig::with_icon_rgba`), and `examples/make_icons.rs` writes the release
//! assets from it (`assets/icon/`: PNGs, the Windows `.ico`, the macOS `.icns`), so the
//! icon and the app always look alike.

use crate::layout::{layout, LayoutParams, PxRect};
use crate::model::{ExtTable, NodeFlags, Sizes, Tree, TreeBuilder};
use crate::render::{rasterise, Palette, Shading};

/// Transparent margin around the rounded square, as a share of the size, for the macOS
/// icon grid (824 px of art in 1024).
pub const MAC_MARGIN: f64 = 100.0 / 1024.0;
/// The margin for window, taskbar and file icons, which want the art to fill the square.
pub const TIGHT_MARGIN: f64 = 0.02;

/// The dark frame around the map.
const FRAME: [u8; 3] = [30, 33, 38];
/// Corner radius of the rounded square, as a share of its side (the macOS shape).
const RADIUS: f64 = 0.225;
/// Width of the frame around the map, as a share of the square's side.
const INSET: f64 = 0.065;

/// The made-up tree the icon shows: (folder, [(file name, size)]). Extensions are ranked
/// by total size, which picks their hues (blue, red, green, yellow, ...).
const FOLDERS: &[(&str, &[(&str, u64)])] = &[
    (
        "videos",
        &[("trip.mp4", 300), ("talk.mp4", 120), ("clip.mp4", 60)],
    ),
    (
        "disk",
        &[("os.iso", 230), ("tools.zip", 70), ("old.zip", 45)],
    ),
    (
        "photos",
        &[
            ("a.jpg", 60),
            ("b.jpg", 45),
            ("c.jpg", 36),
            ("d.jpg", 28),
            ("e.jpg", 22),
            ("f.jpg", 16),
            ("g.jpg", 12),
            ("h.jpg", 9),
        ],
    ),
    (
        "code",
        &[
            ("app.js", 40),
            ("lib.js", 26),
            ("ui.js", 18),
            ("doc.pdf", 30),
            ("spec.pdf", 16),
            ("x.js", 10),
            ("y.js", 7),
        ],
    ),
];

/// The icon's tree, sorted for layout.
fn tree() -> Tree {
    let mut builder = TreeBuilder::new("icon");
    let root = builder.root();
    for (folder, files) in FOLDERS {
        let dir = builder.add_dir(root, folder);
        for (name, size) in *files {
            let bytes = size * 1_000_000;
            builder.add_file(dir, name, Sizes::new(bytes, bytes), NodeFlags::empty());
        }
    }
    builder.finish_sorted()
}

/// Signed distance from `(x, y)` to a rounded square `[x0, x0 + side]²` with corner
/// radius `r`: negative inside.
fn rounded_square_distance(x: f64, y: f64, x0: f64, side: f64, r: f64) -> f64 {
    let half = side / 2.0;
    let centre = x0 + half;
    let qx = (x - centre).abs() - (half - r);
    let qy = (y - centre).abs() - (half - r);
    let outside = qx.max(0.0).hypot(qy.max(0.0));
    outside + qx.max(qy).min(0.0) - r
}

/// Coverage (`0..=1`) of the pixel centred at `(x, y)` by an edge at signed distance `d`.
fn coverage(d: f64) -> f64 {
    (0.5 - d).clamp(0.0, 1.0)
}

/// The icon as `size × size` RGBA (top row first, straight alpha), with `margin` (a share
/// of `size`, [`MAC_MARGIN`] or [`TIGHT_MARGIN`]) left transparent all round.
pub fn rgba(size: u32, margin: f64) -> Vec<u8> {
    let s = f64::from(size);
    let x0 = (s * margin).round();
    let side = s - 2.0 * x0;
    let radius = side * RADIUS;
    let inset = (side * INSET).round().max(1.0);
    let map_x0 = x0 + inset;
    let map_side = side - 2.0 * inset;
    let map_radius = (radius - inset).max(0.0);

    let tree = tree();
    let map_px = map_side.max(1.0) as u32;
    let bounds = PxRect::new(0.0, 0.0, f64::from(map_px), f64::from(map_px));
    let map_layout = layout(&tree, tree.root(), bounds, &LayoutParams::default());
    let table = ExtTable::from_tree(&tree);
    let palette = Palette::from_extensions(&table, tree.size_mode(), tree.ext_count());
    let map = rasterise(
        &map_layout,
        &tree,
        &palette,
        Shading::Cushion,
        map_px,
        map_px,
    );

    let mut out = vec![0u8; (size * size * 4) as usize];
    for py in 0..size {
        for px in 0..size {
            let (x, y) = (f64::from(px) + 0.5, f64::from(py) + 0.5);
            let frame = coverage(rounded_square_distance(x, y, x0, side, radius));
            if frame <= 0.0 {
                continue;
            }
            // Within the frame: the map where it covers, the frame colour elsewhere.
            let d_map = rounded_square_distance(x, y, map_x0, map_side, map_radius);
            let inner = coverage(d_map);
            let (mx, my) = (px as i64 - map_x0 as i64, py as i64 - map_x0 as i64);
            let map_rgb = if inner > 0.0
                && (0..i64::from(map_px)).contains(&mx)
                && (0..i64::from(map_px)).contains(&my)
            {
                let i = (my as usize * map_px as usize + mx as usize) * 4;
                [map[i], map[i + 1], map[i + 2]]
            } else {
                FRAME
            };
            let o = ((py * size + px) * 4) as usize;
            for c in 0..3 {
                let v = f64::from(map_rgb[c]) * inner + f64::from(FRAME[c]) * (1.0 - inner);
                out[o + c] = v.round() as u8;
            }
            out[o + 3] = (frame * 255.0).round() as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_is_a_coloured_rounded_square() {
        let size = 128;
        let img = rgba(size, MAC_MARGIN);
        assert_eq!(img.len(), (size * size * 4) as usize);
        let px = |x: u32, y: u32| {
            let i = ((y * size + x) * 4) as usize;
            [img[i], img[i + 1], img[i + 2], img[i + 3]]
        };
        // The corners are transparent (margin and rounded corner).
        assert_eq!(px(0, 0)[3], 0);
        assert_eq!(px(16, 16)[3], 0);
        // The middle is opaque and coloured (the map), not the grey frame.
        let mid = px(size / 2, size / 2);
        assert_eq!(mid[3], 255);
        let spread = mid[..3].iter().max().unwrap_or(&0) - mid[..3].iter().min().unwrap_or(&0);
        assert!(spread > 40, "{mid:?}");
        // A tight icon fills more of the square.
        let tight = rgba(size, TIGHT_MARGIN);
        let opaque = |img: &[u8]| {
            img.as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[3] == 255)
                .count()
        };
        assert!(opaque(&tight) > opaque(&img));
    }
}
