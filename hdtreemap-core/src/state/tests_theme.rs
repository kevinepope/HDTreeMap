//! Tests of the theme on the state side (`state/theme.rs`): the default, the remembered
//! choice, Match System, the palette's greys, and switching while a scan runs.

use std::rc::Rc;
use std::time::Duration;

use agg_gui::current_visuals;

use super::tests::{finished_usb, usb_scan, MemoryStore};
use super::*;
use crate::platform::PlatformServices;
use crate::settings::Settings;
use crate::theme::{map_tones, Theme};

#[test]
fn a_fresh_state_is_light() {
    let state = AppState::new();
    state.apply_theme();
    assert_eq!(state.theme(), Theme::Light);
    assert!(!state.is_dark());
    assert!(!current_visuals().is_dark());
}

#[test]
fn an_old_settings_blob_without_a_theme_starts_light() {
    let store = MemoryStore::default();
    *store.blob.borrow_mut() =
        Some(r#"{"size_mode": "apparent", "cushions": true, "recent": []}"#.to_string());
    let state = AppState::new();
    state.use_settings_store(Box::new(store));
    state.apply_theme();
    assert_eq!(state.theme(), Theme::Light);
    assert_eq!(state.size_mode(), SizeMode::Apparent);
    assert!(!current_visuals().is_dark());
}

#[test]
fn the_chosen_theme_switches_the_visuals_and_is_saved() {
    let store = MemoryStore::default();
    let state = AppState::new();
    state.use_settings_store(Box::new(store.clone()));
    state.apply_theme();
    let version = state.theme_version();
    state.set_theme(Theme::Dark);
    assert!(current_visuals().is_dark());
    assert!(state.theme_version() > version);
    assert!(state.save_settings(true));
    let saved = Settings::from_json(&store.blob.borrow().clone().unwrap_or_default());
    assert_eq!(saved.theme, Theme::Dark);
    state.toggle_theme();
    assert_eq!(state.theme(), Theme::Light);
    assert!(!current_visuals().is_dark());
}

#[test]
fn match_system_follows_the_os_and_is_light_when_unknown() {
    let state = AppState::new();
    state.apply_theme();
    state.set_theme(Theme::System);
    assert!(!state.is_dark(), "no OS appearance known: light");
    state.set_system_dark(Some(true));
    assert!(state.is_dark());
    assert!(current_visuals().is_dark());
    state.set_system_dark(Some(false));
    assert!(!current_visuals().is_dark());
    // The OS appearance does not matter to an explicit choice.
    state.set_theme(Theme::Light);
    state.set_system_dark(Some(true));
    assert!(!current_visuals().is_dark());
}

#[test]
fn the_palette_takes_the_looks_greys() {
    let state = finished_usb();
    state.apply_theme();
    let light = state.palette().expect("palette");
    assert_eq!(light.block(), map_tones(false).block);
    assert_eq!(light.background(), map_tones(false).background);
    state.set_theme(Theme::Dark);
    let dark = state.palette().expect("palette");
    assert!(
        !Rc::ptr_eq(&light, &dark),
        "a new palette, so the map re-rasterises"
    );
    assert_eq!(dark.block(), map_tones(true).block);
    assert_eq!(dark.background(), map_tones(true).background);
}

#[test]
fn switching_mid_scan_keeps_the_scan_running() {
    let state = AppState::new();
    state.apply_theme();
    let volume = crate::platform::DemoPlatform::default()
        .list_volumes()
        .into_iter()
        .next()
        .expect("a demo drive");
    state.start_scan(ScanTarget::volume(&volume), usb_scan());
    state.pump(Duration::from_millis(1));
    for theme in [Theme::Dark, Theme::System, Theme::Light, Theme::Dark] {
        state.set_theme(theme);
        state.set_system_dark(Some(theme == Theme::Dark));
        state.pump(Duration::from_millis(1));
    }
    while state.pump(Duration::from_secs(5)) {}
    assert!(matches!(state.screen(), Screen::Results(_)));
    assert!(state.is_dark());
}
