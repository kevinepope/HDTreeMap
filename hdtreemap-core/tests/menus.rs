//! Headless UI tests of the menu bar and keyboard shortcuts (M4): the in-window bar lists
//! File, Edit, View and Help with their items; opening a menu and choosing an item runs it;
//! every item's command round-trips through its action id and runs; the shortcuts work; the
//! demo leaves out what only a real machine can do; the native macOS layout moves About and
//! Quit to the application menu.

mod common;

use std::rc::Rc;

use agg_gui::platform::{current_platform, Platform};
use agg_gui::widget::find_widget_by_id;
use agg_gui::{Key, Modifiers};
use common::{fake_map, Harness};
use hdtreemap_core::model::{NodeId, SizeMode};
use hdtreemap_core::platform::{DemoPlatform, PlatformServices};
use hdtreemap_core::render::Shading;
use hdtreemap_core::state::Screen;
use hdtreemap_core::ui::commands::{Command, Commands, GITHUB_URL};
use hdtreemap_core::ui::menus::{menu_bar_spec, MenuRole, MenuSpec, MENU_BAR_ID};
use hdtreemap_core::ui::sheets::{ABOUT_CLOSE_ID, ABOUT_SHEET_ID};

/// The platform's command modifier (Cmd on a Mac, Ctrl elsewhere).
fn command() -> Modifiers {
    if current_platform() == Platform::MacOS {
        Modifiers {
            meta: true,
            ..Default::default()
        }
    } else {
        Modifiers {
            ctrl: true,
            ..Default::default()
        }
    }
}

fn labels(items: &[MenuSpec]) -> Vec<String> {
    items
        .iter()
        .map(|i| match i {
            MenuSpec::Item(item) => item.label.clone(),
            MenuSpec::Submenu { label, .. } => format!("{label} >"),
            MenuSpec::Separator => "-".to_string(),
        })
        .collect()
}

fn all_commands(items: &[MenuSpec], out: &mut Vec<Command>) {
    for i in items {
        match i {
            MenuSpec::Item(item) => out.push(item.command),
            MenuSpec::Submenu { items, .. } => all_commands(items, out),
            MenuSpec::Separator => {}
        }
    }
}

#[test]
fn the_bar_lists_the_menus_and_their_items() {
    let (h, _) = fake_map();
    assert!(h.visible(MENU_BAR_ID));
    for title in ["File Menu", "Edit Menu", "View Menu", "Help Menu"] {
        assert!(h.visible(title), "{title}");
    }
    let platform: Rc<dyn PlatformServices> = Rc::new(common::fake(Vec::new(), None).0);
    let commands = Commands::new(&h.state, &platform);
    let spec = menu_bar_spec(&commands, false);
    let titles: Vec<&str> = spec.iter().map(|t| t.title).collect();
    assert_eq!(titles, vec!["File", "Edit", "View", "Help"]);
    assert_eq!(
        labels(&spec[0].items),
        vec![
            "Open Folder\u{2026}",
            "Scan Drive >",
            "-",
            "Rescan",
            "Back to Drives",
            "-",
            "Quit"
        ]
    );
    assert_eq!(
        labels(&spec[1].items),
        vec!["Copy Path", "Move to Trash\u{2026}"]
    );
    assert_eq!(
        labels(&spec[2].items),
        vec![
            "Size Mode >",
            "Colour By >",
            "Shading >",
            "-",
            "Zoom In",
            "Zoom Out",
            "Zoom to Root",
            "-",
            "Clear Highlight",
            "-",
            "Theme >"
        ]
    );
    assert_eq!(
        labels(&spec[3].items),
        vec!["About HDTreeMap", "View on GitHub"]
    );
    // Every command round-trips through its menu action id.
    let mut every = Vec::new();
    for top in &spec {
        all_commands(&top.items, &mut every);
    }
    for c in every {
        assert_eq!(Command::from_id(&c.id()), Some(c), "{c:?}");
    }
}

#[test]
fn menu_commands_run_and_report_their_state() {
    let (h, calls) = fake_map();
    let platform: Rc<dyn PlatformServices> = Rc::new({
        let (mut p, _) = common::fake(Vec::new(), None);
        p.calls = Rc::clone(&calls);
        p
    });
    let commands = Commands::new(&h.state, &platform);
    let state = &h.state;

    assert_eq!(
        commands.checked(Command::SizeMode(SizeMode::Allocated)),
        Some(true)
    );
    commands.run(Command::SizeMode(SizeMode::Apparent));
    assert_eq!(state.size_mode(), SizeMode::Apparent);
    assert_eq!(
        commands.checked(Command::SizeMode(SizeMode::Apparent)),
        Some(true)
    );

    assert_eq!(
        commands.checked(Command::Shading(Shading::Cushion)),
        Some(true)
    );
    commands.run(Command::Shading(Shading::Bevel));
    assert_eq!(state.shading(), Shading::Bevel);
    assert_eq!(
        commands.checked(Command::Shading(Shading::Cushion)),
        Some(false)
    );
    assert_eq!(
        commands.checked(Command::Shading(Shading::Bevel)),
        Some(true)
    );

    // Zoom In needs a selection below the zoom root.
    assert!(!commands.enabled(Command::ZoomIn));
    let folder = state
        .with_result(|r, _| {
            r.tree
                .children(r.tree.root())
                .find(|&c| r.tree.node(c).is_some_and(|n| n.is_dir()))
        })
        .flatten()
        .expect("folder");
    state.set_selected(Some(folder));
    commands.run(Command::ZoomIn);
    assert_eq!(state.zoom_root(), folder);
    assert!(commands.enabled(Command::ZoomOut));
    commands.run(Command::ZoomRoot);
    assert_eq!(state.zoom_root(), NodeId(0));

    commands.run(Command::ViewOnGitHub);
    assert_eq!(
        calls.borrow().last(),
        Some(&format!("open_url {GITHUB_URL}"))
    );

    commands.run(Command::About);
    assert!(state.about_sheet().get());

    commands.run(Command::BackToDrives);
    assert_eq!(state.screen(), Screen::Start);
    assert!(!commands.enabled(Command::Rescan));

    commands.run(Command::Quit);
    assert!(state.quit_requested());
}

#[test]
fn choosing_an_item_from_the_bar_runs_it() {
    let (mut h, _) = fake_map();
    h.click("Help Menu");
    // The first row of Help is About.
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert!(h.visible(ABOUT_SHEET_ID), "About opened from the Help menu");
    h.click(ABOUT_CLOSE_ID);
    assert!(!h.visible(ABOUT_SHEET_ID));
}

#[test]
fn keyboard_shortcuts_run_the_commands() {
    let _clipboard = agg_gui::clipboard::simulate();
    let (mut h, calls) = fake_map();
    let folder = h
        .state
        .with_result(|r, _| {
            r.tree
                .children(r.tree.root())
                .find(|&c| r.tree.node(c).is_some_and(|n| n.is_dir()))
        })
        .flatten()
        .expect("folder");
    h.state.set_selected(Some(folder));
    h.frame();
    h.key_with(Key::Char('c'), command());
    let path = h
        .state
        .with_result(|r, _| r.tree.path(folder).display().to_string())
        .expect("path");
    assert_eq!(agg_gui::clipboard::get_text(), Some(path));

    h.key_with(Key::ArrowDown, command());
    assert_eq!(h.state.zoom_root(), folder, "Zoom In");
    h.key_with(Key::ArrowUp, command());
    assert_eq!(h.state.zoom_root(), NodeId(0), "Zoom Out");

    h.key_with(Key::Char('r'), command());
    // The fake's unpaced scan finishes within the frame; the platform saw it start.
    assert!(
        calls.borrow().iter().any(|c| c.starts_with("scan ")),
        "Rescan: {:?}",
        calls.borrow()
    );
}

#[test]
fn the_demo_menus_leave_out_what_needs_a_real_machine() {
    let h = Harness::new(Rc::new(DemoPlatform::unpaced()));
    let platform: Rc<dyn PlatformServices> = Rc::new(DemoPlatform::unpaced());
    let commands = Commands::new(&h.state, &platform);
    let spec = menu_bar_spec(&commands, false);
    let file = labels(&spec[0].items);
    assert!(
        !file
            .iter()
            .any(|l| l.starts_with("Open Folder") || l == "Quit"),
        "{file:?}"
    );
    // Scan Drive lists the drives the start screen listed.
    let MenuSpec::Submenu { items, .. } = &spec[0].items[0] else {
        panic!("Scan Drive first");
    };
    assert_eq!(items.len(), 6);
    assert!(commands.enabled(Command::ScanVolume(5)));
    assert!(find_widget_by_id(h.app.root(), MENU_BAR_ID).is_some());
}

#[test]
fn the_native_mac_layout_puts_about_and_quit_in_the_app_menu() {
    let (h, _) = fake_map();
    let platform: Rc<dyn PlatformServices> = Rc::new(common::fake(Vec::new(), None).0);
    let commands = Commands::new(&h.state, &platform);
    let spec = menu_bar_spec(&commands, true);
    let titles: Vec<&str> = spec.iter().map(|t| t.title).collect();
    assert_eq!(titles, vec!["HDTreeMap", "File", "Edit", "View", "Help"]);
    let roles: Vec<MenuRole> = spec[0]
        .items
        .iter()
        .filter_map(|i| match i {
            MenuSpec::Item(item) => Some(item.role),
            _ => None,
        })
        .collect();
    assert_eq!(roles, vec![MenuRole::About, MenuRole::Quit]);
    assert!(!labels(&spec[1].items).contains(&"Quit".to_string()));
    assert_eq!(labels(&spec[4].items), vec!["View on GitHub"]);
}

#[test]
fn a_native_menu_bar_hides_the_in_window_one() {
    let state = hdtreemap_core::AppState::new();
    state.set_native_menu_bar(true);
    let h = Harness::with_state(state, Rc::new(DemoPlatform::unpaced()));
    assert!(!h.visible(MENU_BAR_ID));
}
