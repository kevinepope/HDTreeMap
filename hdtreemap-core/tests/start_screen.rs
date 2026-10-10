//! Headless UI tests for the start screen and the target screen (`hdtreemap_core::ui`).
//!
//! Each test builds the real production tree (`build_app`) over a fake or demo
//! `PlatformServices`, with no window and no GPU, lays it out, paints it into a software
//! framebuffer, and drives it with the same `App` mouse entry points a shell uses. Widgets
//! are found by their reflection ids (`find_widget_by_id` / `find_widget_screen_rect`).

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use agg_gui::widget::{find_widget_by_id, find_widget_screen_rect};
use agg_gui::{App, Framebuffer, GfxCtx, Modifiers, MouseButton, Size};
use hdtreemap_core::platform::{
    ActionOutcome, ActionResult, DemoPlatform, PlatformServices, VolumeInfo,
};
use hdtreemap_core::state::{ScanTarget, Screen};
use hdtreemap_core::ui::start_screen::{
    volume_card_id, volume_scan_id, CHOOSE_FOLDER_ID, DEMO_BANNER_ID, DEMO_NOTE_ID, NO_VOLUMES_ID,
    START_SCREEN_ID,
};
use hdtreemap_core::ui::target_screen::{BACK_BUTTON_ID, TARGET_PATH_ID, TARGET_SCREEN_ID};
use hdtreemap_core::{build_app, install_theme_and_fonts, AppState};

const WIDTH: f64 = 1200.0;
const HEIGHT: f64 = 800.0;

/// A scripted machine: fixed volumes, a folder picker that answers `picked`.
struct FakePlatform {
    volumes: Vec<VolumeInfo>,
    picked: Option<PathBuf>,
    pick_calls: Rc<Cell<u32>>,
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
}

fn volume(name: &str, mount: &str, total: u64, free: u64) -> VolumeInfo {
    VolumeInfo {
        id: mount.to_string(),
        name: name.to_string(),
        mount_path: PathBuf::from(mount),
        total_bytes: total,
        free_bytes: free,
        description: None,
    }
}

fn fake(volumes: Vec<VolumeInfo>, picked: Option<&str>) -> (FakePlatform, Rc<Cell<u32>>) {
    let calls = Rc::new(Cell::new(0));
    let platform = FakePlatform {
        volumes,
        picked: picked.map(PathBuf::from),
        pick_calls: Rc::clone(&calls),
    };
    (platform, calls)
}

/// The real app over `platform`, laid out and painted once.
struct Harness {
    state: AppState,
    app: App,
}

impl Harness {
    fn new(platform: Rc<dyn PlatformServices>) -> Self {
        Self::with_state(AppState::new(), platform)
    }

    fn with_state(state: AppState, platform: Rc<dyn PlatformServices>) -> Self {
        let fonts = install_theme_and_fonts(1.0).expect("bundled fonts load");
        let app = build_app(state.clone(), platform, fonts);
        let mut h = Self { state, app };
        h.frame();
        h
    }

    /// Lay out and paint into a throwaway software framebuffer, like one shell frame.
    fn frame(&mut self) {
        self.app.layout(Size::new(WIDTH, HEIGHT));
        let mut fb = Framebuffer::new(WIDTH as u32, HEIGHT as u32);
        let mut ctx = GfxCtx::new(&mut fb);
        self.app.paint(&mut ctx);
    }

    fn visible(&self, id: &str) -> bool {
        find_widget_by_id(self.app.root(), id).is_some()
            && find_widget_screen_rect(self.app.root(), id).is_some()
    }

    /// Click the centre of the widget `id` (Y-down screen coordinates, as a shell sends).
    fn click(&mut self, id: &str) {
        let rect = find_widget_screen_rect(self.app.root(), id)
            .unwrap_or_else(|| panic!("no visible widget {id:?}"));
        let x = rect.x + rect.width * 0.5;
        let y = HEIGHT - (rect.y + rect.height * 0.5);
        self.app.on_mouse_move(x, y);
        self.app
            .on_mouse_down(x, y, MouseButton::Left, Modifiers::default());
        self.app
            .on_mouse_up(x, y, MouseButton::Left, Modifiers::default());
        self.frame();
    }
}

const GB: u64 = 1024 * 1024 * 1024;

#[test]
fn native_start_screen_lists_volumes_and_offers_choose_folder() {
    let (platform, _) = fake(
        vec![
            volume("Macintosh HD", "/", 500 * GB, 120 * GB),
            volume("Backup", "/Volumes/Backup", 2000 * GB, 1900 * GB),
        ],
        None,
    );
    let h = Harness::new(Rc::new(platform));
    assert!(h.visible(START_SCREEN_ID));
    assert!(h.visible(&volume_card_id(0)));
    assert!(h.visible(&volume_card_id(1)));
    assert!(!h.visible(&volume_card_id(2)));
    assert!(h.visible(CHOOSE_FOLDER_ID));
    assert!(!h.visible(DEMO_BANNER_ID), "no demo banner natively");
    assert!(!h.visible(DEMO_NOTE_ID));
    assert!(!h.visible(NO_VOLUMES_ID));
}

#[test]
fn scanning_a_volume_records_it_and_back_returns() {
    let (platform, _) = fake(
        vec![
            volume("Macintosh HD", "/", 500 * GB, 120 * GB),
            volume("Backup", "/Volumes/Backup", 2000 * GB, 1900 * GB),
        ],
        None,
    );
    let mut h = Harness::new(Rc::new(platform));
    h.click(&volume_scan_id(1));
    let Screen::Target(target) = h.state.screen() else {
        panic!("Scan did not choose a target");
    };
    assert_eq!(target.path, PathBuf::from("/Volumes/Backup"));
    assert_eq!(target.volume_name.as_deref(), Some("Backup"));
    assert!(h.visible(TARGET_SCREEN_ID));
    assert!(h.visible(TARGET_PATH_ID));
    assert!(!h.visible(START_SCREEN_ID));

    h.click(BACK_BUTTON_ID);
    assert_eq!(h.state.screen(), Screen::Start);
    assert!(h.visible(START_SCREEN_ID));
}

#[test]
fn choose_folder_uses_the_picker() {
    let (platform, calls) = fake(vec![volume("Disk", "/", 10 * GB, 5 * GB)], Some("/picked"));
    let mut h = Harness::new(Rc::new(platform));
    h.click(CHOOSE_FOLDER_ID);
    assert_eq!(calls.get(), 1);
    assert_eq!(
        h.state.screen(),
        Screen::Target(ScanTarget::folder("/picked"))
    );
    assert!(h.visible(TARGET_SCREEN_ID));
}

#[test]
fn a_cancelled_picker_stays_on_the_start_screen() {
    let (platform, calls) = fake(vec![volume("Disk", "/", 10 * GB, 5 * GB)], None);
    let mut h = Harness::new(Rc::new(platform));
    h.click(CHOOSE_FOLDER_ID);
    assert_eq!(calls.get(), 1);
    assert_eq!(h.state.screen(), Screen::Start);
    assert!(h.visible(START_SCREEN_ID));
}

#[test]
fn no_volumes_shows_a_hint() {
    let (platform, _) = fake(Vec::new(), None);
    let h = Harness::new(Rc::new(platform));
    assert!(h.visible(NO_VOLUMES_ID));
    assert!(h.visible(CHOOSE_FOLDER_ID));
}

#[test]
fn demo_start_screen_shows_simulated_drives_and_no_folder_picker() {
    let mut h = Harness::new(Rc::new(DemoPlatform));
    assert!(h.visible(DEMO_BANNER_ID));
    assert!(h.visible(DEMO_NOTE_ID));
    assert!(
        !h.visible(CHOOSE_FOLDER_ID),
        "the browser cannot pick folders"
    );
    for i in 0..6 {
        assert!(
            find_widget_by_id(h.app.root(), &volume_card_id(i)).is_some(),
            "card {i}"
        );
    }
    h.click(&volume_scan_id(0));
    let Screen::Target(target) = h.state.screen() else {
        panic!("Scan did not choose a target");
    };
    assert_eq!(target.volume_id.as_deref(), Some("laptop-ssd"));
}

#[test]
fn a_command_line_path_opens_on_the_target_screen() {
    let (platform, _) = fake(vec![volume("Disk", "/", 10 * GB, 5 * GB)], None);
    let state = AppState::with_initial_path(Some(PathBuf::from("/from/cli")));
    let h = Harness::with_state(state, Rc::new(platform));
    assert!(h.visible(TARGET_SCREEN_ID));
    assert!(!h.visible(START_SCREEN_ID));
}

#[test]
fn small_windows_lay_out_without_panicking() {
    let mut h = Harness::new(Rc::new(DemoPlatform));
    for (w, ht) in [(640.0, 480.0), (320.0, 240.0), (1.0, 1.0)] {
        h.app.layout(Size::new(w, ht));
    }
    h.frame();
}
