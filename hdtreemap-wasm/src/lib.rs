//! # WebAssembly shell for the HDTreeMap demo
//!
//! Thinnest possible browser shim: the canvas, the WebGPU / WebGL2 surface, the
//! requestAnimationFrame loop, DOM input and DPR tracking all belong to
//! `agg-gui-web-shell`. This crate installs the theme and fonts and boots the shared app
//! from `hdtreemap-core` with the simulated [`DemoPlatform`] (wrapped so web links open in
//! a new tab): the browser cannot read the visitor's disks, so the demo shows a set of
//! descriptive simulated drives. Settings (size mode, colours, splitters, recent drives)
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
use hdtreemap_core::state::ScanTarget;
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
    return window.open(url, '_blank', 'noopener') !== null; }")]
extern "C" {
    /// Open `url` in a new browser tab; false when the browser blocked it.
    fn hdtreemap_open_url(url: &str) -> bool;
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
    start(config, |init| {
        let fonts = install_theme_and_fonts(init.device_scale()).map_err(WebShellError::app)?;
        let state = AppState::new();
        state.use_settings_store(Box::new(BrowserSettings(LocalStorageSettings::new(
            SETTINGS_KEY,
        ))));
        let host = Host {
            state: state.clone(),
        };
        let app = build_app(state, Rc::new(WebPlatform::default()), fonts);
        Ok((app, host))
    })
    .map_err(|err| JsValue::from_str(&err.to_string()))
}
