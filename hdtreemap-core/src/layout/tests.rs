//! Unit tests for the treemap layout: squarify's tiling and proportions, the tree layout's
//! invariants (areas proportional to sizes, no overlaps, within bounds, sensible aspect
//! ratios, zero sizes skipped, sub-pixel leftovers aggregated, deterministic), the cushion
//! ridges and hit testing.

use super::*;
use crate::demo_fs::{DemoPace, DemoProfile, DemoScanner};
use crate::model::{NodeFlags, SizeMode, Sizes, TreeBuilder};

const BOUNDS: PxRect = PxRect::new(0.0, 0.0, 640.0, 480.0);

fn file(b: &mut TreeBuilder, parent: NodeId, name: &str, size: u64) -> NodeId {
    b.add_file(parent, name, Sizes::new(size, size), NodeFlags::empty())
}

/// /r
/// ├── big.bin   600
/// ├── docs/     a.txt 200, b.txt 100, c.txt 50
/// ├── d.txt     50
/// └── zero.txt  0
fn sample() -> Tree {
    let mut b = TreeBuilder::new("/r");
    let root = b.root();
    file(&mut b, root, "big.bin", 600);
    let docs = b.add_dir(root, "docs");
    file(&mut b, docs, "a.txt", 200);
    file(&mut b, docs, "b.txt", 100);
    file(&mut b, docs, "c.txt", 50);
    file(&mut b, root, "d.txt", 50);
    file(&mut b, root, "zero.txt", 0);
    b.finish(SizeMode::Allocated)
}

fn usb_tree() -> Tree {
    DemoScanner::new(DemoProfile::UsbStick, DemoPace::Unpaced)
        .run_to_end()
        .expect("demo scan")
        .tree
}

fn painted(map: &TreemapLayout) -> Vec<&LayoutRect> {
    map.rects().iter().filter(|r| r.kind.is_painted()).collect()
}

/// How many painted rectangles cover each pixel centre of `bounds`.
fn coverage(map: &TreemapLayout, w: u32, h: u32) -> Vec<u32> {
    let mut cover = vec![0u32; (w * h) as usize];
    for r in painted(map) {
        let (x0, x1) = r.rect.pixel_cols(w);
        let (y0, y1) = r.rect.pixel_rows(h);
        for y in y0..y1 {
            for x in x0..x1 {
                cover[(y * w + x) as usize] += 1;
            }
        }
    }
    cover
}

#[test]
fn squarify_tiles_the_rectangle_in_proportion() {
    let sizes = [500, 250, 120, 80, 30, 15, 5];
    let total: u64 = sizes.iter().sum();
    let mut out = Vec::new();
    squarify(&sizes, BOUNDS, &mut out);
    assert_eq!(out.len(), sizes.len());
    let mut area = 0.0;
    for (r, &s) in out.iter().zip(&sizes) {
        let want = BOUNDS.area() * s as f64 / total as f64;
        assert!(
            (r.area() - want).abs() < 1e-6 * BOUNDS.area(),
            "{r:?} vs {want}"
        );
        assert!(r.x >= -1e-9 && r.y >= -1e-9);
        assert!(r.right() <= BOUNDS.w + 1e-9 && r.bottom() <= BOUNDS.h + 1e-9);
        area += r.area();
    }
    assert!((area - BOUNDS.area()).abs() < 1e-6);
    for (i, a) in out.iter().enumerate() {
        for b in &out[i + 1..] {
            let ox = (a.right().min(b.right()) - a.x.max(b.x)).max(0.0);
            let oy = (a.bottom().min(b.bottom()) - a.y.max(b.y)).max(0.0);
            assert!(ox * oy < 1e-9, "{a:?} overlaps {b:?}");
        }
    }
}

#[test]
fn squarify_keeps_rectangles_squarish() {
    // Equal sizes in a square fall into a near-perfect grid.
    let sizes = [1u64; 16];
    let mut out = Vec::new();
    squarify(&sizes, PxRect::new(0.0, 0.0, 400.0, 400.0), &mut out);
    for r in &out {
        assert!(aspect(r) < 1.5, "{r:?}");
    }
}

#[test]
fn squarify_survives_degenerate_input() {
    let mut out = Vec::new();
    squarify(&[], BOUNDS, &mut out);
    assert!(out.is_empty());
    squarify(&[0, 0], BOUNDS, &mut out);
    assert_eq!(out.len(), 2);
    assert!(out.iter().all(|r| r.area() == 0.0));
    out.clear();
    squarify(&[5, 3], PxRect::new(0.0, 0.0, 0.0, 10.0), &mut out);
    assert_eq!(out.len(), 2);
}

fn aspect(r: &PxRect) -> f64 {
    let (a, b) = (r.w.max(r.h), r.w.min(r.h));
    if b <= 0.0 {
        f64::INFINITY
    } else {
        a / b
    }
}

#[test]
fn file_areas_are_proportional_to_their_sizes() {
    let tree = sample();
    let map = layout(&tree, tree.root(), BOUNDS, &LayoutParams::default());
    let total = tree.size(tree.root()) as f64;
    let files: Vec<_> = painted(&map);
    assert_eq!(files.len(), 5, "zero.txt gets no rectangle");
    for r in files {
        assert_eq!(r.kind, RectKind::File);
        let want = BOUNDS.area() * tree.size(r.node) as f64 / total;
        assert!(
            (r.rect.area() - want).abs() < 1e-6 * BOUNDS.area(),
            "{}: {} vs {want}",
            tree.name(r.node),
            r.rect.area()
        );
    }
    assert!(map.rects().iter().all(|r| tree.name(r.node) != "zero.txt"));
}

#[test]
fn directories_are_listed_before_their_contents_with_depths() {
    let tree = sample();
    let map = layout(&tree, tree.root(), BOUNDS, &LayoutParams::default());
    let rects = map.rects();
    assert_eq!(rects[0].node, tree.root());
    assert_eq!(rects[0].kind, RectKind::Dir);
    assert_eq!(rects[0].parent, NO_PARENT);
    assert_eq!(rects[0].rect, BOUNDS);
    for (i, r) in rects.iter().enumerate().skip(1) {
        let parent = r.parent as usize;
        assert!(parent < i, "pre-order");
        assert_eq!(rects[parent].kind, RectKind::Dir);
        assert_eq!(r.depth, rects[parent].depth + 1);
        let p = rects[parent].rect;
        assert!(r.rect.x >= p.x - 1e-9 && r.rect.right() <= p.right() + 1e-9);
        assert!(r.rect.y >= p.y - 1e-9 && r.rect.bottom() <= p.bottom() + 1e-9);
    }
}

#[test]
fn a_real_tree_tiles_its_bounds_exactly_once() {
    let tree = usb_tree();
    let (w, h) = (320u32, 240u32);
    let bounds = PxRect::new(0.0, 0.0, f64::from(w), f64::from(h));
    let map = layout(&tree, tree.root(), bounds, &LayoutParams::default());
    assert!(map.painted_count() > 50);
    let area: f64 = painted(&map).iter().map(|r| r.rect.area()).sum();
    assert!((area - bounds.area()).abs() < 1e-6 * bounds.area());
    for r in map.rects() {
        assert!(r.rect.x >= -1e-9 && r.rect.y >= -1e-9);
        assert!(r.rect.right() <= bounds.w + 1e-6 && r.rect.bottom() <= bounds.h + 1e-6);
    }
    let cover = coverage(&map, w, h);
    assert!(
        cover.iter().all(|&c| c == 1),
        "every pixel belongs to exactly one painted rectangle"
    );
}

#[test]
fn aspect_ratios_stay_sensible_on_a_typical_tree() {
    let tree = usb_tree();
    let map = layout(&tree, tree.root(), BOUNDS, &LayoutParams::default());
    // Rectangles big enough to point at should mostly be close to square.
    let ratios: Vec<f64> = painted(&map)
        .iter()
        .filter(|r| r.rect.area() >= 100.0)
        .map(|r| aspect(&r.rect))
        .collect();
    assert!(ratios.len() > 20);
    let squarish = ratios.iter().filter(|&&a| a <= 3.0).count();
    assert!(
        squarish * 10 >= ratios.len() * 8,
        "{squarish} of {} within 3:1",
        ratios.len()
    );
    let mean = ratios.iter().sum::<f64>() / ratios.len() as f64;
    assert!(mean < 3.0, "mean aspect {mean}");
}

#[test]
fn layouts_are_deterministic() {
    let tree = usb_tree();
    let a = layout(&tree, tree.root(), BOUNDS, &LayoutParams::default());
    let b = layout(&tree, tree.root(), BOUNDS, &LayoutParams::default());
    assert_eq!(a.rects(), b.rects());
}

#[test]
fn sub_pixel_children_merge_into_one_aggregate() {
    let mut b = TreeBuilder::new("/r");
    let root = b.root();
    file(&mut b, root, "big.bin", 1_000_000);
    let many = b.add_dir(root, "many");
    file(&mut b, many, "medium.bin", 50_000);
    for i in 0..5000 {
        file(&mut b, many, &format!("tiny{i}.txt"), 10);
    }
    let tree = b.finish(SizeMode::Allocated);
    let bounds = PxRect::new(0.0, 0.0, 100.0, 100.0);
    let map = layout(&tree, root, bounds, &LayoutParams::default());
    let aggregates: Vec<(usize, &LayoutRect)> = map
        .rects()
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RectKind::Aggregate)
        .collect();
    assert_eq!(aggregates.len(), 1);
    let (index, agg) = aggregates[0];
    assert_eq!(agg.node, many);
    let info = map.aggregate(index).expect("aggregate info");
    assert_eq!(
        info,
        AggregateInfo {
            count: 5000,
            bytes: 50_000
        }
    );
    let want = bounds.area() * 50_000.0 / tree.size(root) as f64;
    assert!((agg.rect.area() - want).abs() < 1e-9 * bounds.area());
    assert_eq!(map.aggregate(0), None);
    // The tiny files themselves are not laid out.
    assert!(map.rects().len() < 10);
}

#[test]
fn a_folder_with_nothing_big_enough_is_one_block() {
    let mut b = TreeBuilder::new("/r");
    let root = b.root();
    let dir = b.add_dir(root, "dir");
    for i in 0..1000 {
        file(&mut b, dir, &format!("f{i}"), 1);
    }
    let tree = b.finish(SizeMode::Allocated);
    let map = layout(
        &tree,
        root,
        PxRect::new(0.0, 0.0, 10.0, 10.0),
        &LayoutParams::default(),
    );
    let blocks: Vec<_> = painted(&map);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].kind, RectKind::FolderBlock);
    assert_eq!(blocks[0].node, dir);
}

#[test]
fn min_area_is_configurable() {
    let tree = usb_tree();
    let fine = layout(&tree, tree.root(), BOUNDS, &LayoutParams::default());
    let coarse_params = LayoutParams {
        min_area: 64.0,
        ..LayoutParams::default()
    };
    let coarse = layout(&tree, tree.root(), BOUNDS, &coarse_params);
    assert!(coarse.painted_count() < fine.painted_count());
}

#[test]
fn zooming_lays_out_only_the_subtree() {
    let tree = sample();
    let docs = tree
        .children(tree.root())
        .find(|&c| tree.name(c) == "docs")
        .expect("docs");
    let map = layout(&tree, docs, BOUNDS, &LayoutParams::default());
    assert_eq!(map.root(), docs);
    assert_eq!(map.rects()[0].node, docs);
    assert_eq!(map.rects()[0].depth, 0);
    assert_eq!(painted(&map).len(), 3);
}

#[test]
fn empty_inputs_give_empty_layouts() {
    let tree = sample();
    let empty = TreeBuilder::new("/e").finish(SizeMode::Allocated);
    for (t, bounds) in [
        (&empty, BOUNDS),
        (&tree, PxRect::new(0.0, 0.0, 0.0, 0.0)),
        (&tree, PxRect::new(0.0, 0.0, -5.0, 10.0)),
        (&tree, PxRect::new(0.0, 0.0, f64::NAN, 10.0)),
    ] {
        let map = layout(t, t.root(), bounds, &LayoutParams::default());
        assert!(map.rects().is_empty());
        assert_eq!(map.hit(1.0, 1.0), None);
    }
    // An unknown zoom root too.
    let map = layout(&tree, NodeId(9999), BOUNDS, &LayoutParams::default());
    assert!(map.rects().is_empty());
    // A one-pixel map still works.
    let map = layout(
        &tree,
        tree.root(),
        PxRect::new(0.0, 0.0, 1.0, 1.0),
        &LayoutParams::default(),
    );
    assert!(map.painted_count() >= 1);
}

#[test]
fn cushions_peak_in_the_middle_of_a_lone_file() {
    let mut b = TreeBuilder::new("/r");
    let root = b.root();
    file(&mut b, root, "only.bin", 10);
    let tree = b.finish(SizeMode::Allocated);
    let bounds = PxRect::new(10.0, 20.0, 200.0, 100.0);
    let map = layout(&tree, root, bounds, &LayoutParams::default());
    let leaf = painted(&map)[0];
    let [a, bq, c, d] = leaf.cushion.map(f64::from);
    assert!(a < 0.0 && bq < 0.0, "a cushion is a dome");
    // The gradient vanishes at the centre and has slope 4·(h + f·h) at the edges.
    let (cx, cy) = (110.0, 70.0);
    assert!((2.0 * a * cx + c).abs() < 1e-3);
    assert!((2.0 * bq * cy + d).abs() < 1e-3);
    let p = CushionParams::default();
    let edge_slope = 2.0 * a * 10.0 + c;
    assert!((edge_slope - 4.0 * p.height * (1.0 + p.falloff)).abs() < 1e-3);
}

#[test]
fn hit_testing_finds_the_rectangle_and_its_folders() {
    let tree = usb_tree();
    let map = layout(&tree, tree.root(), BOUNDS, &LayoutParams::default());
    let mut checked = 0;
    for yi in 0..48 {
        for xi in 0..64 {
            let (x, y) = (xi as f64 * 10.0 + 3.3, yi as f64 * 10.0 + 7.1);
            let index = map.hit(x, y).expect("every point is covered");
            let r = &map.rects()[index];
            assert!(r.kind.is_painted());
            assert!(r.rect.contains(x, y));
            // The ancestor chain contains the point and ends at the root.
            let chain: Vec<usize> = map.ancestors(index).collect();
            assert_eq!(chain.last(), Some(&0));
            for a in &chain {
                let dir = &map.rects()[*a];
                assert_eq!(dir.kind, RectKind::Dir);
                assert!(dir.rect.contains(x, y));
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 64 * 48);
    assert_eq!(map.hit(-1.0, 5.0), None);
    assert_eq!(map.hit(5.0, BOUNDS.h), None);
    assert_eq!(map.hit(f64::NAN, 5.0), None);
}

#[test]
fn hits_and_lookups_agree_on_known_files() {
    let tree = sample();
    let map = layout(&tree, tree.root(), BOUNDS, &LayoutParams::default());
    for r in painted(&map) {
        let (cx, cy) = (r.rect.x + r.rect.w / 2.0, r.rect.y + r.rect.h / 2.0);
        let hit = map.hit(cx, cy).expect("hit");
        assert_eq!(map.rects()[hit].node, r.node);
        assert_eq!(map.index_of(r.node), Some(hit));
    }
    assert_eq!(map.index_of(tree.root()), Some(0));
}
