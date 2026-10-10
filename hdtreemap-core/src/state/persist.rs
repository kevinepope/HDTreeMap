//! The remembered settings (`settings.rs`) on the state side: what the user chose is read
//! back into a [`Settings`] for saving, a loaded one is applied at start-up, and every scan
//! started is put first in the recent list the start screen shows.
//!
//! The shell hands over its [`SettingsStore`] once ([`AppState::use_settings_store`]) and
//! calls [`AppState::save_settings`] from its idle hook with whether the pointer is idle;
//! agg-gui's `AutoSave` writes only when the blob changed and never mid-drag (a splitter
//! being dragged), and [`AppState::flush_settings`] writes on exit or page hide.

use std::cell::RefCell;

use agg_gui::persistence::AutoSave;

use super::{AppState, ScanTarget};
use crate::settings::{RecentScan, Settings, SettingsStore};

/// See the module docs.
#[derive(Default)]
pub(super) struct Persistence {
    store: RefCell<Option<Box<dyn SettingsStore>>>,
    auto: RefCell<AutoSave>,
    recent: RefCell<Vec<RecentScan>>,
}

impl AppState {
    fn persistence(&self) -> &Persistence {
        &self.inner.persist
    }

    /// The settings as they are now.
    pub fn settings(&self) -> Settings {
        Settings {
            size_mode: self.size_mode(),
            shading: self.shading(),
            color_by: self.color_by(),
            side_split: self.side_split().get(),
            tree_split: self.tree_split().get(),
            recent: self.recent_scans(),
        }
    }

    /// Use `settings` (at start-up, before a scan).
    pub fn apply_settings(&self, settings: &Settings) {
        self.set_size_mode(settings.size_mode);
        self.set_shading(settings.shading);
        self.set_color_by(settings.color_by);
        self.side_split().set(settings.side_split);
        self.tree_split().set(settings.tree_split);
        *self.persistence().recent.borrow_mut() = settings.recent.clone();
    }

    /// Load and apply the settings in `store`, and save to it from now on.
    pub fn use_settings_store(&self, store: Box<dyn SettingsStore>) {
        if let Some(text) = store.load() {
            self.apply_settings(&Settings::from_json(&text));
        }
        // What is in effect now is what is stored (or the defaults): no write until a change.
        self.persistence()
            .auto
            .borrow_mut()
            .seed(self.settings().to_json());
        *self.persistence().store.borrow_mut() = Some(store);
    }

    /// Write the settings if they changed and `idle` (no mouse button held). Returns
    /// whether it wrote. Called from the shell's idle hook.
    pub fn save_settings(&self, idle: bool) -> bool {
        let store = self.persistence().store.borrow();
        let Some(store) = store.as_ref() else {
            return false;
        };
        self.persistence().auto.borrow_mut().tick(
            idle,
            || self.settings().to_json(),
            |blob| store.save(blob),
        )
    }

    /// Write the settings now if they changed (on exit, or when the page is hidden).
    pub fn flush_settings(&self) -> bool {
        self.save_settings(true)
    }

    /// The folders and drives scanned before, most recent first.
    pub fn recent_scans(&self) -> Vec<RecentScan> {
        self.persistence().recent.borrow().clone()
    }

    /// Put `target` first in the recent list.
    pub(super) fn remember_scan(&self, target: &ScanTarget) {
        let mut settings = Settings {
            recent: self.recent_scans(),
            ..Settings::default()
        };
        settings.remember(RecentScan {
            path: target.path.clone(),
            volume_name: target.volume_name.clone(),
            volume_id: target.volume_id.clone(),
        });
        *self.persistence().recent.borrow_mut() = settings.recent;
    }
}
