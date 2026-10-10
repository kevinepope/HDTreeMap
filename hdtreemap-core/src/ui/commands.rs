//! The app's commands: everything the menu bar and the keyboard shortcuts can do, in one
//! place, so the in-window `MenuBar` (Windows, Linux, the web), the native macOS menu
//! (built by `hdtreemap-native` from the same commands) and the shortcut handler all run,
//! enable and check exactly the same things.
//!
//! A [`Command`] has a stable string id (the menu action), and [`Commands`] answers whether
//! it is enabled and checked right now and runs it. The Edit menu's commands act on the
//! selection through [`NodeActions`](super::node_actions::NodeActions).

use std::rc::Rc;

use super::node_actions::{NodeAction, NodeActions};
use super::start_screen::start_scan;
use crate::model::{NodeId, SizeMode};
use crate::platform::{ActionOutcome, PlatformServices};
use crate::render::{ColorBy, Shading};
use crate::state::{AppState, ScanTarget, Screen};

/// The project's home page (Help ▸ View on GitHub, the About sheet).
pub const GITHUB_URL: &str = "https://github.com/larsbrubaker/HDTreeMap";

/// One menu or shortcut command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// File ▸ Open Folder… (the folder picker; native only).
    OpenFolder,
    /// File ▸ Scan Drive ▸ the `n`-th volume of the cached volume list.
    ScanVolume(usize),
    /// File ▸ Rescan: scan the shown target again.
    Rescan,
    /// File ▸ Back to Drives.
    BackToDrives,
    /// File ▸ Quit (native only).
    Quit,
    /// Edit ▸ Copy Path of the selection.
    CopyPath,
    /// Edit ▸ Move to Trash… the selection.
    Trash,
    /// View ▸ Size Mode.
    SizeMode(SizeMode),
    /// View ▸ Cushion Shading.
    ToggleCushions,
    /// View ▸ Colour By.
    ColorBy(ColorBy),
    /// View ▸ Zoom In: to the selected folder (or the selected file's folder).
    ZoomIn,
    /// View ▸ Zoom Out one level.
    ZoomOut,
    /// View ▸ Zoom to Root.
    ZoomRoot,
    /// View ▸ Clear Highlight.
    ClearHighlight,
    /// Help ▸ About HDTreeMap.
    About,
    /// Help ▸ View on GitHub.
    ViewOnGitHub,
}

impl Command {
    /// The menu action id.
    pub fn id(self) -> String {
        match self {
            Command::OpenFolder => "file.open".into(),
            Command::ScanVolume(n) => format!("file.scan.{n}"),
            Command::Rescan => "file.rescan".into(),
            Command::BackToDrives => "file.back".into(),
            Command::Quit => "file.quit".into(),
            Command::CopyPath => "edit.copy-path".into(),
            Command::Trash => "edit.trash".into(),
            Command::SizeMode(SizeMode::Allocated) => "view.size.allocated".into(),
            Command::SizeMode(SizeMode::Apparent) => "view.size.apparent".into(),
            Command::ToggleCushions => "view.cushions".into(),
            Command::ColorBy(ColorBy::Extension) => "view.color.type".into(),
            Command::ColorBy(ColorBy::TopFolder) => "view.color.folder".into(),
            Command::ZoomIn => "view.zoom-in".into(),
            Command::ZoomOut => "view.zoom-out".into(),
            Command::ZoomRoot => "view.zoom-root".into(),
            Command::ClearHighlight => "view.clear-highlight".into(),
            Command::About => "help.about".into(),
            Command::ViewOnGitHub => "help.github".into(),
        }
    }

    /// The command with menu action id `id`.
    pub fn from_id(id: &str) -> Option<Self> {
        if let Some(n) = id.strip_prefix("file.scan.") {
            return n.parse().ok().map(Command::ScanVolume);
        }
        [
            Command::OpenFolder,
            Command::Rescan,
            Command::BackToDrives,
            Command::Quit,
            Command::CopyPath,
            Command::Trash,
            Command::SizeMode(SizeMode::Allocated),
            Command::SizeMode(SizeMode::Apparent),
            Command::ToggleCushions,
            Command::ColorBy(ColorBy::Extension),
            Command::ColorBy(ColorBy::TopFolder),
            Command::ZoomIn,
            Command::ZoomOut,
            Command::ZoomRoot,
            Command::ClearHighlight,
            Command::About,
            Command::ViewOnGitHub,
        ]
        .into_iter()
        .find(|c| c.id() == id)
    }
}

/// Runs [`Command`]s. Cheap to clone.
#[derive(Clone)]
pub struct Commands {
    state: AppState,
    platform: Rc<dyn PlatformServices>,
    nodes: NodeActions,
}

impl Commands {
    pub fn new(state: &AppState, platform: &Rc<dyn PlatformServices>) -> Self {
        Self {
            state: state.clone(),
            platform: Rc::clone(platform),
            nodes: NodeActions::new(state, platform),
        }
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn platform(&self) -> &Rc<dyn PlatformServices> {
        &self.platform
    }

    /// The node actions (context menus, the trash confirmation).
    pub fn nodes(&self) -> &NodeActions {
        &self.nodes
    }

    /// Whether the results view is up (a finished scan, or a running one shown live).
    fn has_map(&self) -> bool {
        self.state.screen().target().is_some()
    }

    /// Whether a finished scan is shown (Rescan waits for the running one).
    fn on_results(&self) -> bool {
        matches!(self.state.screen(), Screen::Results(_)) && !self.state.is_scanning()
    }

    /// The selection, if it is a live node of the shown result.
    fn selection(&self) -> Option<NodeId> {
        if !self.has_map() {
            return None;
        }
        let node = self.state.selected()?;
        self.state
            .with_result(|r, _| r.tree.is_live(node))
            .unwrap_or(false)
            .then_some(node)
    }

    /// The folder Zoom In goes to: the selected folder, or the selected file's folder, when
    /// that is not the zoom root already.
    fn zoom_in_target(&self) -> Option<NodeId> {
        let node = self.selection()?;
        let folder = self
            .state
            .with_result(|r, _| {
                let n = r.tree.node(node)?;
                Some(if n.is_dir() { node } else { n.parent() })
            })
            .flatten()?;
        (folder != self.state.zoom_root() && !folder.is_none()).then_some(folder)
    }

    fn zoomed_in(&self) -> bool {
        self.has_map() && self.state.zoom_root() != NodeId(0)
    }

    /// Whether `command` can run now (menus grey it out when not).
    pub fn enabled(&self, command: Command) -> bool {
        match command {
            Command::OpenFolder | Command::Quit => !self.platform.is_demo(),
            Command::ScanVolume(n) => self.state.volumes().is_some_and(|v| n < v.len()),
            Command::Rescan => self.on_results(),
            Command::BackToDrives => self.state.screen() != Screen::Start,
            Command::CopyPath => self
                .selection()
                .is_some_and(|n| self.nodes.enabled(n, NodeAction::CopyPath)),
            Command::Trash => self
                .selection()
                .is_some_and(|n| self.nodes.enabled(n, NodeAction::Trash)),
            Command::SizeMode(_) | Command::ToggleCushions | Command::ColorBy(_) => self.has_map(),
            Command::ZoomIn => self.zoom_in_target().is_some(),
            Command::ZoomOut | Command::ZoomRoot => self.zoomed_in(),
            Command::ClearHighlight => self.has_map() && self.state.highlight_ext().is_some(),
            Command::About | Command::ViewOnGitHub => true,
        }
    }

    /// The check or radio mark of `command`: `Some(on)` for the size modes, the colour
    /// schemes and cushion shading, `None` for plain commands.
    pub fn checked(&self, command: Command) -> Option<bool> {
        match command {
            Command::SizeMode(mode) => Some(self.state.size_mode() == mode),
            Command::ToggleCushions => Some(self.state.shading() == Shading::Cushion),
            Command::ColorBy(by) => Some(self.state.color_by() == by),
            _ => None,
        }
    }

    /// Run `command` if it is enabled.
    pub fn run(&self, command: Command) {
        if !self.enabled(command) {
            return;
        }
        let state = &self.state;
        match command {
            Command::OpenFolder => {
                if let Some(path) = self.platform.pick_folder() {
                    start_scan(state, &self.platform, ScanTarget::folder(path));
                }
            }
            Command::ScanVolume(n) => {
                if let Some(volume) = state.volumes().and_then(|v| v.get(n).cloned()) {
                    start_scan(state, &self.platform, ScanTarget::volume(&volume));
                }
            }
            Command::Rescan => {
                if let Screen::Results(target) = state.screen() {
                    start_scan(state, &self.platform, target);
                }
            }
            Command::BackToDrives => state.back_to_start(),
            Command::Quit => state.request_quit(),
            Command::CopyPath => self.on_selection(NodeAction::CopyPath),
            Command::Trash => self.on_selection(NodeAction::Trash),
            Command::SizeMode(mode) => state.set_size_mode(mode),
            Command::ColorBy(by) => state.set_color_by(by),
            Command::ToggleCushions => state.set_shading(match state.shading() {
                Shading::Cushion => Shading::Flat,
                Shading::Flat => Shading::Cushion,
            }),
            Command::ZoomIn => {
                if let Some(folder) = self.zoom_in_target() {
                    state.set_zoom_root(folder);
                }
            }
            Command::ZoomOut => {
                state.zoom_out();
            }
            Command::ZoomRoot => {
                state.set_zoom_root(NodeId(0));
            }
            Command::ClearHighlight => state.set_highlight_ext(None),
            Command::About => state.show_about(),
            Command::ViewOnGitHub => match self.platform.open_url(GITHUB_URL) {
                Ok(ActionOutcome::Done) => {}
                Ok(ActionOutcome::Simulated(note)) => state.notify(note),
                Err(err) => state.notify_error(err.message),
            },
        }
    }

    fn on_selection(&self, action: NodeAction) {
        if let Some(node) = self.selection() {
            self.nodes.run(node, action);
        }
    }
}
