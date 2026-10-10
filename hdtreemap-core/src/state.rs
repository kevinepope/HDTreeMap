//! Application state shared by the widget tree, the shells and the tests.
//!
//! The UI runs on one thread (the browser main thread on wasm), so state lives in `Rc` cells:
//! widgets hold clones of [`AppState`] and read it when they are (re)built, and tests mutate
//! the same cells directly. Every change bumps [`AppState::version`], which the root
//! `Rebuilder` in `ui/mod.rs` watches to swap screens.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use crate::platform::VolumeInfo;

/// What the user chose to scan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanTarget {
    /// Folder or volume mount path to scan.
    pub path: PathBuf,
    /// The volume's display name when a volume (rather than a folder) was chosen.
    pub volume_name: Option<String>,
    /// The volume's stable id (a demo profile on the web) when a volume was chosen.
    pub volume_id: Option<String>,
}

impl ScanTarget {
    pub fn folder(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            volume_name: None,
            volume_id: None,
        }
    }

    pub fn volume(volume: &VolumeInfo) -> Self {
        Self {
            path: volume.mount_path.clone(),
            volume_name: Some(volume.name.clone()),
            volume_id: Some(volume.id.clone()),
        }
    }
}

/// Which screen the app shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Volume list and "Choose Folder…".
    Start,
    /// A target has been chosen.
    Target(ScanTarget),
}

/// Everything the UI reads and writes. Cheap to clone; clones share the same cells.
#[derive(Clone)]
pub struct AppState {
    screen: Rc<RefCell<Screen>>,
    version: Rc<Cell<u64>>,
}

impl AppState {
    /// Start on the start screen.
    pub fn new() -> Self {
        Self {
            screen: Rc::new(RefCell::new(Screen::Start)),
            version: Rc::new(Cell::new(0)),
        }
    }

    /// Start on the target screen when a path was given (the native command line), else on
    /// the start screen.
    pub fn with_initial_path(path: Option<PathBuf>) -> Self {
        let state = Self::new();
        if let Some(path) = path {
            *state.screen.borrow_mut() = Screen::Target(ScanTarget::folder(path));
        }
        state
    }

    pub fn screen(&self) -> Screen {
        self.screen.borrow().clone()
    }

    /// Bumped on every change; cheap to poll each layout.
    pub fn version(&self) -> u64 {
        self.version.get()
    }

    /// Record the chosen target and switch to its screen.
    pub fn choose(&self, target: ScanTarget) {
        self.set_screen(Screen::Target(target));
    }

    /// Return to the start screen.
    pub fn back_to_start(&self) {
        self.set_screen(Screen::Start);
    }

    fn set_screen(&self, screen: Screen) {
        *self.screen.borrow_mut() = screen;
        self.version.set(self.version.get().wrapping_add(1));
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choosing_and_going_back_bumps_the_version() {
        let state = AppState::new();
        assert_eq!(state.screen(), Screen::Start);
        let v0 = state.version();
        state.choose(ScanTarget::folder("/home/me"));
        assert_eq!(
            state.screen(),
            Screen::Target(ScanTarget::folder("/home/me"))
        );
        assert!(state.version() > v0);
        let v1 = state.version();
        state.back_to_start();
        assert_eq!(state.screen(), Screen::Start);
        assert!(state.version() > v1);
    }

    #[test]
    fn a_command_line_path_starts_on_the_target_screen() {
        let state = AppState::with_initial_path(Some(PathBuf::from("/data")));
        assert_eq!(state.screen(), Screen::Target(ScanTarget::folder("/data")));
        assert_eq!(AppState::with_initial_path(None).screen(), Screen::Start);
    }

    #[test]
    fn volume_targets_carry_the_volume_identity() {
        let volume = VolumeInfo {
            id: "usb-stick".into(),
            name: "Small USB Stick".into(),
            mount_path: PathBuf::from("/demo/usb-stick"),
            total_bytes: 10,
            free_bytes: 5,
            description: None,
        };
        let target = ScanTarget::volume(&volume);
        assert_eq!(target.path, PathBuf::from("/demo/usb-stick"));
        assert_eq!(target.volume_id.as_deref(), Some("usb-stick"));
        assert_eq!(target.volume_name.as_deref(), Some("Small USB Stick"));
    }
}
