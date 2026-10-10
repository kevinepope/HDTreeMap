//! What can be done to one file or folder (`docs/PLAN.md` §2, Interaction): the items of
//! the right-click menu on the map and the folder tree, and the Edit menu's actions on the
//! selection. Each action goes through `PlatformServices`, so the web demo simulates the
//! ones that would touch the computer and the tests record them.
//!
//! - **Reveal** in Finder / Explorer / the file manager, and **Open**, refuse items whose
//!   name could not be stored exactly (`Tree::exact_path`): the stored path is not the real
//!   one, so acting on it could hit the wrong file.
//! - **Copy Path** puts the path on the clipboard.
//! - **Zoom Here** (folders) and **Zoom to Parent** move the map.
//! - **Rescan Folder** rescans one folder in the background (`AppState::start_rescan`).
//! - **Move to Trash…** asks first ([`ui::sheets`](super::sheets)); confirming calls
//!   `PlatformServices::trash` and then removes the item from the tree. The scan root and
//!   inexact names are refused.
//!
//! Results are reported in toasts (`AppState::notify`).

use std::path::PathBuf;
use std::rc::Rc;

use crate::fonts::fa;
use crate::format::format_bytes;
use crate::model::{NodeFlags, NodeId};
use crate::platform::{ActionOutcome, ActionResult, PlatformServices};
use crate::state::{AppState, ScanTarget, Screen};

/// What the app says when it refuses to act on an inexact name.
pub const LOSSY_NAME_MESSAGE: &str =
    "This name can't be represented exactly; use your file manager.";
/// What the app says when asked to trash the scanned folder itself.
pub const TRASH_ROOT_MESSAGE: &str =
    "The scanned folder itself can't be moved to the Trash from here.";
/// The toast after a simulated (web demo) Move to Trash.
pub const SIMULATED_TRASH_MESSAGE: &str = "Simulated: nothing was deleted";
/// The toast after Copy Path.
pub const COPIED_MESSAGE: &str = "Copied path";
/// What the web demo says when a folder is rescanned.
pub const DEMO_RESCAN_MESSAGE: &str =
    "Simulated drive: the demo regenerates the whole drive to rescan a folder.";

/// One thing to do to a file or folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeAction {
    Reveal,
    Open,
    CopyPath,
    ZoomHere,
    ZoomToParent,
    Rescan,
    Trash,
}

impl NodeAction {
    /// Every action, in menu order.
    pub const ALL: [NodeAction; 7] = [
        NodeAction::Reveal,
        NodeAction::Open,
        NodeAction::CopyPath,
        NodeAction::ZoomHere,
        NodeAction::ZoomToParent,
        NodeAction::Rescan,
        NodeAction::Trash,
    ];

    /// The menu action id.
    pub fn id(self) -> &'static str {
        match self {
            NodeAction::Reveal => "node.reveal",
            NodeAction::Open => "node.open",
            NodeAction::CopyPath => "node.copy-path",
            NodeAction::ZoomHere => "node.zoom-here",
            NodeAction::ZoomToParent => "node.zoom-parent",
            NodeAction::Rescan => "node.rescan",
            NodeAction::Trash => "node.trash",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.id() == id)
    }

    /// The Font Awesome icon.
    pub fn icon(self) -> char {
        match self {
            NodeAction::Reveal => fa::FOLDER_OPEN,
            NodeAction::Open => fa::EXTERNAL_LINK,
            NodeAction::CopyPath => fa::CLIPBOARD,
            NodeAction::ZoomHere => fa::SEARCH_PLUS,
            NodeAction::ZoomToParent => fa::LEVEL_UP,
            NodeAction::Rescan => fa::REFRESH,
            NodeAction::Trash => fa::TRASH,
        }
    }
}

/// The label of Reveal: the platform's file manager by name, or a demo note.
pub fn reveal_label(platform: &dyn PlatformServices) -> &'static str {
    if platform.is_demo() {
        "Reveal (demo)"
    } else if cfg!(target_os = "macos") {
        "Reveal in Finder"
    } else if cfg!(windows) {
        "Show in Explorer"
    } else {
        "Show in File Manager"
    }
}

/// The label of `action` in menus.
pub fn label(action: NodeAction, platform: &dyn PlatformServices) -> &'static str {
    match action {
        NodeAction::Reveal => reveal_label(platform),
        NodeAction::Open => "Open",
        NodeAction::CopyPath => "Copy Path",
        NodeAction::ZoomHere => "Zoom Here",
        NodeAction::ZoomToParent => "Zoom to Parent",
        NodeAction::Rescan => "Rescan Folder",
        NodeAction::Trash => "Move to Trash\u{2026}",
    }
}

/// Runs node actions against the app state and the platform. Cheap to clone.
#[derive(Clone)]
pub struct NodeActions {
    state: AppState,
    platform: Rc<dyn PlatformServices>,
}

/// What the tree says about a node, for deciding what applies.
struct Facts {
    live: bool,
    is_dir: bool,
    /// The scan root (never trashed).
    is_root: bool,
    skipped: bool,
    has_parent: bool,
    is_zoom_root: bool,
}

impl NodeActions {
    pub fn new(state: &AppState, platform: &Rc<dyn PlatformServices>) -> Self {
        Self {
            state: state.clone(),
            platform: Rc::clone(platform),
        }
    }

    pub fn platform(&self) -> &dyn PlatformServices {
        self.platform.as_ref()
    }

    fn facts(&self, node: NodeId) -> Option<Facts> {
        self.state
            .with_result(|r, _| {
                let tree = &r.tree;
                let n = tree.node(node)?;
                Some(Facts {
                    live: tree.is_live(node),
                    is_dir: n.is_dir(),
                    is_root: node == tree.root(),
                    skipped: n.flags().contains(NodeFlags::SKIPPED),
                    has_parent: !n.parent().is_none(),
                    is_zoom_root: node == self.state.zoom_root(),
                })
            })
            .flatten()
    }

    /// Whether `action` applies to `node` (greyed out in menus when not). Refusals that
    /// need explaining (an inexact name, the scan root for Move to Trash) stay enabled
    /// and say why when chosen.
    pub fn enabled(&self, node: NodeId, action: NodeAction) -> bool {
        let Some(f) = self.facts(node).filter(|f| f.live) else {
            return false;
        };
        match action {
            NodeAction::Reveal | NodeAction::Open | NodeAction::CopyPath => true,
            NodeAction::ZoomHere => f.is_dir && !f.is_zoom_root,
            NodeAction::ZoomToParent => f.has_parent,
            NodeAction::Rescan => {
                f.is_dir
                    && !f.skipped
                    && !self.state.is_scanning()
                    && self.results_target().is_some()
            }
            // The scan root stays enabled so choosing it explains the refusal.
            NodeAction::Trash => true,
        }
    }

    /// Do `action` to `node`.
    pub fn run(&self, node: NodeId, action: NodeAction) {
        if !self.enabled(node, action) {
            return;
        }
        match action {
            NodeAction::Reveal => {
                if let Some(path) = self.exact_path(node) {
                    self.report(self.platform.reveal(&path), None);
                }
            }
            NodeAction::Open => {
                if let Some(path) = self.exact_path(node) {
                    self.report(self.platform.open(&path), None);
                }
            }
            NodeAction::CopyPath => {
                let path = self.state.with_result(|r, _| r.tree.path(node));
                if let Some(path) = path {
                    agg_gui::clipboard::set_text(&path.to_string_lossy());
                    self.state.notify(COPIED_MESSAGE);
                }
            }
            NodeAction::ZoomHere => {
                self.state.set_zoom_root(node);
            }
            NodeAction::ZoomToParent => {
                let parent = self
                    .state
                    .with_result(|r, _| r.tree.node(node).map(|n| n.parent()))
                    .flatten();
                if let Some(parent) = parent {
                    self.state.set_zoom_root(parent);
                }
            }
            NodeAction::Rescan => self.rescan(node),
            NodeAction::Trash => self.ask_to_trash(node),
        }
    }

    /// The node's path for the OS, or `None` after telling the user it is inexact.
    fn exact_path(&self, node: NodeId) -> Option<PathBuf> {
        let path = self
            .state
            .with_result(|r, _| r.tree.exact_path(node))
            .flatten();
        if path.is_none() {
            self.state.notify_error(LOSSY_NAME_MESSAGE);
        }
        path
    }

    /// Toast a platform action's outcome: `done` (if any) when it worked, the demo's note
    /// when simulated, the error when it failed.
    fn report(&self, result: ActionResult, done: Option<String>) {
        match result {
            Ok(ActionOutcome::Done) => {
                if let Some(done) = done {
                    self.state.notify(done);
                }
            }
            Ok(ActionOutcome::Simulated(note)) => self.state.notify(note),
            Err(err) => self.state.notify_error(err.message),
        }
    }

    /// The target of the results screen (what was scanned), if it is showing.
    fn results_target(&self) -> Option<ScanTarget> {
        match self.state.screen() {
            Screen::Results(target) => Some(target),
            _ => None,
        }
    }

    fn rescan(&self, node: NodeId) {
        let Some(path) = self.exact_path(node) else {
            return;
        };
        let Some(target) = self.results_target() else {
            return;
        };
        let folder = ScanTarget {
            path,
            volume_name: target.volume_name,
            volume_id: target.volume_id,
            expected_bytes: None,
            free_bytes: target.free_bytes,
        };
        let source = self.platform.scan_source(&folder);
        if self.state.start_rescan(node, source) && self.platform.is_demo() {
            self.state.notify(DEMO_RESCAN_MESSAGE);
        }
    }

    /// Open the Move to Trash confirmation for `node`, or say why not.
    fn ask_to_trash(&self, node: NodeId) {
        let Some(f) = self.facts(node) else {
            return;
        };
        if f.is_root {
            self.state.notify_error(TRASH_ROOT_MESSAGE);
            return;
        }
        if self.exact_path(node).is_none() {
            return;
        }
        self.state.request_trash(node);
    }

    /// The confirmation's "Move to Trash" button: trash the waiting node, then take it out
    /// of the tree. A failure is reported and the tree is left as it was.
    pub fn confirm_trash(&self) {
        let Some(node) = self.state.trash_request() else {
            return;
        };
        self.state.cancel_trash();
        let Some(f) = self.facts(node).filter(|f| f.live) else {
            return;
        };
        if f.is_root {
            self.state.notify_error(TRASH_ROOT_MESSAGE);
            return;
        }
        let Some(path) = self.exact_path(node) else {
            return;
        };
        let name = self
            .state
            .with_result(|r, _| r.tree.name(node).to_string())
            .unwrap_or_default();
        match self.platform.trash(&path) {
            Ok(outcome) => {
                let removed = self.state.remove_node(node);
                let size = removed.map(|s| s.get(self.state.size_mode())).unwrap_or(0);
                match outcome {
                    ActionOutcome::Done => self.state.notify(format!(
                        "Moved {name} to the Trash ({})",
                        format_bytes(size)
                    )),
                    ActionOutcome::Simulated(_) => self.state.notify(SIMULATED_TRASH_MESSAGE),
                }
            }
            Err(err) => self.state.notify_error(err.message),
        }
    }
}
