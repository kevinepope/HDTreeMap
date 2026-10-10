//! The scan pump: polls every running scan once per UI frame.
//!
//! agg-gui drains its `ui_thread` queue every loop iteration and before each painted frame,
//! then runs the registered drain hooks; [`pump_scans`] is HDTreeMap's hook (installed by
//! `build_app` through [`install_scan_pump`]). It gives each [`AppState`] with a running
//! scan up to [`FRAME_BUDGET`] to advance it. The demo scanner does its generating in that
//! slice (single-threaded wasm has no worker); the filesystem scanner only checks for its
//! result. While a scan runs, the pump arms a redraw [`PUMP_INTERVAL`] ahead, so a reactive
//! frame loop keeps coming back. The worker-thread scanner also wakes the loop itself.
//!
//! Hooks are process-wide but the registry is per thread, like agg-gui's UI state, so
//! headless tests running in parallel never pump each other's scans.

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::time::Duration;

use super::{AppState, Inner};

/// UI-thread time a scan may use per frame.
pub const FRAME_BUDGET: Duration = Duration::from_millis(8);

/// How soon the pump runs again while a scan is in progress.
const PUMP_INTERVAL: Duration = Duration::from_millis(30);

thread_local! {
    static RUNNING: RefCell<Vec<Weak<Inner>>> = const { RefCell::new(Vec::new()) };
}

/// Register [`pump_scans`] as an agg-gui drain hook (idempotent).
pub fn install_scan_pump() {
    agg_gui::ui_thread::add_drain_hook(pump_scans);
}

/// Note that `inner` has a scan to pump.
pub(super) fn register(inner: &Rc<Inner>) {
    RUNNING.with(|running| {
        let mut running = running.borrow_mut();
        if !running.iter().any(|w| w.as_ptr() == Rc::as_ptr(inner)) {
            running.push(Rc::downgrade(inner));
        }
    });
    agg_gui::animation::request_draw();
}

/// Advance every running scan on this thread by up to [`FRAME_BUDGET`]. The drain hook.
pub fn pump_scans() {
    // Take the list out first: pumping may finish a scan, rebuild state and register anew.
    let states: Vec<Rc<Inner>> =
        RUNNING.with(|r| r.borrow().iter().filter_map(Weak::upgrade).collect());
    let mut still_running = Vec::new();
    for inner in states {
        let state = AppState { inner };
        if state.pump(FRAME_BUDGET) {
            still_running.push(Rc::downgrade(&state.inner));
        }
    }
    RUNNING.with(|r| {
        let mut running = r.borrow_mut();
        // Keep scans registered during this pump, drop finished and dead ones.
        running.retain(|w| {
            w.upgrade()
                .is_some_and(|inner| inner.scan.borrow().is_some())
        });
        for w in still_running {
            if !running.iter().any(|kept| kept.as_ptr() == w.as_ptr()) {
                running.push(w);
            }
        }
        if !running.is_empty() {
            agg_gui::animation::request_draw_after(PUMP_INTERVAL);
        }
    });
}
