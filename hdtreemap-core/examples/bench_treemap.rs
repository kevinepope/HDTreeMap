//! Treemap layout and raster timing, headless (`docs/PLAN.md` §3.4 targets: under ~150 ms to
//! re-rasterise a 4K map of a million files).
//!
//! ```sh
//! cargo run --release -p hdtreemap-core --example bench_treemap -- [DEMO_ID | PATH] [WxH]
//! ```
//!
//! Scans a simulated drive (default `archive-server`, ≈1.8 M files) or a real folder, then
//! lays it out and rasterises it (cushion and flat) at the given size (default 3840x2160),
//! a few times each, and prints the best and mean times.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hdtreemap_core::demo_fs::{DemoPace, DemoProfile, DemoScanner};
use hdtreemap_core::layout::{layout, LayoutParams, PxRect};
use hdtreemap_core::model::ExtTable;
use hdtreemap_core::render::{rasterise, Palette, Shading};
use hdtreemap_core::scan::{scan_blocking, ScanProgress, ScanResult};

const RUNS: usize = 5;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let source = args
        .first()
        .cloned()
        .unwrap_or_else(|| "archive-server".to_string());
    let (w, h) = args
        .get(1)
        .and_then(|s| s.split_once('x'))
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .unwrap_or((3840u32, 2160u32));

    let started = Instant::now();
    let result: ScanResult = match DemoProfile::from_id(&source) {
        Some(profile) => DemoScanner::new(profile, DemoPace::Unpaced)
            .run_to_end()
            .expect("demo scan"),
        None => scan_blocking(Path::new(&source), &Arc::new(ScanProgress::new()), None),
    };
    let tree = &result.tree;
    println!(
        "{source}: {} files, {} folders, scanned in {:.1} s",
        tree.file_count(),
        tree.dir_count(),
        started.elapsed().as_secs_f64()
    );

    let bounds = PxRect::new(0.0, 0.0, f64::from(w), f64::from(h));
    let params = LayoutParams::default();
    let mut map = None;
    let layout_times = time(|| map = Some(layout(tree, tree.root(), bounds, &params)));
    let map = map.expect("laid out");
    println!(
        "layout {w}x{h}: {} rects ({} painted) | {}",
        map.rects().len(),
        map.painted_count(),
        summary(&layout_times)
    );

    let table = ExtTable::from_tree(tree);
    let palette = Palette::from_extensions(&table, tree.size_mode(), tree.ext_count());
    for shading in [Shading::Cushion, Shading::Flat] {
        let times = time(|| {
            let buf = rasterise(&map, tree, &palette, shading, w, h);
            std::hint::black_box(buf);
        });
        println!("raster {shading:?}: {}", summary(&times));
    }

    let hits = time(|| {
        for i in 0..10_000u32 {
            let x = f64::from(i * 7919 % w) + 0.5;
            let y = f64::from(i * 104_729 % h) + 0.5;
            std::hint::black_box(map.hit(x, y));
        }
    });
    println!("10k hit tests: {}", summary(&hits));
}

fn time(mut f: impl FnMut()) -> Vec<Duration> {
    (0..RUNS)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed()
        })
        .collect()
}

fn summary(times: &[Duration]) -> String {
    let ms: Vec<f64> = times.iter().map(|t| t.as_secs_f64() * 1000.0).collect();
    let best = ms.iter().copied().fold(f64::INFINITY, f64::min);
    let mean = ms.iter().sum::<f64>() / ms.len() as f64;
    format!("best {best:.1} ms, mean {mean:.1} ms")
}
