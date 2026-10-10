//! `FrameStats`: with `HDTREEMAP_STATS` set, `build_app` wraps the root widget in this
//! pass-through to time every frame's layout and paint of the whole widget tree (the
//! CPU side of a frame; the GPU present is the shell's), and print a summary about once a
//! second of painted frames. This is how "frame time while hovering the map" is measured
//! (`docs/PLAN.md` §3.4: under 10 ms when nothing but the hover changed).

use std::time::Duration;

use web_time::Instant;

use agg_gui::{DrawCtx, Event, EventResult, HAnchor, Rect, Size, VAnchor, Widget};

/// Print a summary once this much time has passed since the last one.
const REPORT_EVERY: Duration = Duration::from_secs(1);

/// Times its one child. See the module docs.
pub struct FrameStats {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    layout_started: Option<Instant>,
    paint_started: Option<Instant>,
    /// Layout time of the frame being painted.
    last_layout: Duration,
    frames: u32,
    total: Duration,
    worst: Duration,
    since: Instant,
}

impl FrameStats {
    pub fn new(child: Box<dyn Widget>) -> Self {
        Self {
            bounds: Rect::default(),
            children: vec![child],
            layout_started: None,
            paint_started: None,
            last_layout: Duration::ZERO,
            frames: 0,
            total: Duration::ZERO,
            worst: Duration::ZERO,
            since: Instant::now(),
        }
    }
}

impl Widget for FrameStats {
    fn type_name(&self) -> &'static str {
        "FrameStats"
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    fn h_anchor(&self) -> HAnchor {
        HAnchor::STRETCH
    }

    fn v_anchor(&self) -> VAnchor {
        VAnchor::STRETCH
    }

    fn layout(&mut self, available: Size) -> Size {
        self.layout_started = Some(Instant::now());
        if let Some(child) = self.children.first_mut() {
            // The root fills the window whatever it measures, as it would unwrapped.
            child.layout(available);
            child.set_bounds(Rect::new(0.0, 0.0, available.width, available.height));
        }
        self.bounds = Rect::new(0.0, 0.0, available.width, available.height);
        if let Some(started) = self.layout_started.take() {
            self.last_layout = started.elapsed();
        }
        available
    }

    fn paint(&mut self, _ctx: &mut dyn DrawCtx) {
        self.paint_started = Some(Instant::now());
    }

    fn finish_paint(&mut self, _ctx: &mut dyn DrawCtx) {
        let Some(started) = self.paint_started.take() else {
            return;
        };
        let frame = self.last_layout + started.elapsed();
        self.frames += 1;
        self.total += frame;
        self.worst = self.worst.max(frame);
        if self.since.elapsed() >= REPORT_EVERY {
            let avg = self.total.as_secs_f64() * 1000.0 / f64::from(self.frames);
            eprintln!(
                "hdtreemap stats: {} frames | layout+paint avg {avg:.2} ms, max {:.2} ms",
                self.frames,
                self.worst.as_secs_f64() * 1000.0
            );
            self.frames = 0;
            self.total = Duration::ZERO;
            self.worst = Duration::ZERO;
            self.since = Instant::now();
        }
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}
