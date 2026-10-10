//! # WebAssembly shell for the HDTreeMap demo
//!
//! Thinnest possible browser shim: the canvas, the WebGPU / WebGL2 surface, the
//! requestAnimationFrame loop, DOM input and DPR tracking all belong to
//! `agg-gui-web-shell`. This crate installs the theme and fonts and boots the shared app
//! from `hdtreemap-core` with the simulated [`DemoPlatform`] (wrapped so web links open in
//! a new tab): the browser cannot read the visitor's disks, so the demo shows a set of
//! descriptive simulated drives. `?drive=<id>` in the page URL starts scanning that drive
//! at once (as `--demo <id>` does natively), `?stats` logs `HDTREEMAP_STATS`-style timing
//! lines to the console, View ▸ Theme ▸ Match System follows the browser's
//! `prefers-color-scheme` (watched with `matchMedia`, so it switches live), and
//! [`hdtreemap_status`] reports the screen
//! and scan state for page automation (load and frame-time checks). Settings (size mode, colours, splitters, recent drives)
//! are remembered in `localStorage` under [`SETTINGS_KEY`]; when storage is unavailable
//! (private mode, blocked) the demo simply starts from the defaults.
//!
//! On native targets the crate is empty (it is a workspace member only so fmt and clippy
//! cover it).

#![cfg(target_arch = "wasm32")]

use std::rc::Rc;

use agg_gui_web_shell::agg_gui::App;
use agg_gui_web_shell::{
    start, LocalStorageSettings, RedrawPolicy, SettingsStore as WebSettingsStore, WebShellConfig,
    WebShellControl, WebShellError, WebShellHost,
};
use std::path::{Path, PathBuf};

use hdtreemap_core::platform::{
    ActionOutcome, ActionResult, DemoPlatform, PlatformError, PlatformServices, VolumeInfo,
};
use hdtreemap_core::scan::ScanSource;
use hdtreemap_core::settings::SettingsStore;
use hdtreemap_core::state::{ScanTarget, Screen};
use hdtreemap_core::{build_app, install_theme_and_fonts, AppState, APP_TITLE};
use wasm_bindgen::prelude::*;

/// `id` of the canvas in `demo/index.html`.
const CANVAS_ID: &str = "hdtreemap-canvas";

/// The `localStorage` key of the settings blob.
pub const SETTINGS_KEY: &str = "hdtreemap.settings";

/// The settings blob in `localStorage`; every failure means "nothing stored" or "not
/// saved" (the web shell's store already never throws).
struct BrowserSettings(LocalStorageSettings);

impl SettingsStore for BrowserSettings {
    fn load(&self) -> Option<String> {
        self.0.load()
    }

    fn save(&self, blob: &str) {
        let _ = WebSettingsStore::save(&self.0, blob);
    }
}

/// Saves the settings when they change (pointer idle) and when the page is hidden.
struct Host {
    state: AppState,
}

impl WebShellHost for Host {
    fn on_idle(&mut self, _app: &mut App, control: &mut WebShellControl<'_>) {
        self.state.save_settings(control.pointer_idle());
    }

    fn on_page_hide(&mut self, _app: &mut App) {
        self.state.flush_settings();
    }
}

#[wasm_bindgen(inline_js = "export function hdtreemap_open_url(url) { \
    return window.open(url, '_blank', 'noopener') !== null; } \
    export function hdtreemap_url_param(name) { \
    try { return new URLSearchParams(window.location.search).get(name); } \
    catch (e) { return null; } } \
    export function hdtreemap_console_log(line) { console.log(line); } \
    function hdtreemap_scheme() { \
    try { return window.matchMedia ? window.matchMedia('(prefers-color-scheme: dark)') : null; } \
    catch (e) { return null; } } \
    export function hdtreemap_prefers_dark() { \
    const q = hdtreemap_scheme(); return q ? q.matches : undefined; } \
    export function hdtreemap_watch_color_scheme(cb) { \
    const q = hdtreemap_scheme(); if (!q || !q.addEventListener) { return false; } \
    q.addEventListener('change', (e) => cb(e.matches)); return true; }")]
extern "C" {
    /// Open `url` in a new browser tab; false when the browser blocked it.
    fn hdtreemap_open_url(url: &str) -> bool;
    /// The page URL's parameter `name` (`None` when absent; `Some("")` for a bare flag).
    fn hdtreemap_url_param(name: &str) -> Option<String>;
    /// `console.log(line)`.
    fn hdtreemap_console_log(line: &str);
    /// The page's `prefers-color-scheme: dark` (`None` without `matchMedia`).
    fn hdtreemap_prefers_dark() -> Option<bool>;
    /// Call `cb(dark)` whenever `prefers-color-scheme` changes; false when it cannot.
    fn hdtreemap_watch_color_scheme(cb: &Closure<dyn Fn(bool)>) -> bool;
}

thread_local! {
    /// The app's state, for [`hdtreemap_status`].
    static STATE: std::cell::RefCell<Option<AppState>> = const { std::cell::RefCell::new(None) };
}

/// The app's state for page automation, as JSON: `{"screen": "start" | "scanning" |
/// "results", "files": .., "live_ticks": .., "rasters": .., "theme": "light" | "dark" |
/// "system", "dark": ..}` (files scanned so far, or in the result; whether the look shown
/// is dark).
#[wasm_bindgen]
pub fn hdtreemap_status() -> String {
    STATE.with(|s| {
        let state = s.borrow();
        let Some(state) = state.as_ref() else {
            return "{\"screen\": \"loading\"}".to_string();
        };
        let screen = match state.screen() {
            Screen::Start => "start",
            Screen::Scanning(_) if state.is_scanning() => "scanning",
            Screen::Scanning(_) | Screen::Results(_) => "results",
        };
        let files = state
            .progress()
            .map(|p| p.files)
            .or_else(|| state.with_result(|r, _| r.tree.file_count()))
            .unwrap_or(0);
        format!(
            "{{\"screen\": \"{screen}\", \"files\": {files}, \"live_ticks\": {}, \
             \"rasters\": {}, \"theme\": \"{}\", \"dark\": {}}}",
            state.live_ticks(),
            state.map_counters().rasters,
            state.theme().key(),
            state.is_dark()
        )
    })
}

/// The browser's platform: the simulated [`DemoPlatform`], except that web links really
/// open (in a new tab).
#[derive(Default)]
struct WebPlatform {
    demo: DemoPlatform,
}

impl PlatformServices for WebPlatform {
    fn list_volumes(&self) -> Vec<VolumeInfo> {
        self.demo.list_volumes()
    }
    fn pick_folder(&self) -> Option<PathBuf> {
        self.demo.pick_folder()
    }
    fn reveal(&self, path: &Path) -> ActionResult {
        self.demo.reveal(path)
    }
    fn open(&self, path: &Path) -> ActionResult {
        self.demo.open(path)
    }
    fn trash(&self, path: &Path) -> ActionResult {
        self.demo.trash(path)
    }
    fn open_url(&self, url: &str) -> ActionResult {
        if hdtreemap_open_url(url) {
            Ok(ActionOutcome::Done)
        } else {
            Err(PlatformError::new(format!(
                "The browser blocked a new tab. Open {url} yourself."
            )))
        }
    }
    fn is_demo(&self) -> bool {
        true
    }
    fn scan_source(&self, target: &ScanTarget) -> Box<dyn ScanSource> {
        self.demo.scan_source(target)
    }
}

#[wasm_bindgen(start)]
pub fn main() -> Result<(), JsValue> {
    let config = WebShellConfig::new(CANVAS_ID)
        .with_app_name(APP_TITLE)
        .with_device_label("hdtreemap-wasm")
        .with_redraw_policy(RedrawPolicy::Reactive);
    // Errors are already on the console and in the shell's fatal panel; returning them
    // also fails the wasm start so the page's boot code sees it.
    // `?stats`: timing lines (layout, raster, live ticks, frames) in the console.
    if hdtreemap_url_param("stats").is_some() {
        hdtreemap_core::render::enable_stats(hdtreemap_console_log);
    }
    start(config, |init| {
        let fonts = install_theme_and_fonts(init.device_scale()).map_err(WebShellError::app)?;
        let state = AppState::new();
        state.use_settings_store(Box::new(BrowserSettings(LocalStorageSettings::new(
            SETTINGS_KEY,
        ))));
        let platform = Rc::new(WebPlatform::default());
        let drive = hdtreemap_url_param("drive").unwrap_or_default();
        if let Some(volume) = platform.list_volumes().into_iter().find(|v| v.id == drive) {
            let target = ScanTarget::volume(&volume);
            let source = platform.scan_source(&target);
            state.start_scan(target, source);
        }
        // Match System: the browser's colour scheme, now and as it changes.
        state.set_system_dark(hdtreemap_prefers_dark());
        let watched = state.clone();
        let on_change = Closure::<dyn Fn(bool)>::new(move |dark| {
            watched.set_system_dark(Some(dark));
        });
        if hdtreemap_watch_color_scheme(&on_change) {
            // Lives as long as the page.
            on_change.forget();
        }
        STATE.with(|s| *s.borrow_mut() = Some(state.clone()));
        let host = Host {
            state: state.clone(),
        };
        let app = build_app(state, platform, fonts);
        Ok((app, host))
    })
    .map_err(|err| JsValue::from_str(&err.to_string()))
}
