//! Live-scan timing, headless: how long a scan takes when the UI thread assembles the tree
//! and redraws the live map as it goes, against a blocking scan of the same folder
//! (`docs/PLAN.md` §3.2, "live map"; the target is under 10% slower).
//!
//! ```sh
//! cargo run --release -p hdtreemap-core --example bench_live_scan -- [PATH | DEMO_ID] [WxH] [ROUNDS]
//! ```
//!
//! For each round it runs a blocking scan (`scan_blocking`, or a demo drive run to the end)
//! and then a live one: an `AppState` pumped the way the native shell pumps it (every
//! `PUMP_INTERVAL_MS` with the scan pump's budget), and on each live tick the map laid out
//! and rasterised at `WxH` (default 2400x1500, a 1200x750 map at 2x), its cost reported as
//! live work. It prints both times, the live ticks and the slowest frames.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hdtreemap_core::demo_fs::{DemoPace, DemoProfile, DemoScanner};
use hdtreemap_core::layout::{layout, LayoutParams, PxRect};
use hdtreemap_core::render::{rasterise, Shading};
use hdtreemap_core::scan::{scan_blocking, FsScanner, ScanProgress, ScanSource};
use hdtreemap_core::state::{AppState, ScanTarget, FRAME_BUDGET};

/// How often the native shell's reactive loop comes back while a scan runs (the pump's
/// own redraw interval).
const PUMP_INTERVAL_MS: u64 = 30;

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
        .unwrap_or((2400u32, 1500u32));
    let rounds: usize = args.get(2).and_then(|r| r.parse().ok()).unwrap_or(2);

    for round in 1..=rounds {
        let started = Instant::now();
        let files = match DemoProfile::from_id(&source) {
            Some(profile) => DemoScanner::new(profile, DemoPace::Unpaced)
                .run_to_end()
                .map_or(0, |r| r.tree.file_count()),
            None => scan_blocking(Path::new(&source), &Arc::new(ScanProgress::new()), None)
                .tree
                .file_count(),
        };
        let blocking = started.elapsed();
        let live = live_scan(&source, w, h);
        let slower = live.elapsed.as_secs_f64() / blocking.as_secs_f64() - 1.0;
        println!(
            "round {round}: {files} files | blocking {:.2} s | live {:.2} s ({:+.1}%) | \
             {} live ticks, refresh mean {:.1} ms max {:.1} ms | frames: mean {:.1} ms, \
             p99 {:.1} ms, max {:.1} ms | final sort {:.1} ms",
            blocking.as_secs_f64(),
            live.elapsed.as_secs_f64(),
            slower * 100.0,
            live.ticks,
            live.refresh_mean_ms,
            live.refresh_max_ms,
            live.frame_mean_ms,
            live.frame_p99_ms,
            live.frame_max_ms,
            live.finish_ms,
        );
    }
}

struct LiveRun {
    elapsed: Duration,
    ticks: u64,
    refresh_mean_ms: f64,
    refresh_max_ms: f64,
    frame_mean_ms: f64,
    frame_p99_ms: f64,
    frame_max_ms: f64,
    finish_ms: f64,
}

fn live_scan(source: &str, w: u32, h: u32) -> LiveRun {
    let state = AppState::new();
    let (target, scan): (ScanTarget, Box<dyn ScanSource>) = match DemoProfile::from_id(source) {
        Some(profile) => (
            ScanTarget::folder(profile.mount_path()),
            Box::new(DemoScanner::new(profile, DemoPace::Unpaced)),
        ),
        None => (
            ScanTarget::folder(source),
            Box::new(FsScanner::start(PathBuf::from(source), None)),
        ),
    };
    let started = Instant::now();
    state.start_scan(target, scan);
    let bounds = PxRect::new(0.0, 0.0, f64::from(w), f64::from(h));
    let params = LayoutParams::default();
    let mut drawn_version = None;
    let mut refreshes = Vec::new();
    let mut frames = Vec::new();
    let mut finish_ms = 0.0;
    loop {
        let frame = Instant::now();
        let running = state.pump(FRAME_BUDGET);
        let pumped = frame.elapsed();
        if !running {
            finish_ms = pumped.as_secs_f64() * 1000.0;
        }
        if running && drawn_version != Some(state.tree_version()) {
            drawn_version = Some(state.tree_version());
            let refresh = Instant::now();
            let palette = state.palette();
            state.with_result(|r, _| {
                if let Some(palette) = &palette {
                    let map = layout(r.tree, r.tree.root(), bounds, &params);
                    let pixels = rasterise(&map, r.tree, palette, Shading::Cushion, w, h);
                    std::hint::black_box(pixels);
                }
            });
            let spent = refresh.elapsed();
            state.add_live_work(spent);
            refreshes.push(spent.as_secs_f64() * 1000.0);
        }
        frames.push(frame.elapsed().as_secs_f64() * 1000.0);
        if !running {
            break;
        }
        let rest = Duration::from_millis(PUMP_INTERVAL_MS).saturating_sub(frame.elapsed());
        std::thread::sleep(rest);
    }
    let elapsed = started.elapsed();
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    let max = |v: &[f64]| v.iter().copied().fold(0.0, f64::max);
    let mut sorted = frames.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let p99 = sorted
        .get(sorted.len().saturating_sub(1) * 99 / 100)
        .copied()
        .unwrap_or(0.0);
    LiveRun {
        elapsed,
        ticks: state.live_ticks(),
        refresh_mean_ms: mean(&refreshes),
        refresh_max_ms: max(&refreshes),
        frame_mean_ms: mean(&frames),
        frame_p99_ms: p99,
        frame_max_ms: max(&frames),
        finish_ms,
    }
}
