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
//! frame (an agg-gui `ui_thread` drain hook) through [`AppState::pump`], which moves the app
//! to the results (treemap) screen when the source hands back its tree.
//!
//! What the treemap shows of the finished scan (zoom root, hover, selection, highlighted
//! extension, shading) and the finer-grained versions its widgets watch are in
//! `state/view.rs`; the palette and the zoom root's extension table, cached for every view
//! to share, in `state/derived.rs`; the last volume list (for free space) in
//! `state/volumes.rs`.

mod derived;
mod pump;
mod view;
mod volumes;

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use web_time::Instant;

pub use pump::{install_scan_pump, pump_scans, FRAME_BUDGET};
pub use view::{Hover, MapCounters, SIDE_SPLIT_DEFAULT, TREE_SPLIT_DEFAULT};

use crate::model::{ExtTable, SizeMode};
use crate::platform::VolumeInfo;
use crate::scan::{ProgressSnapshot, ScanPoll, ScanResult, ScanSource};

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
    /// A scan of the target is running.
    Scanning(ScanTarget),
    /// The target's scan finished; its result is in the state and the treemap shows it.
    Results(ScanTarget),
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

    /// Show the scan screen for `target` and run `source` until it finishes. Replaces (and
    /// cancels) any scan already running.
    pub fn start_scan(&self, target: ScanTarget, source: Box<dyn ScanSource>) {
        self.drop_scan();
        self.inner.finished.borrow_mut().take();
        *self.inner.scan.borrow_mut() = Some(source);
        self.inner.last_progress.set(None);
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
                    let v = self.inner.progress_version.get();
                    self.inner.progress_version.set(v.wrapping_add(1));
                    agg_gui::animation::request_draw();
                }
                true
            }
            ScanPoll::Finished(result) => {
                self.inner.scan.borrow_mut().take();
                let extensions = ExtTable::from_tree(&result.tree);
                *self.inner.finished.borrow_mut() = Some(Finished {
                    result: *result,
                    extensions,
                });
                self.reset_view();
                let target = match self.screen() {
                    Screen::Scanning(target) | Screen::Results(target) => target,
                    Screen::Start => ScanTarget::folder(PathBuf::new()),
                };
                self.set_screen(Screen::Results(target));
                agg_gui::animation::request_draw();
                false
            }
        }
    }

    /// Read the finished scan and its extension table.
    pub fn with_result<R>(&self, f: impl FnOnce(&ScanResult, &ExtTable) -> R) -> Option<R> {
        let finished = self.inner.finished.borrow();
        finished.as_ref().map(|d| f(&d.result, &d.extensions))
    }

    /// The finished tree's size mode ([`SizeMode::Allocated`] with no result).
    pub fn size_mode(&self) -> SizeMode {
        self.with_result(|r, _| r.tree.size_mode())
            .unwrap_or_default()
    }

    /// Re-sort the finished tree (and its extension table) by `mode`. The map re-lays out
    /// (through [`AppState::tree_version`]); the screen itself is not rebuilt.
    pub fn set_size_mode(&self, mode: SizeMode) {
        let changed = {
            let mut finished = self.inner.finished.borrow_mut();
            match finished.as_mut() {
                Some(d) if d.result.tree.size_mode() != mode => {
                    d.result.tree.set_size_mode(mode);
                    d.extensions.sort(mode);
                    true
                }
                _ => false,
            }
        };
        if changed {
            self.tree_changed();
        }
    }

    fn drop_scan(&self) {
        let scan = self.inner.scan.borrow_mut().take();
        if let Some(source) = scan {
            source.cancel();
        }
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
