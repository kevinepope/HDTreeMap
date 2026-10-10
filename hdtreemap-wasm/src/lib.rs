//! # WebAssembly shell for the HDTreeMap demo
//!
//! Thinnest possible browser shim: the canvas, the WebGPU / WebGL2 surface, the
//! requestAnimationFrame loop, DOM input and DPR tracking all belong to
//! `agg-gui-web-shell`. This crate installs the theme and fonts and boots the shared app
//! from `hdtreemap-core` with the simulated [`DemoPlatform`]: the browser cannot read the
//! visitor's disks, so the demo shows a set of descriptive simulated drives.
//!
//! On native targets the crate is empty (it is a workspace member only so fmt and clippy
//! cover it).

#![cfg(target_arch = "wasm32")]

use std::rc::Rc;

use agg_gui_web_shell::{start, NoHost, RedrawPolicy, WebShellConfig, WebShellError};
use hdtreemap_core::platform::DemoPlatform;
use hdtreemap_core::{build_app, install_theme_and_fonts, AppState, APP_TITLE};
use wasm_bindgen::prelude::*;

/// `id` of the canvas in `demo/index.html`.
const CANVAS_ID: &str = "hdtreemap-canvas";

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
        let app = build_app(AppState::new(), Rc::new(DemoPlatform), fonts);
        Ok((app, NoHost))
    })
    .map_err(|err| JsValue::from_str(&err.to_string()))
}
