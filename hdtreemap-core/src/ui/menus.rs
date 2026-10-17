//! The app's menus, described once as data ([`menu_bar_spec`]) and turned into:
//!
//! - the in-window agg-gui `MenuBar` ([`build_menu_bar`]) on Windows, Linux and the web,
//!   whose entries are rebuilt each time a menu opens so enable states and check marks are
//!   current;
//! - the native macOS menu bar, which `hdtreemap-native` converts to agg-gui-shell's
//!   `MenuBarModel` from the same spec (the app menu gets About and Quit there);
//! - the keyboard shortcuts ([`CommandKeys`]), which work with either bar, or none.
//!
//! The right-click menu of a file or folder ([`context_entries`]) lists the node actions
//! (`ui/node_actions.rs`).

use std::sync::Arc;

use agg_gui::platform::{current_platform, Platform};
use agg_gui::widgets::menu::{MenuEntry, MenuItem, MenuShortcut};
use agg_gui::{
    DrawCtx, EventResult, Key, MenuBar, Modifiers, Rect, Size, TopMenu, Widget, WidgetBase,
};

use super::commands::{Command, Commands};
use super::node_actions::{label, NodeAction, NodeActions};
use crate::fonts::{fa, Fonts};
use crate::model::{NodeId, SizeMode};
use crate::render::{ColorBy, Shading};
use crate::theme::Theme;

/// Widget id of the in-window menu bar.
pub const MENU_BAR_ID: &str = "menu-bar";

/// What a native menu does with an item beyond running it (agg-gui-shell's roles).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuRole {
    None,
    About,
    Quit,
    OpenFile,
}

/// One menu command.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemSpec {
    pub label: String,
    pub command: Command,
    /// As `MenuShortcut::parse` reads it ("Ctrl" is Cmd on a Mac).
    pub shortcut: Option<&'static str>,
    pub icon: Option<char>,
    pub role: MenuRole,
}

/// A row of a menu.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuSpec {
    Item(ItemSpec),
    Submenu { label: String, items: Vec<MenuSpec> },
    Separator,
}

/// A top-level menu.
#[derive(Clone, Debug, PartialEq)]
pub struct TopMenuSpec {
    pub title: &'static str,
    pub items: Vec<MenuSpec>,
}

fn item(
    label: &str,
    command: Command,
    shortcut: Option<&'static str>,
    icon: Option<char>,
) -> MenuSpec {
    MenuSpec::Item(ItemSpec {
        label: label.to_string(),
        command,
        shortcut,
        icon,
        role: MenuRole::None,
    })
}

fn with_role(spec: MenuSpec, role: MenuRole) -> MenuSpec {
    match spec {
        MenuSpec::Item(mut item) => {
            item.role = role;
            MenuSpec::Item(item)
        }
        other => other,
    }
}

/// Move to Trash's shortcut: Cmd+Backspace on a Mac, Delete elsewhere.
pub fn trash_shortcut() -> &'static str {
    if current_platform() == Platform::MacOS {
        "Ctrl+Backspace"
    } else {
        "Delete"
    }
}

/// The menu bar. `native_mac` lays it out for the macOS system bar: an application menu
/// first holding About and Quit, which then leave Help and File.
pub fn menu_bar_spec(commands: &Commands, native_mac: bool) -> Vec<TopMenuSpec> {
    let demo = commands.platform().is_demo();
    let volumes = commands.state().volumes().unwrap_or_default();
    let mut drives: Vec<MenuSpec> = volumes
        .iter()
        .enumerate()
        .map(|(i, v)| item(&v.name, Command::ScanVolume(i), None, Some(fa::HDD)))
        .collect();
    if drives.is_empty() {
        drives.push(item(
            "No drives listed",
            Command::ScanVolume(usize::MAX),
            None,
            None,
        ));
    }
    let about = with_role(
        item(
            "About HDTreeMap",
            Command::About,
            None,
            Some(fa::INFO_CIRCLE),
        ),
        MenuRole::About,
    );
    let quit = with_role(
        item("Quit", Command::Quit, Some("Ctrl+Q"), Some(fa::POWER_OFF)),
        MenuRole::Quit,
    );

    let mut file = Vec::new();
    if !demo {
        file.push(with_role(
            item(
                "Open Folder\u{2026}",
                Command::OpenFolder,
                Some("Ctrl+O"),
                Some(fa::FOLDER_OPEN),
            ),
            MenuRole::OpenFile,
        ));
    }
    file.push(MenuSpec::Submenu {
        label: "Scan Drive".into(),
        items: drives,
    });
    file.push(MenuSpec::Separator);
    file.push(item(
        "Rescan",
        Command::Rescan,
        Some("Ctrl+R"),
        Some(fa::REFRESH),
    ));
    file.push(item(
        "Back to Drives",
        Command::BackToDrives,
        None,
        Some(fa::ARROW_LEFT),
    ));
    if !demo && !native_mac {
        file.push(MenuSpec::Separator);
        file.push(quit.clone());
    }

    let edit = vec![
        item(
            "Copy Path",
            Command::CopyPath,
            Some("Ctrl+C"),
            Some(fa::CLIPBOARD),
        ),
        item(
            "Move to Trash\u{2026}",
            Command::Trash,
            Some(trash_shortcut()),
            Some(fa::TRASH),
        ),
    ];

    let view = vec![
        MenuSpec::Submenu {
            label: "Size Mode".into(),
            items: vec![
                item(
                    "On Disk",
                    Command::SizeMode(SizeMode::Allocated),
                    None,
                    None,
                ),
                item(
                    "Apparent",
                    Command::SizeMode(SizeMode::Apparent),
                    None,
                    None,
                ),
            ],
        },
        MenuSpec::Submenu {
            label: "Colour By".into(),
            items: [ColorBy::Extension, ColorBy::TopFolder]
                .into_iter()
                .map(|by| item(by.label(), Command::ColorBy(by), None, None))
                .collect(),
        },
        MenuSpec::Submenu {
            label: "Shading".into(),
            items: Shading::ALL
                .into_iter()
                .map(|shading| item(shading.label(), Command::Shading(shading), None, None))
                .collect(),
        },
        MenuSpec::Separator,
        item(
            "Zoom In",
            Command::ZoomIn,
            Some("Ctrl+Down"),
            Some(fa::SEARCH_PLUS),
        ),
        item(
            "Zoom Out",
            Command::ZoomOut,
            Some("Ctrl+Up"),
            Some(fa::SEARCH_MINUS),
        ),
        item("Zoom to Root", Command::ZoomRoot, None, Some(fa::HOME)),
        MenuSpec::Separator,
        item(
            "Clear Highlight",
            Command::ClearHighlight,
            Some("Esc"),
            Some(fa::ERASER),
        ),
        MenuSpec::Separator,
        MenuSpec::Submenu {
            label: "Theme".into(),
            items: Theme::ALL
                .into_iter()
                .map(|theme| item(theme.label(), Command::Theme(theme), None, None))
                .collect(),
        },
    ];

    let mut help = Vec::new();
    if !native_mac {
        help.push(about.clone());
    }
    help.push(item(
        "View on GitHub",
        Command::ViewOnGitHub,
        None,
        Some(fa::GITHUB),
    ));

    let mut menus = Vec::new();
    if native_mac {
        menus.push(TopMenuSpec {
            title: "HDTreeMap",
            items: vec![about, MenuSpec::Separator, quit],
        });
    }
    menus.push(TopMenuSpec {
        title: "File",
        items: file,
    });
    menus.push(TopMenuSpec {
        title: "Edit",
        items: edit,
    });
    menus.push(TopMenuSpec {
        title: "View",
        items: view,
    });
    menus.push(TopMenuSpec {
        title: "Help",
        items: help,
    });
    menus
}

/// `items` as agg-gui menu entries, with enable states and marks as of now.
pub fn entries(commands: &Commands, items: &[MenuSpec]) -> Vec<MenuEntry> {
    items
        .iter()
        .map(|spec| match spec {
            MenuSpec::Separator => MenuEntry::Separator,
            MenuSpec::Submenu { label, items } => {
                MenuItem::submenu(label.clone(), entries(commands, items)).into()
            }
            MenuSpec::Item(spec) => {
                let mut row = MenuItem::action(spec.label.clone(), spec.command.id());
                if let Some(icon) = spec.icon {
                    row = row.icon(icon);
                }
                if let Some(shortcut) = spec.shortcut {
                    row = row.shortcut(shortcut);
                }
                row = match (spec.command, commands.checked(spec.command)) {
                    (
                        Command::SizeMode(_)
                        | Command::ColorBy(_)
                        | Command::Shading(_)
                        | Command::Theme(_),
                        Some(on),
                    ) => row.radio(on),
                    (_, Some(on)) => row.checked(on),
                    (_, None) => row,
                };
                if !commands.enabled(spec.command) {
                    row = row.disabled();
                }
                row.into()
            }
        })
        .collect()
}

/// The in-window menu bar.
pub fn build_menu_bar(commands: &Commands, fonts: &Fonts) -> MenuBar {
    let menus = menu_bar_spec(commands, false)
        .into_iter()
        .enumerate()
        .map(|(i, top)| {
            let provider = commands.clone();
            TopMenu::new(top.title, entries(commands, &top.items)).with_items_provider(move || {
                menu_bar_spec(&provider, false)
                    .get(i)
                    .map(|t| entries(&provider, &t.items))
                    .unwrap_or_default()
            })
        })
        .collect();
    let runner = commands.clone();
    MenuBar::new(Arc::clone(&fonts.ui), menus, move |id| {
        if let Some(command) = Command::from_id(id) {
            runner.run(command);
        }
    })
    .with_id(MENU_BAR_ID)
}

/// Every command with a shortcut, from the menu spec.
pub fn shortcuts(commands: &Commands) -> Vec<(MenuShortcut, Command)> {
    fn collect(items: &[MenuSpec], out: &mut Vec<(MenuShortcut, Command)>) {
        for spec in items {
            match spec {
                MenuSpec::Item(item) => {
                    if let Some(shortcut) = item.shortcut.and_then(MenuShortcut::parse) {
                        out.push((shortcut, item.command));
                    }
                }
                MenuSpec::Submenu { items, .. } => collect(items, out),
                MenuSpec::Separator => {}
            }
        }
    }
    let mut out = Vec::new();
    for top in menu_bar_spec(commands, false) {
        collect(&top.items, &mut out);
    }
    out
}

/// The right-click menu of `node`: every node action, with the trash after a separator.
pub fn context_entries(nodes: &NodeActions, node: NodeId) -> Vec<MenuEntry> {
    let mut out = Vec::new();
    for action in NodeAction::ALL {
        if action == NodeAction::Trash {
            out.push(MenuEntry::Separator);
        }
        let mut row =
            MenuItem::action(label(action, nodes.platform()), action.id()).icon(action.icon());
        if !nodes.enabled(node, action) {
            row = row.disabled();
        }
        out.push(row.into());
    }
    out
}

/// Wraps the app and runs the menu commands' keyboard shortcuts when nothing else used the
/// key (a focused control, or the in-window menu bar, which matches them first). Needed for
/// the macOS native menu, which only claims the chords of its standard roles.
pub struct CommandKeys {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    base: WidgetBase,
    commands: Commands,
}

impl CommandKeys {
    pub fn new(content: Box<dyn Widget>, commands: Commands) -> Self {
        Self {
            bounds: Rect::default(),
            children: vec![content],
            base: WidgetBase::new(),
            commands,
        }
    }
}

impl Widget for CommandKeys {
    fn type_name(&self) -> &'static str {
        "CommandKeys"
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    fn widget_base(&self) -> Option<&WidgetBase> {
        Some(&self.base)
    }

    fn widget_base_mut(&mut self) -> Option<&mut WidgetBase> {
        Some(&mut self.base)
    }

    fn layout(&mut self, available: Size) -> Size {
        let rect = Rect::new(0.0, 0.0, available.width, available.height);
        if let Some(child) = self.children.first_mut() {
            child.layout(available);
            child.set_bounds(rect);
        }
        self.bounds = rect;
        available
    }

    fn on_event(&mut self, _event: &agg_gui::Event) -> EventResult {
        EventResult::Ignored
    }

    fn paint(&mut self, _ctx: &mut dyn DrawCtx) {}

    fn on_unconsumed_key(&mut self, key: &Key, modifiers: Modifiers) -> EventResult {
        let matched = shortcuts(&self.commands)
            .into_iter()
            .find(|(shortcut, command)| {
                shortcut.matches(key, modifiers) && self.commands.enabled(*command)
            });
        match matched {
            Some((_, command)) => {
                self.commands.run(command);
                agg_gui::animation::request_draw();
                EventResult::Consumed
            }
            None => EventResult::Ignored,
        }
    }
}

/// The window's frame: an optional bar along the top at its natural height, and the
/// content laid out at exactly the rest of the size (so a screen's own flex layout gets a
/// real height to share out).
pub struct TopBarFrame {
    bounds: Rect,
    /// `[bar, content]` or `[content]`.
    children: Vec<Box<dyn Widget>>,
    has_bar: bool,
}

impl TopBarFrame {
    pub fn new(bar: Option<Box<dyn Widget>>, content: Box<dyn Widget>) -> Self {
        let has_bar = bar.is_some();
        let mut children: Vec<Box<dyn Widget>> = bar.into_iter().collect();
        children.push(content);
        Self {
            bounds: Rect::default(),
            children,
            has_bar,
        }
    }
}

impl Widget for TopBarFrame {
    fn type_name(&self) -> &'static str {
        "TopBarFrame"
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    fn layout(&mut self, available: Size) -> Size {
        let (w, h) = (available.width.max(0.0), available.height.max(0.0));
        let mut bar_h = 0.0;
        if self.has_bar {
            if let Some(bar) = self.children.first_mut() {
                bar_h = bar.layout(Size::new(w, h)).height.min(h);
                bar.set_bounds(Rect::new(0.0, h - bar_h, w, bar_h));
            }
        }
        let rest = (h - bar_h).max(0.0);
        if let Some(content) = self.children.last_mut() {
            content.layout(Size::new(w, rest));
            content.set_bounds(Rect::new(0.0, 0.0, w, rest));
        }
        self.bounds = Rect::new(0.0, 0.0, w, h);
        Size::new(w, h)
    }

    fn paint(&mut self, _ctx: &mut dyn DrawCtx) {}

    fn on_event(&mut self, _event: &agg_gui::Event) -> EventResult {
        EventResult::Ignored
    }
}
