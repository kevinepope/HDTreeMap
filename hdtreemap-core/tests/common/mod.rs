//! Shared harness for the headless UI tests: the real production tree (`build_app`) over a
//! fake or demo `PlatformServices`, with no window and no GPU. Each frame drains agg-gui's
//! `ui_thread` queue (which runs the scan pump, as a shell does), lays out, and paints into
//! a software framebuffer; clicks go through the same `App` mouse entry points a shell uses.
//! Widgets are found by their reflection ids.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use agg_gui::widget::{find_widget_by_id, find_widget_screen_rect};
use agg_gui::{App, Framebuffer, GfxCtx, Key, Label, Modifiers, MouseButton, Rect, Size};
use hdtreemap_core::demo_fs::{DemoPace, DemoProfile, DemoScanner};
use hdtreemap_core::platform::{
    ActionOutcome, ActionResult, PlatformError, PlatformServices, VolumeInfo,
};
use hdtreemap_core::scan::ScanSource;
use hdtreemap_core::state::ScanTarget;
use hdtreemap_core::ui::context_menu::{ContextMenuHost, CONTEXT_MENU_ID};
use hdtreemap_core::ui::toast::{Toasts, TOAST_ID};
use hdtreemap_core::{build_app, install_theme_and_fonts, AppState};

pub const WIDTH: f64 = 1200.0;
pub const HEIGHT: f64 = 800.0;
pub const GB: u64 = 1024 * 1024 * 1024;

/// A scripted machine: fixed volumes, a folder picker that answers `picked`, scans that
/// stream the small demo USB stick at its realistic pace (about 1.5 s), and OS actions that
/// are recorded in `calls` (`"reveal /path"`, `"open /path"`, `"trash /path"`,
/// `"list_volumes"`) and succeed unless `trash_error` is set.
pub struct FakePlatform {
    pub volumes: Vec<VolumeInfo>,
    pub picked: Option<PathBuf>,
    pub pick_calls: Rc<Cell<u32>>,
    pub calls: Rc<RefCell<Vec<String>>>,
    pub trash_error: Option<String>,
    pub pace: DemoPace,
}

impl FakePlatform {
    fn record(&self, call: String) {
        self.calls.borrow_mut().push(call);
    }
}

impl PlatformServices for FakePlatform {
    fn list_volumes(&self) -> Vec<VolumeInfo> {
        self.record("list_volumes".to_string());
        self.volumes.clone()
    }
    fn pick_folder(&self) -> Option<PathBuf> {
        self.pick_calls.set(self.pick_calls.get() + 1);
        self.picked.clone()
    }
    fn reveal(&self, path: &Path) -> ActionResult {
        self.record(format!("reveal {}", path.display()));
        Ok(ActionOutcome::Done)
    }
    fn open(&self, path: &Path) -> ActionResult {
        self.record(format!("open {}", path.display()));
        Ok(ActionOutcome::Done)
    }
    fn trash(&self, path: &Path) -> ActionResult {
        self.record(format!("trash {}", path.display()));
        match &self.trash_error {
            Some(message) => Err(PlatformError::new(message.clone())),
            None => Ok(ActionOutcome::Done),
        }
    }
    fn open_url(&self, url: &str) -> ActionResult {
        self.record(format!("open_url {url}"));
        Ok(ActionOutcome::Done)
    }
    fn is_demo(&self) -> bool {
        false
    }
    fn scan_source(&self, target: &ScanTarget) -> Box<dyn ScanSource> {
        self.record(format!("scan {}", target.path.display()));
        Box::new(DemoScanner::new(DemoProfile::UsbStick, self.pace))
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
        calls: Rc::new(RefCell::new(Vec::new())),
        trash_error: None,
        pace: DemoPace::Realistic,
    };
    (platform, calls)
}

/// The real app over a platform, laid out and painted once.
pub struct Harness {
    pub state: AppState,
    pub app: App,
    /// The window size (logical = physical: the harness runs at scale 1).
    pub size: Size,
}

impl Harness {
    pub fn new(platform: Rc<dyn PlatformServices>) -> Self {
        Self::with_state(AppState::new(), platform)
    }

    pub fn with_state(state: AppState, platform: Rc<dyn PlatformServices>) -> Self {
        let fonts = install_theme_and_fonts(1.0).expect("bundled fonts load");
        let app = build_app(state.clone(), platform, fonts);
        let mut h = Self {
            state,
            app,
            size: Size::new(WIDTH, HEIGHT),
        };
        h.frame();
        h
    }

    /// One shell frame: drain queued UI work (and the scan pump), lay out, paint.
    pub fn frame(&mut self) {
        agg_gui::ui_thread::invoke_pending_actions();
        self.app.layout(self.size);
        let mut fb = Framebuffer::new(self.size.width as u32, self.size.height as u32);
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

    /// The on-screen rectangle of widget `id` (Y-up, as agg-gui reports it).
    pub fn rect(&self, id: &str) -> Rect {
        find_widget_screen_rect(self.app.root(), id)
            .unwrap_or_else(|| panic!("no visible widget {id:?}"))
    }

    /// The shell (Y-down) point at fractions `(fx, fy)` of widget `id`, measured from its
    /// top-left corner.
    pub fn point_in(&self, id: &str, fx: f64, fy: f64) -> (f64, f64) {
        let r = self.rect(id);
        (
            r.x + r.width * fx,
            self.size.height - (r.y + r.height * (1.0 - fy)),
        )
    }

    /// The shell (Y-down) point at the centre of `local`, a rectangle in widget `id`'s own
    /// (Y-up) coordinates.
    pub fn point_at_local(&self, id: &str, local: Rect) -> (f64, f64) {
        let r = self.rect(id);
        (
            r.x + local.x + local.width / 2.0,
            self.size.height - (r.y + local.y + local.height / 2.0),
        )
    }

    /// The widget `id` downcast to its concrete type.
    pub fn widget<T: 'static>(&self, id: &str) -> &T {
        find_widget_by_id(self.app.root(), id)
            .and_then(|w| w.as_any())
            .and_then(|a| a.downcast_ref::<T>())
            .unwrap_or_else(|| panic!("no {} {id:?}", std::any::type_name::<T>()))
    }

    /// Click the centre of the widget `id` (Y-down screen coordinates, as a shell sends).
    pub fn click(&mut self, id: &str) {
        let (x, y) = self.point_in(id, 0.5, 0.5);
        self.click_at(x, y);
    }

    /// Move the pointer to `(x, y)` (Y-down) and run a frame.
    pub fn move_to(&mut self, x: f64, y: f64) {
        self.app.on_mouse_move(x, y);
        self.frame();
    }

    /// A double click at `(x, y)` (Y-down): the second press states a click count of 2.
    pub fn double_click_at(&mut self, x: f64, y: f64) {
        self.click_at(x, y);
        self.app
            .on_mouse_down_clicks(x, y, MouseButton::Left, Modifiers::default(), 2);
        self.app
            .on_mouse_up(x, y, MouseButton::Left, Modifiers::default());
        self.frame();
    }

    /// Press and release `key` with no modifiers, then run a frame.
    pub fn key(&mut self, key: Key) {
        self.app.on_key_down(key.clone(), Modifiers::default());
        self.app.on_key_up(key, Modifiers::default());
        self.frame();
    }

    /// Resize the window and run a frame.
    pub fn resize(&mut self, width: f64, height: f64) {
        self.size = Size::new(width, height);
        self.frame();
    }

    /// The text of the `Label` with id `id`.
    pub fn label_text(&self, id: &str) -> String {
        find_widget_by_id(self.app.root(), id)
            .and_then(|w| w.as_any())
            .and_then(|a| a.downcast_ref::<Label>())
            .map(|l| l.text_str().to_string())
            .unwrap_or_else(|| panic!("no label {id:?}"))
    }

    /// A right click at `(x, y)` (Y-down), then a frame.
    pub fn right_click_at(&mut self, x: f64, y: f64) {
        self.app.on_mouse_move(x, y);
        self.app
            .on_mouse_down(x, y, MouseButton::Right, Modifiers::default());
        self.app
            .on_mouse_up(x, y, MouseButton::Right, Modifiers::default());
        self.frame();
    }

    /// Click the centre of `rect`, given in root coordinates (Y up).
    pub fn click_root_rect(&mut self, rect: Rect) {
        let x = rect.x + rect.width / 2.0;
        let y = self.size.height - (rect.y + rect.height / 2.0);
        self.click_at(x, y);
    }

    /// The context menu host.
    pub fn context_menu(&self) -> &ContextMenuHost {
        self.widget::<ContextMenuHost>(CONTEXT_MENU_ID)
    }

    /// Click the open context menu's row labelled `label`.
    pub fn choose(&mut self, label: &str) {
        let rect = self
            .context_menu()
            .item_rect(label)
            .unwrap_or_else(|| panic!("no menu row {label:?}: {:?}", self.context_menu().items()));
        self.click_root_rect(rect);
    }

    /// Press `key` with `modifiers`, then run a frame.
    pub fn key_with(&mut self, key: Key, modifiers: Modifiers) {
        self.app.on_key_down(key.clone(), modifiers);
        self.app.on_key_up(key, modifiers);
        self.frame();
    }

    /// The toasts shown by the last paint.
    pub fn toasts(&self) -> Vec<String> {
        self.widget::<Toasts>(TOAST_ID).shown().to_vec()
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

/// A harness on the main view of a finished USB-stick demo scan, over a recording fake
/// platform (its scans also stream the USB stick, unpaced). Returns the call log.
pub fn fake_map() -> (Harness, Rc<RefCell<Vec<String>>>) {
    let (mut platform, _) = fake(Vec::new(), None);
    platform.pace = DemoPace::Unpaced;
    fake_map_with(platform)
}

/// [`fake_map`] over `platform`.
pub fn fake_map_with(platform: FakePlatform) -> (Harness, Rc<RefCell<Vec<String>>>) {
    use hdtreemap_core::platform::DemoPlatform;
    use hdtreemap_core::state::Screen;
    let calls = Rc::clone(&platform.calls);
    let state = AppState::new();
    let volume = DemoPlatform::unpaced()
        .list_volumes()
        .into_iter()
        .find(|v| v.id == "usb-stick")
        .expect("usb stick");
    let target = ScanTarget::volume(&volume);
    state.start_scan(target, platform.scan_source(&ScanTarget::folder("/x")));
    while state.pump(std::time::Duration::from_secs(5)) {}
    calls.borrow_mut().clear();
    let mut h = Harness::with_state(state, Rc::new(platform));
    h.frame();
    assert!(matches!(h.state.screen(), Screen::Results(_)));
    (h, calls)
}

/// A harness on the main view of a finished, unpaced scan of demo drive `id`.
pub fn demo_map(id: &str) -> Harness {
    use hdtreemap_core::platform::DemoPlatform;
    use hdtreemap_core::state::Screen;
    let state = AppState::new();
    let platform = DemoPlatform::unpaced();
    let volume = platform
        .list_volumes()
        .into_iter()
        .find(|v| v.id == id)
        .unwrap_or_else(|| panic!("no demo drive {id:?}"));
    let target = ScanTarget::volume(&volume);
    state.start_scan(target.clone(), platform.scan_source(&target));
    while state.pump(std::time::Duration::from_secs(5)) {}
    let mut h = Harness::with_state(state, Rc::new(platform));
    h.frame();
    assert!(matches!(h.state.screen(), Screen::Results(_)));
    h
}
