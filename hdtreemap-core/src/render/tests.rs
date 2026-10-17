//! Golden-invariant tests for the rasteriser on a small fixed tree at 64×48: every
//! rectangle shows its palette colour, cushions are brighter toward the upper left, flat mode
//! fills evenly with darker edges, bevel mode rims each tile light and dark, and the
//! palette's hue assignment. Pixel invariants rather than a stored image, so harmless
//! tuning does not break them.

use super::city;
use super::raster::{to_byte, FLAT_EDGE_MIN, FLAT_INTENSITY};
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
    assert!(buf.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
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
fn bevel_mode_has_a_light_top_left_rim_and_a_dark_bottom_right_rim() {
    let (tree, palette, map) = fixture();
    let buf = rasterise(&map, &tree, &palette, Shading::Bevel, W, H);
    let flat = rasterise(&map, &tree, &palette, Shading::Flat, W, H);
    let mut checked = 0;
    for r in painted(&map) {
        let (x0, x1) = r.rect.pixel_cols(W);
        let (y0, y1) = r.rect.pixel_rows(H);
        if x1 - x0 < 4 * BEVEL_WIDTH + 2 || y1 - y0 < 4 * BEVEL_WIDTH + 2 {
            continue;
        }
        checked += 1;
        let (cx, cy) = ((x0 + x1) / 2, (y0 + y1) / 2);
        let centre = pixel(&buf, cx, cy);
        assert_eq!(centre, pixel(&flat, cx, cy), "the fill matches flat mode");
        assert_eq!(centre, pixel(&buf, cx + 1, cy + 1), "the fill is even");
        let color = palette.rect_color(&tree, r);
        // The rim is BEVEL_WIDTH deep: the pixel just inside it is fill.
        for d in 0..BEVEL_WIDTH {
            let top = pixel(&buf, cx, y0 + d);
            let left = pixel(&buf, x0 + d, cy);
            let bottom = pixel(&buf, cx, y1 - 1 - d);
            let right = pixel(&buf, x1 - 1 - d, cy);
            assert!(luma(top) > luma(centre), "top rim is lighter");
            assert_eq!(top, left, "top and left rims match");
            assert!(luma(bottom) < luma(centre), "bottom rim is darker");
            assert_eq!(bottom, right, "bottom and right rims match");
            // Lightening moves every channel towards white, so a dark channel still rises.
            for c in 0..3 {
                assert!(
                    top[c] >= centre[c],
                    "channel {c} of {color:?} is not darker"
                );
            }
        }
        assert_eq!(pixel(&buf, cx, y0 + BEVEL_WIDTH), centre);
        assert_eq!(pixel(&buf, cx, y1 - 1 - BEVEL_WIDTH), centre);
        // The top-left corner belongs to the light rim, the bottom-right to the dark one.
        assert_eq!(pixel(&buf, x0, y0), pixel(&buf, cx, y0));
        assert_eq!(pixel(&buf, x1 - 1, y1 - 1), pixel(&buf, cx, y1 - 1));
    }
    assert!(checked >= 2, "fixture has {checked} bevelled rectangles");
}

#[test]
fn tiny_bevelled_rectangles_get_a_thin_rim_or_none() {
    let (tree, palette, _) = fixture();
    // Squeezed to a strip, so every rectangle is a few pixels tall at most.
    let bounds = PxRect::new(0.0, 0.0, f64::from(W), 5.0);
    let strip = layout(&tree, tree.root(), bounds, &LayoutParams::default());
    let buf = rasterise(&strip, &tree, &palette, Shading::Bevel, W, 5);
    for r in strip.rects().iter().filter(|r| r.kind.is_painted()) {
        let (x0, x1) = r.rect.pixel_cols(W);
        let (y0, y1) = r.rect.pixel_rows(5);
        if x0 >= x1 || y0 >= y1 {
            continue;
        }
        let colour = palette
            .rect_color(&tree, r)
            .map(|c| to_byte(c * FLAT_INTENSITY));
        let (w, h) = (x1 - x0, y1 - y0);
        let top_left = pixel(&buf, x0, y0);
        if w.min(h) < FLAT_EDGE_MIN {
            // Too small for a rim: plain fill.
            assert_eq!(&top_left[..3], &colour[..]);
        } else {
            // A one-pixel rim: the next pixel in is already fill.
            assert!(luma(top_left) > luma([colour[0], colour[1], colour[2], 255]));
            if w >= 3 && h >= 3 {
                assert_eq!(&pixel(&buf, x0 + 1, y0 + 1)[..3], &colour[..]);
            }
        }
    }
}

/// `/r` with one file, so the building fills the raster less its streets.
fn one_file() -> (Tree, Palette, TreemapLayout) {
    let mut b = TreeBuilder::new("/r");
    let root = b.root();
    b.add_file(root, "big.mp4", Sizes::new(1000, 1000), NodeFlags::empty());
    let tree = b.finish(SizeMode::Allocated);
    let table = ExtTable::from_tree(&tree);
    let palette = Palette::from_extensions(&table, SizeMode::Allocated, tree.ext_count());
    let bounds = PxRect::new(0.0, 0.0, f64::from(W), f64::from(H));
    let map = layout(&tree, tree.root(), bounds, &LayoutParams::default());
    (tree, palette, map)
}

#[test]
fn a_cityscape_building_has_a_raised_roof_two_walls_and_a_street() {
    let (tree, palette, map) = one_file();
    let buf = rasterise(&map, &tree, &palette, Shading::Cityscape, W, H);
    let r = painted(&map)[0];
    let color = palette.rect_color(&tree, r);
    let roof = color.map(|c| to_byte(c * FLAT_INTENSITY));
    let right = color.map(|c| to_byte(c * FLAT_INTENSITY * city::RIGHT_WALL));
    let bottom = color.map(|c| to_byte(c * FLAT_INTENSITY * city::BOTTOM_WALL));
    let background = palette.background();
    let h = city::max_height(W, H);
    assert_eq!(h, city::MIN_MAX_HEIGHT, "a 64×48 raster uses the minimum");
    // The footprint is the rectangle less a one-pixel street right and bottom; the zoom
    // root's own edges add no street.
    let (x1, y1) = (W as f32 - city::STREET, H as f32 - city::STREET);
    // The bottom-right corner is street (ground).
    assert_eq!(pixel(&buf, W - 1, H - 1), background);
    // The roof is the footprint moved up and left by the rise.
    let (ox, oy) = (h * city::RISE[0], h * city::RISE[1]);
    let roof_at = |fx: f32, fy: f32| pixel(&buf, (fx + ox) as u32, (fy + oy) as u32);
    assert_eq!(&roof_at(x1 / 2.0, y1 / 2.0)[..3], &roof[..]);
    assert_eq!(
        &roof_at(x1 - 1.0, y1 - 1.0)[..3],
        &roof[..],
        "roof's bottom-right"
    );
    // Top-left of the raster: the roof moved off the edge, so only roof remains there.
    assert_eq!(&pixel(&buf, 0, 0)[..3], &roof[..]);
    // Straight below the roof's bottom edge lies the bottom wall, then the street.
    let under_roof = (y1 + oy) as u32 + 1;
    assert_eq!(&pixel(&buf, W / 2, under_roof)[..3], &bottom[..]);
    assert_eq!(&pixel(&buf, W / 2, H - 2)[..3], &bottom[..]);
    // Right of the roof's right edge lies the right wall (where it is not under the
    // bottom wall's parallelogram).
    let beside_roof = (x1 + ox) as u32 + 1;
    assert_eq!(&pixel(&buf, beside_roof, H / 2)[..3], &right[..]);
    // The walls are darker than the roof, the bottom darkest.
    assert!(luma([roof[0], roof[1], roof[2], 255]) > luma([right[0], right[1], right[2], 255]));
    assert!(
        luma([right[0], right[1], right[2], 255]) > luma([bottom[0], bottom[1], bottom[2], 255])
    );
}

#[test]
fn bigger_files_make_taller_buildings_and_streets_widen_at_folder_edges() {
    let (tree, palette, map) = fixture();
    let buf = rasterise(&map, &tree, &palette, Shading::Cityscape, W, H);
    let background = palette.background();
    let indices: Vec<usize> = map
        .rects()
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind.is_painted())
        .map(|(i, _)| i)
        .collect();
    let buildings = city::plan(&map, &tree, &indices, W, H);
    // movie.mp4 (5000) is the biggest and reaches the full height; a.jpg (1500) is lower
    // by the cube root of the ratio; nothing is taller than the biggest.
    let by_name = |name: &str| {
        indices
            .iter()
            .zip(&buildings)
            .find(|(&i, _)| tree.name(map.rects()[i].node) == name)
            .map(|(_, b)| *b)
            .expect(name)
    };
    let movie = by_name("movie.mp4");
    let a = by_name("a.jpg");
    assert_eq!(movie.height, city::max_height(W, H));
    let expected = movie.height * (1500.0f64 / 5000.0).cbrt() as f32;
    assert!(
        (a.height - expected).abs() < 1e-3,
        "{} vs {expected}",
        a.height
    );
    assert!(buildings.iter().all(|b| b.height <= movie.height));
    // a.jpg sits inside photos/: where its edge is also the folder's, the street is wider
    // than a plain one, and inside the folder it is a plain street.
    let photos = map
        .rects()
        .iter()
        .find(|r| r.kind == RectKind::Dir && r.depth == 1)
        .expect("photos/ rect");
    let a_rect = map.rects()[map.index_of(tree_node(&tree, "a.jpg")).expect("a.jpg")].rect;
    let right_street = a_rect.right() as f32 - a.x1;
    let bottom_street = a_rect.bottom() as f32 - a.y1;
    let on_folder_right = (photos.rect.right() - a_rect.right()).abs() < 1e-6;
    let on_folder_bottom = (photos.rect.bottom() - a_rect.bottom()).abs() < 1e-6;
    let plain = city::STREET;
    let wide = city::STREET + city::STREET_PER_FOLDER;
    assert_eq!(right_street, if on_folder_right { wide } else { plain });
    assert_eq!(bottom_street, if on_folder_bottom { wide } else { plain });
    // Somewhere in the raster the ground shows between buildings.
    let ground = (0..H)
        .flat_map(|y| (0..W).map(move |x| (x, y)))
        .filter(|&(x, y)| pixel(&buf, x, y) == background)
        .count();
    assert!(ground > 0, "streets show");
    // Nothing is lit beyond a roof: every pixel is ground or no brighter than the
    // brightest roof.
    let brightest = indices
        .iter()
        .map(|&i| {
            let [r, g, b] = palette
                .rect_color(&tree, &map.rects()[i])
                .map(|v| to_byte(v * FLAT_INTENSITY));
            luma([r, g, b, 255])
        })
        .max()
        .expect("buildings");
    for y in 0..H {
        for x in 0..W {
            let p = pixel(&buf, x, y);
            assert!(p == background || luma(p) <= brightest);
        }
    }
}

/// The node called `name` (unique in the fixtures).
fn tree_node(tree: &Tree, name: &str) -> crate::model::NodeId {
    (0..tree.arena_len())
        .map(|i| crate::model::NodeId(i as u32))
        .find(|&id| tree.node(id).is_some() && tree.name(id) == name)
        .expect(name)
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
    assert!(buf.as_chunks::<4>().0.iter().all(|p| *p == BACKGROUND));
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

#[test]
fn by_top_folder_everything_in_a_folder_shares_its_hue() {
    let (tree, _, map) = fixture();
    let palette = Palette::for_top_folders(&tree, tree.root());
    assert_eq!(palette.color_by(), ColorBy::TopFolder);
    // Children by size: movie.mp4 5000, photos 2500, notes.txt 600, misc 400.
    let order: Vec<&str> = palette
        .folders()
        .iter()
        .map(|&(n, _)| tree.name(n))
        .collect();
    assert_eq!(order, ["movie.mp4", "photos", "notes.txt", "misc"]);
    let photos = palette.folders()[1].0;
    assert_eq!(palette.folder_color(photos), HUES[1]);
    let colors = palette.colors(&tree, &map);
    for (r, color) in map.rects().iter().zip(&colors) {
        if r.kind != RectKind::File {
            continue;
        }
        let top = if tree.node(r.node).map(|n| n.parent()) == Some(tree.root()) {
            r.node
        } else {
            photos
        };
        assert_eq!(*color, palette.folder_color(top), "{}", tree.name(r.node));
    }
    // Both photos share the folder's hue; the cushions still tell them apart when drawn.
    let jpgs: Vec<_> = map
        .rects()
        .iter()
        .zip(&colors)
        .filter(|(r, _)| tree.name(r.node).ends_with(".jpg"))
        .map(|(_, c)| *c)
        .collect();
    assert_eq!(jpgs, [HUES[1], HUES[1]]);
    // A highlight dims everything but the highlighted extension's files.
    let jpg = (0..tree.ext_count() as u16)
        .find(|&e| tree.ext_name(e) == "jpg")
        .expect("jpg");
    let lit = palette
        .highlighting(jpg, tree.ext_count())
        .colors(&tree, &map);
    for ((r, base), lit) in map.rects().iter().zip(&colors).zip(&lit) {
        if r.kind == RectKind::File && tree.name(r.node).ends_with(".jpg") {
            assert_eq!(base, lit);
        } else if r.kind.is_painted() {
            assert!(lit[0] <= base[0] * 0.5 + 1e-6);
        }
    }
}
