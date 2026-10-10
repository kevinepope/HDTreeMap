//! Shared harness for the headless UI tests: the real production tree (`build_app`) over a
//! fake or demo `PlatformServices`, with no window and no GPU. Each frame drains agg-gui's
//! `ui_thread` queue (which runs the scan pump, as a shell does), lays out, and paints into
//! a software framebuffer; clicks go through the same `App` mouse entry points a shell uses.
//! Widgets are found by their reflection ids.

#![allow(dead_code)]

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use agg_gui::widget::{find_widget_by_id, find_widget_screen_rect};
use agg_gui::{App, Framebuffer, GfxCtx, Modifiers, MouseButton, Size};
use hdtreemap_core::demo_fs::{DemoPace, DemoProfile, DemoScanner};
use hdtreemap_core::platform::{ActionOutcome, ActionResult, PlatformServices, VolumeInfo};
use hdtreemap_core::scan::ScanSource;
use hdtreemap_core::state::ScanTarget;
use hdtreemap_core::{build_app, install_theme_and_fonts, AppState};

pub const WIDTH: f64 = 1200.0;
pub const HEIGHT: f64 = 800.0;
pub const GB: u64 = 1024 * 1024 * 1024;

/// A scripted machine: fixed volumes, a folder picker that answers `picked`, and scans that
/// stream the small demo USB stick at its realistic pace (about 1.5 s).
pub struct FakePlatform {
    pub volumes: Vec<VolumeInfo>,
    pub picked: Option<PathBuf>,
    pub pick_calls: Rc<Cell<u32>>,
}

impl PlatformServices for FakePlatform {
    fn list_volumes(&self) -> Vec<VolumeInfo> {
        self.volumes.clone()
    }
    fn pick_folder(&self) -> Option<PathBuf> {
        self.pick_calls.set(self.pick_calls.get() + 1);
        self.picked.clone()
    }
    fn reveal(&self, _path: &Path) -> ActionResult {
        Ok(ActionOutcome::Done)
    }
    fn open(&self, _path: &Path) -> ActionResult {
        Ok(ActionOutcome::Done)
    }
    fn trash(&self, _path: &Path) -> ActionResult {
        Ok(ActionOutcome::Done)
    }
    fn is_demo(&self) -> bool {
        false
    }
    fn scan_source(&self, _target: &ScanTarget) -> Box<dyn ScanSource> {
        Box::new(DemoScanner::new(DemoProfile::UsbStick, DemoPace::Realistic))
    }
}

pub fn volume(name: &str, mount: &str, total: u64, free: u64) -> VolumeInfo {
    VolumeInfo {
        id: mount.to_string(),
        name: name.to_string(),
        mount_path: PathBuf::from(mount),
        total_bytes: total,
        free_bytes: free,
        description: None,
    }
}

pub fn fake(volumes: Vec<VolumeInfo>, picked: Option<&str>) -> (FakePlatform, Rc<Cell<u32>>) {
    let calls = Rc::new(Cell::new(0));
    let platform = FakePlatform {
        volumes,
        picked: picked.map(PathBuf::from),
        pick_calls: Rc::clone(&calls),
    };
    (platform, calls)
}

/// The real app over a platform, laid out and painted once.
pub struct Harness {
    pub state: AppState,
    pub app: App,
}

impl Harness {
    pub fn new(platform: Rc<dyn PlatformServices>) -> Self {
        Self::with_state(AppState::new(), platform)
    }

    pub fn with_state(state: AppState, platform: Rc<dyn PlatformServices>) -> Self {
        let fonts = install_theme_and_fonts(1.0).expect("bundled fonts load");
        let app = build_app(state.clone(), platform, fonts);
        let mut h = Self { state, app };
        h.frame();
        h
    }

    /// One shell frame: drain queued UI work (and the scan pump), lay out, paint.
    pub fn frame(&mut self) {
        agg_gui::ui_thread::invoke_pending_actions();
        self.app.layout(Size::new(WIDTH, HEIGHT));
        let mut fb = Framebuffer::new(WIDTH as u32, HEIGHT as u32);
        let mut ctx = GfxCtx::new(&mut fb);
        self.app.paint(&mut ctx);
    }

    /// Run frames until `done` holds (at most `max` frames). Returns whether it did.
    pub fn frames_until(&mut self, max: usize, mut done: impl FnMut(&Self) -> bool) -> bool {
        for _ in 0..max {
            if done(self) {
                return true;
            }
            self.frame();
        }
        done(self)
    }

    pub fn visible(&self, id: &str) -> bool {
        find_widget_by_id(self.app.root(), id).is_some()
            && find_widget_screen_rect(self.app.root(), id).is_some()
    }

    /// Click the centre of the widget `id` (Y-down screen coordinates, as a shell sends).
    pub fn click(&mut self, id: &str) {
        let rect = find_widget_screen_rect(self.app.root(), id)
            .unwrap_or_else(|| panic!("no visible widget {id:?}"));
        let x = rect.x + rect.width * 0.5;
        let y = HEIGHT - (rect.y + rect.height * 0.5);
        self.click_at(x, y);
    }

    pub fn click_at(&mut self, x: f64, y: f64) {
        self.app.on_mouse_move(x, y);
        self.app
            .on_mouse_down(x, y, MouseButton::Left, Modifiers::default());
        self.app
            .on_mouse_up(x, y, MouseButton::Left, Modifiers::default());
        self.frame();
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        // Leave the thread's UI queue to whoever runs next on this test thread.
        agg_gui::ui_thread::unbind_current_thread();
    }
}

/// Start `state` scanning `path` with the fake platform's source (the CLI-path start-up).
pub fn scanning_state(path: &str) -> AppState {
    let state = AppState::new();
    let (platform, _) = fake(Vec::new(), None);
    let target = ScanTarget::folder(path);
    state.start_scan(target.clone(), platform.scan_source(&target));
    state
}
