//! Golden-invariant tests for the rasteriser on a small fixed tree at 64×48: every
//! rectangle shows its palette colour, cushions are brighter toward the upper left, flat mode
//! fills evenly with darker edges, and the palette's hue assignment. Pixel invariants rather
//! than a stored image, so harmless tuning does not break them.

use super::*;
use crate::layout::{layout, LayoutParams, LayoutRect, PxRect, RectKind, TreemapLayout};
use crate::model::{ExtTable, NodeFlags, SizeMode, Sizes, Tree, TreeBuilder};

const W: u32 = 64;
const H: u32 = 48;

/// /r: movie.mp4 5000, photos/ (a.jpg 1500, b.jpg 1000), notes.txt 600, misc 400 (no ext)
fn fixture() -> (Tree, Palette, TreemapLayout) {
    let mut b = TreeBuilder::new("/r");
    let root = b.root();
    let add = |b: &mut TreeBuilder, parent, name: &str, size| {
        b.add_file(parent, name, Sizes::new(size, size), NodeFlags::empty());
    };
    add(&mut b, root, "movie.mp4", 5000);
    let photos = b.add_dir(root, "photos");
    add(&mut b, photos, "a.jpg", 1500);
    add(&mut b, photos, "b.jpg", 1000);
    add(&mut b, root, "notes.txt", 600);
    add(&mut b, root, "misc", 400);
    let tree = b.finish(SizeMode::Allocated);
    let table = ExtTable::from_tree(&tree);
    let palette = Palette::from_extensions(&table, SizeMode::Allocated, tree.ext_count());
    let bounds = PxRect::new(0.0, 0.0, f64::from(W), f64::from(H));
    let map = layout(&tree, tree.root(), bounds, &LayoutParams::default());
    (tree, palette, map)
}

fn pixel(buf: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
}

fn luma(p: [u8; 4]) -> u32 {
    u32::from(p[0]) * 3 + u32::from(p[1]) * 6 + u32::from(p[2])
}

fn painted(map: &TreemapLayout) -> Vec<&LayoutRect> {
    map.rects().iter().filter(|r| r.kind.is_painted()).collect()
}

/// A point at fractions `(fx, fy)` of `r`, as a pixel.
fn at(r: &LayoutRect, fx: f64, fy: f64) -> (u32, u32) {
    (
        (r.rect.x + r.rect.w * fx).floor() as u32,
        (r.rect.y + r.rect.h * fy).floor() as u32,
    )
}

#[test]
fn the_palette_gives_hues_to_the_biggest_extensions() {
    let (tree, palette, _) = fixture();
    let ext = |name: &str| -> u16 {
        (0..tree.ext_count() as u16)
            .find(|&e| tree.ext_name(e) == name)
            .expect("extension")
    };
    assert_eq!(palette.ext_color(ext("mp4")), HUES[0]);
    assert_eq!(palette.ext_color(ext("jpg")), HUES[1]);
    assert_eq!(palette.ext_color(ext("txt")), HUES[2]);
    assert_eq!(palette.ext_color(0), OTHER_FILES, "no extension stays grey");
    assert_eq!(palette.ext_color(999), OTHER_FILES);
    assert_eq!(palette.assigned().len(), 3);
}

#[test]
fn every_rectangle_shows_its_colour_lit_by_its_cushion() {
    let (tree, palette, map) = fixture();
    let buf = rasterise(&map, &tree, &palette, Shading::Cushion, W, H);
    assert_eq!(buf.len(), (W * H * 4) as usize);
    assert!(buf.chunks_exact(4).all(|p| p[3] == 255));
    let rects = painted(&map);
    assert_eq!(rects.len(), 5);
    for r in rects {
        let color = palette.rect_color(&tree, r);
        let (x, y) = at(r, 0.5, 0.5);
        let [a, b, c, d] = r.cushion;
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        let light = cushion_intensity(2.0 * a * px + c, 2.0 * b * py + d);
        let got = pixel(&buf, x, y);
        for ch in 0..3 {
            let want = (color[ch] * light * BRIGHTNESS).min(1.0) * 255.0;
            assert!(
                (f32::from(got[ch]) - want).abs() <= 1.0,
                "{}: channel {ch} {} vs {want}",
                tree.name(r.node),
                got[ch]
            );
        }
    }
}

#[test]
fn cushions_are_brighter_toward_the_upper_left() {
    let (tree, palette, map) = fixture();
    let buf = rasterise(&map, &tree, &palette, Shading::Cushion, W, H);
    let mut checked = 0;
    for r in painted(&map) {
        if r.rect.w < 6.0 || r.rect.h < 6.0 {
            continue;
        }
        let (ulx, uly) = at(r, 0.25, 0.25);
        let (lrx, lry) = at(r, 0.75, 0.75);
        let (cx, cy) = at(r, 0.5, 0.5);
        let (ul, lr, mid) = (
            luma(pixel(&buf, ulx, uly)),
            luma(pixel(&buf, lrx, lry)),
            luma(pixel(&buf, cx, cy)),
        );
        assert!(ul > lr, "{}: {ul} vs {lr}", tree.name(r.node));
        // A dome: its middle is brighter than its far edge.
        let (ex, ey) = (r.rect.right() as u32 - 1, r.rect.bottom() as u32 - 1);
        assert!(mid > luma(pixel(&buf, ex, ey)));
        checked += 1;
    }
    assert!(checked >= 3);
}

#[test]
fn flat_mode_fills_evenly_with_darker_edges() {
    let (tree, palette, map) = fixture();
    let buf = rasterise(&map, &tree, &palette, Shading::Flat, W, H);
    for r in painted(&map) {
        if r.rect.w < 6.0 || r.rect.h < 6.0 {
            continue;
        }
        let color = palette.rect_color(&tree, r);
        let (a, b) = (at(r, 0.3, 0.3), at(r, 0.6, 0.6));
        let (pa, pb) = (pixel(&buf, a.0, a.1), pixel(&buf, b.0, b.1));
        assert_eq!(pa, pb, "flat fill is even");
        assert!((f32::from(pa[0]) - color[0] * 0.92 * 255.0).abs() <= 1.0);
        let (_, x1) = r.rect.pixel_cols(W);
        let edge = pixel(&buf, x1 - 1, a.1);
        assert!(luma(edge) < luma(pa), "right edge is darker");
    }
}

#[test]
fn folders_and_aggregates_are_dark_grey() {
    let mut b = TreeBuilder::new("/r");
    let root = b.root();
    let dir = b.add_dir(root, "dir");
    for i in 0..400 {
        b.add_file(
            dir,
            &format!("f{i}.dat"),
            Sizes::new(1, 1),
            NodeFlags::empty(),
        );
    }
    let tree = b.finish(SizeMode::Allocated);
    let palette = Palette::from_extensions(&ExtTable::from_tree(&tree), SizeMode::Allocated, 2);
    let map = layout(
        &tree,
        root,
        PxRect::new(0.0, 0.0, 8.0, 6.0),
        &LayoutParams::default(),
    );
    let block = painted(&map)[0];
    assert_eq!(block.kind, RectKind::FolderBlock);
    assert_eq!(palette.rect_color(&tree, block), FOLDER_BLOCK);
}

#[test]
fn empty_maps_are_background_and_zero_sizes_are_fine() {
    let tree = TreeBuilder::new("/e").finish(SizeMode::Allocated);
    let palette = Palette::default();
    let map = layout(
        &tree,
        tree.root(),
        PxRect::new(0.0, 0.0, 4.0, 3.0),
        &LayoutParams::default(),
    );
    let buf = rasterise(&map, &tree, &palette, Shading::Cushion, 4, 3);
    assert!(buf.chunks_exact(4).all(|p| p == BACKGROUND));
    assert!(rasterise(&map, &tree, &palette, Shading::Cushion, 0, 0).is_empty());
    assert!(rasterise(&map, &tree, &palette, Shading::Flat, 0, 5).is_empty());
}

#[test]
fn the_light_comes_from_the_upper_left_front() {
    let len = (LIGHT[0] * LIGHT[0] + LIGHT[1] * LIGHT[1] + LIGHT[2] * LIGHT[2]).sqrt();
    assert!((len - 1.0).abs() < 1e-5);
    const { assert!(LIGHT[0] < 0.0 && LIGHT[1] < 0.0 && LIGHT[2] > 0.0) };
    // A flat surface gets ambient plus most of the diffuse light.
    let flat = cushion_intensity(0.0, 0.0);
    assert!(flat > 0.9 && flat <= 1.0);
    // Facing away from the light leaves only the ambient term.
    assert!((cushion_intensity(-1e6, -1e6) - AMBIENT).abs() < 1e-3);
}

#[test]
fn the_highlight_palette_dims_everything_but_one_extension() {
    let (tree, palette, map) = fixture();
    let jpg = (0..tree.ext_count() as u16)
        .find(|&e| tree.ext_name(e) == "jpg")
        .expect("jpg");
    let lit = palette.highlighting(jpg, tree.ext_count());
    assert_eq!(lit.ext_color(jpg), palette.ext_color(jpg));
    for e in (0..tree.ext_count() as u16).filter(|&e| e != jpg) {
        let (a, b) = (lit.ext_color(e), palette.ext_color(e));
        assert!(a.iter().zip(b).all(|(x, y)| *x < y * 0.5), "ext {e} dimmed");
    }
    // Rasterised: the jpg files keep their pixels, everything else is darker.
    let base = rasterise(&map, &tree, &palette, Shading::Cushion, W, H);
    let dimmed = rasterise(&map, &tree, &lit, Shading::Cushion, W, H);
    for r in painted(&map) {
        let (x, y) = at(r, 0.5, 0.5);
        let is_jpg = r.kind == RectKind::File && tree.node(r.node).is_some_and(|n| n.ext() == jpg);
        let (b, d) = (pixel(&base, x, y), pixel(&dimmed, x, y));
        if is_jpg {
            assert_eq!(b, d);
        } else {
            assert!(luma(d) * 2 < luma(b), "{:?} is dimmed", r.node);
        }
    }
}
