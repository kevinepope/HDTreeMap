//! The live map: while a scan runs, the results view shows the tree scanned so far
//! (`docs/PLAN.md` §3.2, "live map"), so the map fills in as the scan goes.
//!
//! The running [`ScanSource`](crate::scan::ScanSource) owns the growing tree on the UI
//! thread and lends it out through `partial()`; `AppState::with_result` hands it to the
//! views exactly as it hands them a finished scan (with [`ScanView::live`] set), so every
//! view works unchanged. What the live map adds is a throttle: the views re-lay out and
//! re-raster when the tree version changes, and while scanning the pump bumps it only on a
//! **live tick**, at most every [`LIVE_INTERVAL_MIN`]. Each tick re-totals the extensions
//! (the legend and the palette) and bumps the tree version; the map, folder tree, legend
//! and status bar follow on the next frame.
//!
//! The interval adapts to what the refresh costs: the views report the time they spend on
//! a tick's refresh ([`AppState::add_live_work`]: extension totals, layout, raster, rows),
//! and the next tick waits until that work is at most [`LIVE_SHARE`] of the time between
//! ticks (capped at [`LIVE_INTERVAL_MAX`]), so a huge partial tree slows the refreshes
//! rather than the frames in between.
//!
//! [`ScanView::live`]: super::ScanView::live

use std::cell::{Cell, Ref, RefCell, RefMut};
use std::time::Duration;

use web_time::Instant;

use super::AppState;
use crate::model::ExtTable;
use crate::render::stats_enabled;

/// Shortest time between two live refreshes of the map.
pub const LIVE_INTERVAL_MIN: Duration = Duration::from_millis(400);
/// Longest time between two live refreshes, however slow they are.
pub const LIVE_INTERVAL_MAX: Duration = Duration::from_secs(4);
/// Most of the time between live ticks their refresh work may take.
pub const LIVE_SHARE: f64 = 0.3;

/// See the module docs.
#[derive(Default)]
pub(super) struct LiveMap {
    /// The partial tree's extension totals as of the last tick; `None` before the first.
    extensions: RefCell<Option<ExtTable>>,
    last_tick: Cell<Option<Instant>>,
    /// What the last tick's refresh cost so far (reported by the views).
    work: Cell<Duration>,
    ticks: Cell<u64>,
}

impl LiveMap {
    /// Forget the scan (a new one starts, or it finished or was cancelled).
    pub(super) fn clear(&self) {
        self.extensions.borrow_mut().take();
        self.last_tick.set(None);
        self.work.set(Duration::ZERO);
    }

    /// The extension totals of the last tick.
    pub(super) fn extensions(&self) -> Ref<'_, Option<ExtTable>> {
        self.extensions.borrow()
    }

    /// The same, to re-sort them for another size mode.
    pub(super) fn extensions_mut(&self) -> RefMut<'_, Option<ExtTable>> {
        self.extensions.borrow_mut()
    }
}

/// How long to wait after a tick whose refresh took `work`.
pub fn live_interval_for(work: Duration) -> Duration {
    let paced = work.as_secs_f64() / LIVE_SHARE;
    Duration::from_secs_f64(paced.clamp(
        LIVE_INTERVAL_MIN.as_secs_f64(),
        LIVE_INTERVAL_MAX.as_secs_f64(),
    ))
}

impl AppState {
    fn live(&self) -> &LiveMap {
        &self.inner.live
    }

    /// Whether the results view shows a scan that is still running.
    pub fn is_live(&self) -> bool {
        self.is_scanning() && self.live().extensions.borrow().is_some()
    }

    /// How many live refreshes the current (or last) scan has had.
    pub fn live_ticks(&self) -> u64 {
        self.live().ticks.get()
    }

    /// The time the next live refresh waits after the last one.
    pub fn live_interval(&self) -> Duration {
        live_interval_for(self.live().work.get())
    }

    /// Count `work` against the current live refresh (a view's share of it). No-op when
    /// no scan is running.
    pub fn add_live_work(&self, work: Duration) {
        if self.is_scanning() {
            let live = self.live();
            live.work.set(live.work.get().saturating_add(work));
        }
    }

    /// Called by the pump while a scan runs: refresh the live map if a tick is due and the
    /// source has a partial tree. Returns whether it did.
    pub(super) fn live_tick(&self, now: Instant) -> bool {
        let live = self.live();
        let due = live
            .last_tick
            .get()
            .is_none_or(|last| now.duration_since(last) >= self.live_interval());
        if !due {
            return false;
        }
        let started = Instant::now();
        let table = {
            let scan = self.inner.scan.borrow();
            let Some(source) = scan.as_ref() else {
                return false;
            };
            // Nothing to show until the tree holds more than its root.
            if source.partial().is_none_or(|p| p.tree.arena_len() < 2) {
                return false;
            }
            let Some(table) = source.partial_extensions() else {
                return false;
            };
            table
        };
        let spent = started.elapsed();
        if stats_enabled() {
            crate::render::stats_line(&format!(
                "live tick {} | extensions {:.1} ms | last refresh {:.1} ms \
                 | next in {} ms",
                live.ticks.get() + 1,
                spent.as_secs_f64() * 1000.0,
                live.work.get().as_secs_f64() * 1000.0,
                self.live_interval().as_millis()
            ));
        }
        *live.extensions.borrow_mut() = Some(table);
        live.last_tick.set(Some(now));
        live.work.set(spent);
        live.ticks.set(live.ticks.get() + 1);
        self.tree_changed();
        true
    }
}
