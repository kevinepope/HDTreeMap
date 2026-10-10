//! `NativePlatform`: hdtreemap-core's `PlatformServices` on the real operating system.
//!
//! | Service | macOS | Windows | Linux |
//! |---|---|---|---|
//! | Volumes | sysinfo `Disks` | sysinfo `Disks` | sysinfo `Disks` |
//! | Pick folder | rfd | rfd | rfd (xdg desktop portal) |
//! | Reveal | `open -R` | `explorer /select,` | FileManager1 `ShowItems` over `dbus-send`, else `xdg-open` on the parent |
//! | Open | opener | opener | opener |
//! | Trash | trash | trash | trash |
//!
//! The volume list is cleaned up by the pure, unit-tested helpers in
//! `hdtreemap_core::platform` (pseudo filesystems, system mounts, duplicates).

use std::path::{Path, PathBuf};
use std::process::Command;

use hdtreemap_core::platform::{
    dedupe_volumes, is_listable_volume, volume_display_name, ActionOutcome, ActionResult,
    PlatformError, PlatformServices, VolumeInfo,
};
use sysinfo::Disks;

/// The real machine.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativePlatform;

impl PlatformServices for NativePlatform {
    fn list_volumes(&self) -> Vec<VolumeInfo> {
        let disks = Disks::new_with_refreshed_list();
        let listed = disks
            .list()
            .iter()
            .filter(|disk| {
                is_listable_volume(
                    &disk.file_system().to_string_lossy(),
                    disk.mount_point(),
                    disk.total_space(),
                )
            })
            .map(|disk| {
                let device = disk.name().to_string_lossy().into_owned();
                let mount_path = disk.mount_point().to_path_buf();
                let volume = VolumeInfo {
                    id: mount_path.to_string_lossy().into_owned(),
                    name: volume_display_name(&device, &mount_path),
                    mount_path,
                    total_bytes: disk.total_space(),
                    free_bytes: disk.available_space(),
                    description: None,
                };
                (device, volume)
            })
            .collect();
        dedupe_volumes(listed)
    }

    fn pick_folder(&self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("Choose a folder to scan")
            .pick_folder()
    }

    fn reveal(&self, path: &Path) -> ActionResult {
        reveal_in_file_manager(path)
    }

    fn open(&self, path: &Path) -> ActionResult {
        opener::open(path)
            .map(|()| ActionOutcome::Done)
            .map_err(|err| PlatformError::new(format!("Could not open {}: {err}", path.display())))
    }

    fn trash(&self, path: &Path) -> ActionResult {
        trash::delete(path)
            .map(|()| ActionOutcome::Done)
            .map_err(|err| {
                PlatformError::new(format!(
                    "Could not move {} to the trash: {err}",
                    path.display()
                ))
            })
    }

    fn is_demo(&self) -> bool {
        false
    }
}

fn reveal_error(path: &Path, err: impl std::fmt::Display) -> PlatformError {
    PlatformError::new(format!("Could not reveal {}: {err}", path.display()))
}

/// Run `command` to completion and turn a launch failure or a failing exit status into an
/// error. (Not used on Windows, where explorer's exit status is meaningless.)
#[cfg(not(windows))]
fn run_checked(mut command: Command, path: &Path) -> ActionResult {
    let status = command.status().map_err(|err| reveal_error(path, err))?;
    if status.success() {
        Ok(ActionOutcome::Done)
    } else {
        Err(reveal_error(path, status))
    }
}

#[cfg(target_os = "macos")]
fn reveal_in_file_manager(path: &Path) -> ActionResult {
    let mut command = Command::new("open");
    command.arg("-R").arg(path);
    run_checked(command, path)
}

#[cfg(windows)]
fn reveal_in_file_manager(path: &Path) -> ActionResult {
    // explorer.exe reports exit code 1 even when it worked, so only a launch failure counts.
    Command::new("explorer")
        .arg("/select,")
        .arg(path)
        .spawn()
        .map(|_| ActionOutcome::Done)
        .map_err(|err| reveal_error(path, err))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn reveal_in_file_manager(path: &Path) -> ActionResult {
    // The freedesktop FileManager1 interface selects the item in the user's file manager
    // (Nautilus, Dolphin, Nemo, ...). Without it, open the containing folder instead.
    let mut dbus = Command::new("dbus-send");
    dbus.args([
        "--session",
        "--print-reply",
        "--dest=org.freedesktop.FileManager1",
        "--type=method_call",
        "/org/freedesktop/FileManager1",
        "org.freedesktop.FileManager1.ShowItems",
    ])
    .arg(format!("array:string:{}", file_uri(path)))
    .arg("string:");
    if run_checked(dbus, path).is_ok() {
        return Ok(ActionOutcome::Done);
    }
    let folder = path.parent().unwrap_or(path);
    let mut xdg = Command::new("xdg-open");
    xdg.arg(folder);
    run_checked(xdg, path)
}

/// `file://` URI for an absolute path, percent-encoding every byte outside RFC 3986's
/// unreserved set and `/`.
#[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
fn file_uri(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let mut uri = String::from("file://");
    for byte in raw.bytes() {
        let unreserved = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        if unreserved || byte == b'/' {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uris_escape_spaces_and_non_ascii() {
        assert_eq!(
            file_uri(Path::new("/home/me/a.txt")),
            "file:///home/me/a.txt"
        );
        assert_eq!(
            file_uri(Path::new("/home/me/My Files/ü#1")),
            "file:///home/me/My%20Files/%C3%BC%231"
        );
    }

    #[test]
    fn native_platform_lists_real_volumes() {
        let volumes = NativePlatform.list_volumes();
        assert!(!NativePlatform.is_demo());
        // Every machine that runs the tests has at least the volume holding the checkout.
        assert!(!volumes.is_empty(), "no volumes listed");
        for v in &volumes {
            assert!(v.total_bytes > 0);
            assert!(v.description.is_none());
            assert_eq!(v.id, v.mount_path.to_string_lossy());
        }
    }
}
