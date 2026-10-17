//! The native macOS menu bar: hdtreemap-core's menu spec (`ui::menus::menu_bar_spec`,
//! in its macOS layout with the application menu first) converted to agg-gui-shell's
//! `MenuBarModel` and installed as the system menu bar. On other platforms `install` does
//! nothing and the app shows its in-window `MenuBar` instead.
//!
//! Everything is a closure over the app's `Commands`, evaluated when a menu opens: a menu's
//! rows (and the Scan Drive list) are rebuilt from the spec then, and enable states and
//! check marks are asked then. A picked item runs the same command the in-window menu and
//! the shortcuts run (agg-gui-shell queues it to the next idle pass of its loop). Each item
//! carries its spec's shortcut, so the menu shows its keys; agg-gui-shell lets the menu
//! claim only the Command chords, and the app's `CommandKeys` runs the rest.

use std::rc::Rc;

use agg_gui_shell::menu_bar::{self as native, MenuBarModel, MenuItemModel, MenuShortcut};
use hdtreemap_core::ui::commands::Commands;
use hdtreemap_core::ui::menus::{menu_bar_spec, MenuRole, MenuSpec};

/// Make the app's menus the system menu bar. Returns whether a native bar is installed
/// (macOS), in which case the window shows no in-window one.
pub fn install(commands: &Commands) -> bool {
    native::install(model(commands))
}

/// The menu bar as agg-gui-shell's model.
pub fn model(commands: &Commands) -> MenuBarModel {
    let menus = menu_bar_spec(commands, true)
        .iter()
        .enumerate()
        .map(|(i, top)| {
            let provider = commands.clone();
            MenuItemModel {
                text: top.title.to_string(),
                sub_menu_items: Some(Rc::new(move || {
                    menu_bar_spec(&provider, true)
                        .get(i)
                        .map(|t| items(&provider, &t.items))
                        .unwrap_or_default()
                })),
                ..MenuItemModel::default()
            }
        })
        .collect();
    MenuBarModel { menus }
}

fn role(role: MenuRole) -> native::MenuItemRole {
    match role {
        MenuRole::None => native::MenuItemRole::None,
        MenuRole::About => native::MenuItemRole::About,
        MenuRole::Quit => native::MenuItemRole::Quit,
        MenuRole::OpenFile => native::MenuItemRole::OpenFile,
    }
}

/// One menu's rows as native items.
fn items(commands: &Commands, specs: &[MenuSpec]) -> Vec<MenuItemModel> {
    specs
        .iter()
        .map(|spec| match spec {
            MenuSpec::Separator => MenuItemModel::separator(),
            MenuSpec::Submenu { label, items: rows } => {
                let provider = commands.clone();
                let rows = rows.clone();
                MenuItemModel {
                    text: label.clone(),
                    sub_menu_items: Some(Rc::new(move || items(&provider, &rows))),
                    ..MenuItemModel::default()
                }
            }
            MenuSpec::Item(item) => {
                let command = item.command;
                let (enabled, checked, run) =
                    (commands.clone(), commands.clone(), commands.clone());
                MenuItemModel {
                    text: item.label.clone(),
                    role: role(item.role),
                    shortcut: item.shortcut.and_then(MenuShortcut::parse),
                    is_enabled: Some(Rc::new(move || enabled.enabled(command))),
                    is_checked: commands.checked(command).is_some().then(|| {
                        Rc::new(move || checked.checked(command) == Some(true))
                            as Rc<dyn Fn() -> bool>
                    }),
                    action: Some(Rc::new(move || run.run(command))),
                    ..MenuItemModel::default()
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hdtreemap_core::platform::{DemoPlatform, PlatformServices};
    use hdtreemap_core::AppState;

    #[test]
    fn the_model_has_the_app_menu_first_and_live_rows() {
        let state = AppState::new();
        let platform: Rc<dyn PlatformServices> = Rc::new(crate::platform::NativePlatform);
        let commands = Commands::new(&state, &platform);
        let model = model(&commands);
        let titles: Vec<&str> = model.menus.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(titles, vec!["HDTreeMap", "File", "Edit", "View", "Help"]);
        let app_menu = native::children_of(&model.menus[0]);
        let roles: Vec<native::MenuItemRole> = app_menu.iter().map(|i| i.role).collect();
        assert_eq!(
            roles,
            vec![
                native::MenuItemRole::About,
                native::MenuItemRole::None,
                native::MenuItemRole::Quit
            ]
        );
        // Quit runs the app's command.
        let quit = app_menu.last().expect("quit");
        assert!(native::is_enabled(quit));
        if let Some(action) = &quit.action {
            action();
        }
        assert!(state.quit_requested());
        // View ▸ Shading's rows carry radio marks; on the start screen they are disabled.
        let view = native::children_of(&model.menus[3]);
        let shading = view.iter().find(|i| i.text == "Shading").expect("submenu");
        let rows = native::children_of(shading);
        assert_eq!(
            rows.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(),
            vec!["Cushion", "Flat", "Bevel", "City"]
        );
        for row in &rows {
            assert!(row.is_checked.is_some());
            assert!(!native::is_enabled(row));
        }
    }

    #[test]
    fn native_items_show_their_shortcuts() {
        let state = AppState::new();
        let platform: Rc<dyn PlatformServices> = Rc::new(crate::platform::NativePlatform);
        let commands = Commands::new(&state, &platform);
        let model = model(&commands);
        let shortcut = |menu: usize, text: &str| {
            native::children_of(&model.menus[menu])
                .into_iter()
                .find(|i| i.text == text)
                .unwrap_or_else(|| panic!("no item {text:?}"))
                .shortcut
        };
        assert_eq!(shortcut(1, "Rescan"), MenuShortcut::parse("Ctrl+R"));
        assert_eq!(shortcut(2, "Copy Path"), MenuShortcut::parse("Ctrl+C"));
        assert_eq!(shortcut(0, "Quit"), MenuShortcut::parse("Ctrl+Q"));
        assert!(shortcut(1, "Rescan").is_some());
        // An item without a shortcut has none.
        assert_eq!(shortcut(0, "About HDTreeMap"), None);
    }

    #[test]
    fn the_demo_model_has_no_open_folder() {
        let state = AppState::new();
        let platform: Rc<dyn PlatformServices> = Rc::new(DemoPlatform::unpaced());
        let commands = Commands::new(&state, &platform);
        let file = native::children_of(&model(&commands).menus[1]);
        assert!(!file.iter().any(|i| i.text.starts_with("Open Folder")));
    }
}
