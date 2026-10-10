//! Application state shared by the widget tree, the shells and the tests.
//!
//! The UI runs on one thread (the browser main thread on wasm), so state lives in one
//! `Rc`: widgets hold clones of [`AppState`] and read it when they are (re)built, and tests
//! drive the same state directly. Two counters tell the widget tree what to rebuild:
//!
//! - [`AppState::version`] changes when the screen (or what it shows) changes; the root
//!   `Rebuilder` in `ui/mod.rs` watches it.
//! - [`AppState::progress_version`] ticks at most every [`PROGRESS_INTERVAL`] while a scan
//!   runs; the scan screen's live counters watch it.
//!
//! A running scan is a [`ScanSource`] the state owns. `state/pump.rs` polls it once per UI
//! frame (an agg-gui `ui_thread` drain hook) through [`AppState::pump`]. While it runs, the
//! results view already shows the tree scanned so far (the live map, `state/live.rs`):
//! [`AppState::with_result`] lends the views the partial tree as a [`ScanView`], exactly as
//! it lends them a finished scan. When the source hands back its tree the screen becomes
//! [`Screen::Results`] without rebuilding the view, so the map just refreshes in place.
//!
//! What the treemap shows of the finished scan (zoom root, hover, selection, highlighted
//! extension, shading) and the finer-grained versions its widgets watch are in
//! `state/view.rs`; the palette and the zoom root's extension table, cached for every view
//! to share, in `state/derived.rs`; the last volume list (for free space) in
//! `state/volumes.rs`; edits after a scan (Move to Trash, Rescan Folder) in
//! `state/edits.rs`; toasts, the confirmation and About sheets, context-menu requests and
//! Quit in `state/notices.rs`.

mod derived;
mod edits;
mod live;
mod notices;
mod pump;
mod view;
mod volumes;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use web_time::Instant;

pub use edits::RescanStatus;
pub use live::{live_interval_for, LIVE_INTERVAL_MAX, LIVE_INTERVAL_MIN, LIVE_SHARE};
pub use notices::{ContextRequest, Toast, ToastKind, ERROR_TOAST_TIME, TOAST_TIME};
pub use pump::{install_scan_pump, pump_scans, FRAME_BUDGET};
pub use view::{Hover, MapCounters, SIDE_SPLIT_DEFAULT, TREE_SPLIT_DEFAULT};

use crate::model::{ExtTable, SizeMode, Tree};
use crate::platform::VolumeInfo;
use crate::scan::{ProgressSnapshot, ScanErrors, ScanPoll, ScanResult, ScanSource};

/// Longest time between live-counter refreshes on the scan screen.
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// What the user chose to scan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanTarget {
    /// Folder or volume mount path to scan.
    pub path: PathBuf,
    /// The volume's display name when a volume (rather than a folder) was chosen.
    pub volume_name: Option<String>,
    /// The volume's stable id (a demo profile on the web) when a volume was chosen.
    pub volume_id: Option<String>,
    /// How many bytes the scan should find (a volume's used space), for a progress bar.
    pub expected_bytes: Option<u64>,
    /// Free space on the volume, when a volume was chosen (for the status bar).
    pub free_bytes: Option<u64>,
}

impl ScanTarget {
    pub fn folder(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            volume_name: None,
            volume_id: None,
            expected_bytes: None,
            free_bytes: None,
        }
    }

    pub fn volume(volume: &VolumeInfo) -> Self {
        Self {
            path: volume.mount_path.clone(),
            volume_name: Some(volume.name.clone()),
            volume_id: Some(volume.id.clone()),
            expected_bytes: Some(volume.used_bytes()),
            free_bytes: Some(volume.free_bytes),
        }
    }

    /// The volume's name, or the folder's path.
    pub fn display_name(&self) -> String {
        match &self.volume_name {
            Some(name) => name.clone(),
            None => self.path.to_string_lossy().into_owned(),
        }
    }
}

/// Which screen the app shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Volume list and "Choose Folder…".
    Start,
    /// A scan of the target is running; the results view shows it live.
    Scanning(ScanTarget),
    /// The target's scan finished; its result is in the state and the treemap shows it.
    Results(ScanTarget),
}

impl Screen {
    /// The scan target of the results view (live or finished).
    pub fn target(&self) -> Option<&ScanTarget> {
        match self {
            Screen::Scanning(target) | Screen::Results(target) => Some(target),
            Screen::Start => None,
        }
    }
}

/// What the views read of a scan: the finished result, or (`live`) the tree a running
/// scan has built so far (correct partial totals, children not yet sorted).
#[derive(Clone, Copy, Debug)]
pub struct ScanView<'a> {
    /// What was scanned (the tree's root name).
    pub root: &'a Path,
    pub tree: &'a Tree,
    /// Entries that could not be read (so far).
    pub errors: &'a ScanErrors,
    /// The scan is still running.
    pub live: bool,
}

/// A finished scan with what the results screen derives from it.
struct Finished {
    result: ScanResult,
    extensions: ExtTable,
}

struct Inner {
    screen: RefCell<Screen>,
    version: Cell<u64>,
    scan: RefCell<Option<Box<dyn ScanSource>>>,
    progress_version: Cell<u64>,
    last_progress: Cell<Option<Instant>>,
    finished: RefCell<Option<Finished>>,
    view: view::ViewState,
    volumes: volumes::VolumeCache,
    /// A folder rescan running in the background (`state/edits.rs`).
    rescan: RefCell<Option<edits::RescanJob>>,
    notices: notices::Notices,
    live: live::LiveMap,
}

/// Everything the UI reads and writes. Cheap to clone; clones share the same state.
#[derive(Clone)]
pub struct AppState {
    inner: Rc<Inner>,
}

impl AppState {
    /// Start on the start screen.
    pub fn new() -> Self {
        Self {
            inner: Rc::new(Inner {
                screen: RefCell::new(Screen::Start),
                version: Cell::new(0),
                scan: RefCell::new(None),
                progress_version: Cell::new(0),
                last_progress: Cell::new(None),
                finished: RefCell::new(None),
                view: view::ViewState::new(),
                volumes: volumes::VolumeCache::default(),
                rescan: RefCell::new(None),
                notices: notices::Notices::default(),
                live: live::LiveMap::default(),
            }),
        }
    }

    pub fn screen(&self) -> Screen {
        self.inner.screen.borrow().clone()
    }

    /// Bumped on every screen change; cheap to poll each layout.
    pub fn version(&self) -> u64 {
        self.inner.version.get()
    }

    /// Ticks while a scan runs (at most every [`PROGRESS_INTERVAL`]).
    pub fn progress_version(&self) -> u64 {
        self.inner.progress_version.get()
    }

    /// Show the results view for `target`, live, and run `source` until it finishes.
    /// Replaces (and cancels) any scan already running.
    pub fn start_scan(&self, target: ScanTarget, mut source: Box<dyn ScanSource>) {
        self.drop_scan();
        self.cancel_rescan();
        self.inner.finished.borrow_mut().take();
        source.set_size_mode(self.size_mode());
        *self.inner.scan.borrow_mut() = Some(source);
        self.inner.last_progress.set(None);
        self.reset_view();
        pump::register(&self.inner);
        self.set_screen(Screen::Scanning(target));
    }

    /// Stop the running scan and return to the start screen.
    pub fn cancel_scan(&self) {
        self.back_to_start();
    }

    /// Return to the start screen, cancelling any scan and dropping any result.
    pub fn back_to_start(&self) {
        self.drop_scan();
        self.cancel_rescan();
        self.inner.finished.borrow_mut().take();
        self.set_screen(Screen::Start);
    }

    /// Whether a scan is running.
    pub fn is_scanning(&self) -> bool {
        self.inner.scan.borrow().is_some()
    }

    /// The running scan's counters, current path and elapsed time.
    pub fn progress(&self) -> Option<ProgressSnapshot> {
        let scan = self.inner.scan.borrow();
        scan.as_ref().map(|s| s.progress().snapshot())
    }

    /// Advance the running scan by up to `budget` of this thread's time. When it finishes,
    /// keep its result and switch to the results screen. Returns whether a scan is still
    /// running. Called once per frame by the scan pump.
    pub fn pump(&self, budget: Duration) -> bool {
        let rescanning = self.pump_rescan(budget);
        self.pump_scan(budget) || rescanning
    }

    /// [`AppState::pump`] for the full scan.
    fn pump_scan(&self, budget: Duration) -> bool {
        let poll = {
            let mut scan = self.inner.scan.borrow_mut();
            let Some(source) = scan.as_mut() else {
                return false;
            };
            source.poll(budget)
        };
        match poll {
            ScanPoll::Running => {
                let now = Instant::now();
                let due = self
                    .inner
                    .last_progress
                    .get()
                    .is_none_or(|last| now.duration_since(last) >= PROGRESS_INTERVAL);
                if due {
                    self.inner.last_progress.set(Some(now));
                    self.bump_progress();
                }
                self.live_tick(now);
                true
            }
            ScanPoll::Finished(result) => {
                self.inner.scan.borrow_mut().take();
                self.inner.live.clear();
                let mut result = *result;
                // A source without a partial tree sorts by its own default.
                result.tree.set_size_mode(self.size_mode());
                let extensions = ExtTable::from_tree(&result.tree);
                *self.inner.finished.borrow_mut() = Some(Finished { result, extensions });
                self.finish_view();
                let target = match self.screen() {
                    Screen::Scanning(target) | Screen::Results(target) => target,
                    Screen::Start => ScanTarget::folder(PathBuf::new()),
                };
                // The live view becomes the results view in place: no rebuild, no flash.
                *self.inner.screen.borrow_mut() = Screen::Results(target);
                self.bump_progress();
                agg_gui::animation::request_draw();
                false
            }
        }
    }

    /// Read the scan the results view shows, with its extension table: the finished
    /// result, or while a scan runs the tree so far (once its first live tick has totalled
    /// the extensions). `None` on the start screen.
    pub fn with_result<R>(&self, f: impl FnOnce(&ScanView<'_>, &ExtTable) -> R) -> Option<R> {
        if let Some(d) = self.inner.finished.borrow().as_ref() {
            let view = ScanView {
                root: &d.result.root,
                tree: &d.result.tree,
                errors: &d.result.errors,
                live: false,
            };
            return Some(f(&view, &d.extensions));
        }
        let extensions = self.inner.live.extensions();
        let extensions = extensions.as_ref()?;
        let scan = self.inner.scan.borrow();
        let partial = scan.as_ref()?.partial()?;
        let view = ScanView {
            root: partial.root,
            tree: partial.tree,
            errors: partial.errors,
            live: true,
        };
        Some(f(&view, extensions))
    }

    /// The size mode the map, tree and legend use (kept across scans).
    pub fn size_mode(&self) -> SizeMode {
        self.view_size_mode()
    }

    /// Use `mode`: re-sort the finished tree (and its extension table), or tell the running
    /// scan. The map re-lays out (through [`AppState::tree_version`]); the screen itself is
    /// not rebuilt.
    pub fn set_size_mode(&self, mode: SizeMode) {
        if self.size_mode() == mode {
            return;
        }
        self.set_view_size_mode(mode);
        if let Some(d) = self.inner.finished.borrow_mut().as_mut() {
            d.result.tree.set_size_mode(mode);
            d.extensions.sort(mode);
        }
        if let Some(source) = self.inner.scan.borrow_mut().as_mut() {
            source.set_size_mode(mode);
        }
        if let Some(table) = self.inner.live.extensions_mut().as_mut() {
            table.sort(mode);
        }
        self.tree_changed();
    }

    fn drop_scan(&self) {
        let scan = self.inner.scan.borrow_mut().take();
        if let Some(source) = scan {
            source.cancel();
        }
        self.inner.live.clear();
    }

    /// Tick [`AppState::progress_version`] (the live counters refresh).
    pub(super) fn bump_progress(&self) {
        let v = self.inner.progress_version.get();
        self.inner.progress_version.set(v.wrapping_add(1));
        agg_gui::animation::request_draw();
    }

    fn set_screen(&self, screen: Screen) {
        *self.inner.screen.borrow_mut() = screen;
        self.bump();
    }

    fn bump(&self) {
        self.inner
            .version
            .set(self.inner.version.get().wrapping_add(1));
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_edits;
