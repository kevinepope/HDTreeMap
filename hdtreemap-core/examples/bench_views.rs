//! Frame timing of the whole results screen (toolbar, folder tree, legend, treemap, status
//! bar), headless on agg-gui's software renderer (`docs/PLAN.md` §3.4: under 10 ms a frame).
//!
//! ```sh
//! cargo run --release -p hdtreemap-core --example bench_views -- [DEMO_ID] [WxH] [EXT...]
//! ```
//!
//! Scans a simulated drive (default `archive-server`, ≈1.5 M files) unpaced, builds the real
//! app at the given window size (default 1600x1000, scale 1), and times frames (drain, layout
//! and paint into a software framebuffer) while: nothing changes; the pointer moves over the
//! map; it moves over the folder tree's rows; it clicks around the map (selecting, so the
//! tree expands and scrolls); and, for each extension given (default `log jpg`), the
//! extension is highlighted while the pointer moves over the map. The software renderer
//! rasterises every widget on the CPU, so these are upper bounds for the GPU shells.

use std::rc::Rc;
use std::time::{Duration, Instant};

use agg_gui::widget::{find_widget_by_id, find_widget_screen_rect};
use agg_gui::{App, Framebuffer, GfxCtx, Modifiers, MouseButton, Rect, Size};
use hdtreemap_core::platform::{DemoPlatform, PlatformServices};
use hdtreemap_core::state::ScanTarget;
use hdtreemap_core::ui::folder_tree::FOLDER_TREE_ID;
use hdtreemap_core::ui::treemap_view::{TreemapView, TREEMAP_ID};
use hdtreemap_core::{build_app, install_theme_and_fonts, AppState};

struct Bench {
    app: App,
    state: AppState,
    size: Size,
    fb: Framebuffer,
    /// Layout share of the frames of the current phase.
    layout: Duration,
}

impl Bench {
    fn frame(&mut self) -> Duration {
        let started = Instant::now();
        agg_gui::ui_thread::invoke_pending_actions();
        self.app.layout(self.size);
        self.layout += started.elapsed();
        let mut ctx = GfxCtx::new(&mut self.fb);
        self.app.paint(&mut ctx);
        started.elapsed()
    }

    fn rect(&self, id: &str) -> Rect {
        find_widget_screen_rect(self.app.root(), id).expect("widget on screen")
    }

    /// Y-down shell point at fractions of widget `id`.
    fn point(&self, id: &str, fx: f64, fy: f64) -> (f64, f64) {
        let r = self.rect(id);
        (
            r.x + r.width * fx,
            self.size.height - (r.y + r.height * (1.0 - fy)),
        )
    }

    /// Time `n` frames, each after `step(i)`.
    fn phase(&mut self, label: &str, n: usize, mut step: impl FnMut(&mut Self, usize)) {
        let mut times = Vec::with_capacity(n);
        self.layout = Duration::ZERO;
        for i in 0..n {
            step(self, i);
            times.push(self.frame());
        }
        let ms: Vec<f64> = times.iter().map(|t| t.as_secs_f64() * 1000.0).collect();
        let mean = ms.iter().sum::<f64>() / ms.len() as f64;
        let mut sorted = ms.clone();
        sorted.sort_by(f64::total_cmp);
        let p95 = sorted[(sorted.len() * 95 / 100).min(sorted.len() - 1)];
        let max = sorted.last().copied().unwrap_or_default();
        let layout = self.layout.as_secs_f64() * 1000.0 / n as f64;
        println!(
            "{label:<34} mean {mean:6.2} ms (layout {layout:5.2}) | p95 {p95:6.2} ms | max {max:6.2} ms"
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let id = args
        .first()
        .cloned()
        .unwrap_or_else(|| "archive-server".to_string());
    let (w, h) = args
        .get(1)
        .and_then(|s| s.split_once('x'))
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .unwrap_or((1600.0f64, 1000.0f64));
    let exts: Vec<String> = if args.len() > 2 {
        args[2..].to_vec()
    } else {
        vec!["log".to_string(), "jpg".to_string()]
    };

    let platform = DemoPlatform::unpaced();
    let volume = platform
        .list_volumes()
        .into_iter()
        .find(|v| v.id == id)
        .expect("a demo drive id");
    let state = AppState::new();
    let target = ScanTarget::volume(&volume);
    let started = Instant::now();
    state.start_scan(target.clone(), platform.scan_source(&target));
    while state.pump(Duration::from_secs(60)) {}
    let files = state.with_result(|r, _| r.tree.file_count()).unwrap_or(0);
    println!(
        "{id}: {files} files scanned in {:.1} s; window {w}x{h}",
        started.elapsed().as_secs_f64()
    );

    let fonts = install_theme_and_fonts(1.0).expect("fonts");
    let app = build_app(state.clone(), Rc::new(platform), fonts);
    let mut b = Bench {
        app,
        state,
        size: Size::new(w, h),
        fb: Framebuffer::new(w as u32, h as u32),
        layout: Duration::ZERO,
    };
    let first = b.frame();
    println!(
        "first frame (layout + raster + tree)  {:.1} ms",
        first.as_secs_f64() * 1000.0
    );
    b.frame();

    b.phase("idle (nothing changed)", 60, |_, _| {});
    b.phase("hover the map", 200, |b, i| {
        let (fx, fy) = ((i % 20) as f64 / 20.0 + 0.02, (i / 20) as f64 / 10.0 + 0.03);
        let (x, y) = b.point(TREEMAP_ID, fx, fy);
        b.app.on_mouse_move(x, y);
    });
    b.phase("hover folder-tree rows", 100, |b, i| {
        let (x, y) = b.point(FOLDER_TREE_ID, 0.4, (i % 12) as f64 / 40.0 + 0.01);
        b.app.on_mouse_move(x, y);
    });
    b.phase("select on the map (tree follows)", 60, |b, i| {
        let (fx, fy) = ((i % 10) as f64 / 10.0 + 0.05, (i / 10) as f64 / 6.0 + 0.05);
        let (x, y) = b.point(TREEMAP_ID, fx, fy);
        b.app.on_mouse_move(x, y);
        b.app
            .on_mouse_down(x, y, MouseButton::Left, Modifiers::default());
        b.app
            .on_mouse_up(x, y, MouseButton::Left, Modifiers::default());
    });
    println!(
        "tree rows built after selecting: {}",
        find_widget_by_id(b.app.root(), FOLDER_TREE_ID)
            .and_then(|w| w.as_any())
            .and_then(|a| a.downcast_ref::<hdtreemap_core::ui::folder_tree::FolderTree>())
            .map_or(0, |t| t.node_count())
    );

    for ext in exts {
        let row = b
            .state
            .with_result(|_, table| table.get(&ext).map(|r| (r.ext, r.count)))
            .flatten();
        let Some((ext_id, count)) = row else {
            println!(".{ext}: not on this drive");
            continue;
        };
        b.state.set_selected(None);
        b.state.set_highlight_ext(Some(ext_id));
        let switch = b.frame();
        let shown = find_widget_by_id(b.app.root(), TREEMAP_ID)
            .and_then(|w| w.as_any())
            .and_then(|a| a.downcast_ref::<TreemapView>())
            .map_or(0, |t| t.highlighted_rects());
        println!(
            ".{ext}: {count} files, {shown} rectangles highlighted; switching it on {:.1} ms",
            switch.as_secs_f64() * 1000.0
        );
        b.phase(
            &format!("hover the map, .{ext} highlighted"),
            200,
            |b, i| {
                let (fx, fy) = ((i % 20) as f64 / 20.0 + 0.02, (i / 20) as f64 / 10.0 + 0.03);
                let (x, y) = b.point(TREEMAP_ID, fx, fy);
                b.app.on_mouse_move(x, y);
            },
        );
        b.state.set_highlight_ext(None);
        b.frame();
    }
}
