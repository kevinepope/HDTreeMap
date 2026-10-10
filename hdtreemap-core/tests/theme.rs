//! Headless UI tests of the light and dark themes: the app starts light (fresh settings or
//! a settings blob from before the theme existed); View ▸ Theme and the toolbar button
//! switch the look live and the choice is saved; a painted frame uses the look's
//! background; the map re-rasterises only when the look flips; switching mid-scan is safe.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use agg_gui::current_visuals;
use agg_gui::Key;
use common::{fake, fake_map, scanning_state, Harness};
use hdtreemap_core::platform::PlatformServices;
use hdtreemap_core::settings::{Settings, SettingsStore};
use hdtreemap_core::state::Screen;
use hdtreemap_core::theme::{visuals, Theme};
use hdtreemap_core::ui::commands::{Command, Commands};
use hdtreemap_core::ui::main_view::THEME_BUTTON_ID;
use hdtreemap_core::ui::menus::{menu_bar_spec, MenuSpec};
use hdtreemap_core::ui::start_screen::START_SCREEN_ID;
use hdtreemap_core::AppState;

/// A settings store in memory.
#[derive(Clone, Default)]
struct MemoryStore(Rc<RefCell<Option<String>>>);

impl SettingsStore for MemoryStore {
    fn load(&self) -> Option<String> {
        self.0.borrow().clone()
    }
    fn save(&self, blob: &str) {
        *self.0.borrow_mut() = Some(blob.to_string());
    }
}

fn close(a: [f32; 3], b: agg_gui::Color) -> bool {
    (a[0] - b.r).abs() < 0.02 && (a[1] - b.g).abs() < 0.02 && (a[2] - b.b).abs() < 0.02
}

/// A spot of bare start-screen panel: the bottom-right corner, beside the footer.
fn start_panel_pixel(h: &mut Harness) -> [f32; 3] {
    let (w, ht) = (h.size.width, h.size.height);
    h.pixel(w - 6.0, ht - 6.0)
}

fn start_harness(state: AppState) -> Harness {
    let (platform, _) = fake(Vec::new(), None);
    Harness::with_state(state, Rc::new(platform))
}

#[test]
fn the_app_starts_light_with_fresh_settings() {
    let mut h = start_harness(AppState::new());
    assert!(h.visible(START_SCREEN_ID));
    assert_eq!(h.state.theme(), Theme::Light);
    assert!(!current_visuals().is_dark());
    let px = start_panel_pixel(&mut h);
    assert!(
        close(px, visuals(false).panel_fill),
        "light panel, got {px:?}"
    );
}

#[test]
fn a_settings_file_from_before_the_theme_starts_light() {
    let store = MemoryStore::default();
    *store.0.borrow_mut() = Some(
        r#"{"size_mode": "allocated", "cushions": true, "color_by": "type",
            "side_split": 0.28, "tree_split": 0.6, "recent": []}"#
            .to_string(),
    );
    let state = AppState::new();
    state.use_settings_store(Box::new(store));
    let mut h = start_harness(state);
    assert_eq!(h.state.theme(), Theme::Light);
    assert!(close(start_panel_pixel(&mut h), visuals(false).panel_fill));
}

#[test]
fn a_remembered_dark_theme_paints_dark_from_the_first_frame() {
    let store = MemoryStore::default();
    *store.0.borrow_mut() = Some(r#"{"theme": "dark"}"#.to_string());
    let state = AppState::new();
    state.use_settings_store(Box::new(store));
    let mut h = start_harness(state);
    let px = start_panel_pixel(&mut h);
    assert!(
        close(px, visuals(true).panel_fill),
        "dark panel, got {px:?}"
    );
}

#[test]
fn the_theme_menu_lists_three_radio_items_and_switches_live() {
    let store = MemoryStore::default();
    let state = AppState::new();
    state.use_settings_store(Box::new(store.clone()));
    let mut h = start_harness(state);
    let (platform, _) = fake(Vec::new(), None);
    let platform: Rc<dyn PlatformServices> = Rc::new(platform);
    let commands = Commands::new(&h.state, &platform);
    let view = &menu_bar_spec(&commands, false)[2];
    let Some(MenuSpec::Submenu { label, items }) = view.items.last() else {
        panic!("View ends with the Theme submenu");
    };
    assert_eq!(label, "Theme");
    let themes: Vec<Command> = items
        .iter()
        .filter_map(|i| match i {
            MenuSpec::Item(item) => Some(item.command),
            _ => None,
        })
        .collect();
    assert_eq!(
        themes,
        Theme::ALL.map(Command::Theme).to_vec(),
        "Light, Dark, Match System"
    );
    for t in Theme::ALL {
        assert!(
            commands.enabled(Command::Theme(t)),
            "on the start screen too"
        );
        assert_eq!(commands.checked(Command::Theme(t)), Some(t == Theme::Light));
    }

    commands.run(Command::Theme(Theme::Dark));
    assert_eq!(commands.checked(Command::Theme(Theme::Dark)), Some(true));
    assert_eq!(commands.checked(Command::Theme(Theme::Light)), Some(false));
    let px = start_panel_pixel(&mut h);
    assert!(close(px, visuals(true).panel_fill), "dark now, got {px:?}");
    assert!(h.state.save_settings(true));
    let saved = Settings::from_json(&store.0.borrow().clone().unwrap_or_default());
    assert_eq!(saved.theme, Theme::Dark);

    commands.run(Command::Theme(Theme::System));
    assert_eq!(h.state.theme(), Theme::System);
    assert!(h.state.save_settings(true));
    let saved = Settings::from_json(&store.0.borrow().clone().unwrap_or_default());
    assert_eq!(saved.theme, Theme::System);
    // No OS appearance reported: Match System is light.
    assert!(close(start_panel_pixel(&mut h), visuals(false).panel_fill));
    h.state.set_system_dark(Some(true));
    assert!(close(start_panel_pixel(&mut h), visuals(true).panel_fill));
}

#[test]
fn the_in_window_menu_runs_the_theme_items() {
    let mut h = start_harness(AppState::new());
    h.click("View Menu");
    // Up from nothing: View's last row, the Theme submenu. Right opens it (no row
    // highlighted); Down, Down: Light, then Dark.
    h.key(Key::ArrowUp);
    h.key(Key::ArrowRight);
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert_eq!(h.state.theme(), Theme::Dark, "Theme ▸ Dark from the bar");
}

#[test]
fn the_main_view_follows_the_theme_and_the_toolbar_button_flips_it() {
    let (mut h, _) = fake_map();
    // The status bar's background: just above the window's bottom edge, at the right.
    let (w, ht) = (h.size.width, h.size.height);
    let bar = h.pixel(w - 4.0, ht - 3.0);
    assert!(
        close(bar, visuals(false).top_bar_bg),
        "light bar, got {bar:?}"
    );
    h.click(THEME_BUTTON_ID);
    assert_eq!(h.state.theme(), Theme::Dark);
    let bar = h.pixel(w - 4.0, ht - 3.0);
    assert!(
        close(bar, visuals(true).top_bar_bg),
        "dark bar, got {bar:?}"
    );
    h.click(THEME_BUTTON_ID);
    assert_eq!(h.state.theme(), Theme::Light);
}

#[test]
fn the_map_rerasterises_only_when_the_look_flips() {
    let (mut h, _) = fake_map();
    h.frame();
    let before = h.state.map_counters().rasters;
    // Light -> Match System with no OS appearance: still light, same picture.
    h.state.set_theme(Theme::System);
    h.frame();
    h.frame();
    assert_eq!(h.state.map_counters().rasters, before);
    h.state.set_theme(Theme::Dark);
    h.frame();
    assert_eq!(h.state.map_counters().rasters, before + 1);
    h.frame();
    assert_eq!(h.state.map_counters().rasters, before + 1);
}

#[test]
fn switching_themes_mid_scan_is_safe() {
    let state = scanning_state("/scan");
    let (platform, _) = fake(Vec::new(), None);
    let mut h = Harness::with_state(state, Rc::new(platform));
    let mut flips = 0;
    let finished = h.frames_until(2000, |h| {
        let next = if h.state.is_dark() {
            Theme::Light
        } else {
            Theme::Dark
        };
        h.state.set_theme(next);
        flips += 1;
        matches!(h.state.screen(), Screen::Results(_)) && !h.state.is_scanning()
    });
    assert!(finished, "the scan finished while the theme flipped");
    assert!(flips > 2);
}
