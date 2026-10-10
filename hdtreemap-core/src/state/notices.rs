//! Short-lived UI state the actions (`ui/commands.rs`) share with the overlay widgets that
//! show them, kept in `AppState` so the treemap, the folder tree, the menus and the tests
//! all reach the same one:
//!
//! - **toasts**: transient messages ("Copied path", "Moved X to the Trash", errors), shown
//!   bottom-right by `ui/toast.rs` and dropped after [`TOAST_TIME`] ([`ERROR_TOAST_TIME`]
//!   for errors);
//! - the **Move to Trash confirmation**: which node is waiting, and the visibility cell of
//!   its `ModalSheet` (`ui/sheets.rs`);
//! - the **About** sheet's visibility;
//! - a pending **context menu** request (where, for which node), taken by the context
//!   menu host (`ui/context_menu.rs`) on its next layout;
//! - a **quit** request, which the native shell turns into closing the window;
//! - whether the shell installed a **native menu bar** (macOS), in which case the window
//!   shows no in-window one.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use agg_gui::Point;
use web_time::Instant;

use super::AppState;
use crate::model::NodeId;

/// How long an ordinary toast stays up.
pub const TOAST_TIME: Duration = Duration::from_secs(4);
/// How long an error toast stays up.
pub const ERROR_TOAST_TIME: Duration = Duration::from_secs(8);
/// Most toasts shown at once (the oldest go first).
const MAX_TOASTS: usize = 4;

/// What a toast reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Error,
}

/// One transient message.
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub shown: Instant,
}

impl Toast {
    /// How long it stays up.
    pub fn lifetime(&self) -> Duration {
        match self.kind {
            ToastKind::Info => TOAST_TIME,
            ToastKind::Error => ERROR_TOAST_TIME,
        }
    }
}

/// A right click asked for a context menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContextRequest {
    /// Where to open it, in root (window) coordinates, Y up, logical points.
    pub at: Point,
    /// The item it acts on.
    pub node: NodeId,
}

/// See the module docs.
pub(super) struct Notices {
    toasts: RefCell<Vec<Toast>>,
    trash_request: Cell<Option<NodeId>>,
    trash_sheet: Rc<Cell<bool>>,
    about_sheet: Rc<Cell<bool>>,
    context: Cell<Option<ContextRequest>>,
    quit: Cell<bool>,
    native_menu: Cell<bool>,
    /// Bumped when the trash request changes (the sheet's content rebuilds on it).
    sheet_version: Cell<u64>,
}

impl Default for Notices {
    fn default() -> Self {
        Self {
            toasts: RefCell::new(Vec::new()),
            trash_request: Cell::new(None),
            trash_sheet: Rc::new(Cell::new(false)),
            about_sheet: Rc::new(Cell::new(false)),
            context: Cell::new(None),
            quit: Cell::new(false),
            native_menu: Cell::new(false),
            sheet_version: Cell::new(0),
        }
    }
}

impl AppState {
    fn notices(&self) -> &Notices {
        &self.inner.notices
    }

    /// Show `text` as a toast.
    pub fn notify(&self, text: impl Into<String>) {
        self.push_toast(text.into(), ToastKind::Info);
    }

    /// Show `text` as an error toast (it stays up longer).
    pub fn notify_error(&self, text: impl Into<String>) {
        self.push_toast(text.into(), ToastKind::Error);
    }

    fn push_toast(&self, text: String, kind: ToastKind) {
        let mut toasts = self.notices().toasts.borrow_mut();
        toasts.push(Toast {
            text,
            kind,
            shown: Instant::now(),
        });
        let excess = toasts.len().saturating_sub(MAX_TOASTS);
        toasts.drain(..excess);
        agg_gui::animation::request_draw();
    }

    /// The toasts still up, oldest first (expired ones are dropped).
    pub fn toasts(&self) -> Vec<Toast> {
        let mut toasts = self.notices().toasts.borrow_mut();
        toasts.retain(|t| t.shown.elapsed() < t.lifetime());
        toasts.clone()
    }

    /// Ask to confirm moving `node` to the trash: the confirmation sheet opens for it.
    pub fn request_trash(&self, node: NodeId) {
        let notices = self.notices();
        notices.trash_request.set(Some(node));
        notices
            .sheet_version
            .set(notices.sheet_version.get().wrapping_add(1));
        notices.trash_sheet.set(true);
        agg_gui::animation::request_draw();
    }

    /// The node waiting for the trash confirmation.
    pub fn trash_request(&self) -> Option<NodeId> {
        self.notices().trash_request.get()
    }

    /// Close the trash confirmation without doing anything.
    pub fn cancel_trash(&self) {
        let notices = self.notices();
        notices.trash_request.set(None);
        notices.trash_sheet.set(false);
        agg_gui::animation::request_draw();
    }

    /// The trash confirmation sheet's visibility cell.
    pub fn trash_sheet(&self) -> Rc<Cell<bool>> {
        Rc::clone(&self.notices().trash_sheet)
    }

    /// Changes whenever a new trash confirmation is requested.
    pub fn sheet_version(&self) -> u64 {
        self.notices().sheet_version.get()
    }

    /// The About sheet's visibility cell.
    pub fn about_sheet(&self) -> Rc<Cell<bool>> {
        Rc::clone(&self.notices().about_sheet)
    }

    /// Open the About sheet.
    pub fn show_about(&self) {
        self.notices().about_sheet.set(true);
        agg_gui::animation::request_draw();
    }

    /// Ask for the context menu of `node` at `at` (root coordinates, Y up).
    pub fn request_context_menu(&self, at: Point, node: NodeId) {
        self.notices()
            .context
            .set(Some(ContextRequest { at, node }));
        agg_gui::animation::request_draw();
    }

    /// Take the pending context-menu request, if any.
    pub fn take_context_request(&self) -> Option<ContextRequest> {
        self.notices().context.take()
    }

    /// Whether a context-menu request is waiting.
    pub fn has_context_request(&self) -> bool {
        self.notices().context.get().is_some()
    }

    /// Ask the shell to close the app (File ▸ Quit).
    pub fn request_quit(&self) {
        self.notices().quit.set(true);
        agg_gui::animation::request_draw();
    }

    /// Whether Quit was chosen.
    pub fn quit_requested(&self) -> bool {
        self.notices().quit.get()
    }

    /// The shell installed a native menu bar (macOS): the window shows no in-window one.
    pub fn set_native_menu_bar(&self, native: bool) {
        self.notices().native_menu.set(native);
    }

    pub fn has_native_menu_bar(&self) -> bool {
        self.notices().native_menu.get()
    }
}
