//! Unit tests for the edits after a scan (`state/edits.rs`: Move to Trash's removal,
//! Rescan Folder's splice) and the transient UI state (`state/notices.rs`: toasts, the
//! trash confirmation).

use std::time::Duration;

use super::tests::{finished_usb, usb_scan};
use super::*;
use crate::model::NodeId;

/// The biggest folder below the root, and its biggest file child or grandchild.
fn folder_and_file(state: &AppState) -> (NodeId, NodeId) {
    state
        .with_result(|r, _| {
            let tree = &r.tree;
            let folder = tree
                .children(tree.root())
                .find(|&c| tree.node(c).is_some_and(|n| n.is_dir()))?;
            let mut stack = vec![folder];
            while let Some(d) = stack.pop() {
                for c in tree.children(d) {
                    if tree.node(c).is_some_and(|n| !n.is_dir()) {
                        return Some((folder, c));
                    }
                    stack.push(c);
                }
            }
            None
        })
        .flatten()
        .expect("a folder with a file")
}

#[test]
fn removing_a_node_updates_totals_extensions_and_the_view() {
    let state = finished_usb();
    let (folder, file) = folder_and_file(&state);
    let (total, ext, ext_total) = state
        .with_result(|r, exts| {
            let ext = r.tree.node(file).expect("file").ext();
            (r.tree.total(), ext, exts.by_ext(ext).map(|row| row.count))
        })
        .expect("result");
    let size = state.with_result(|r, _| r.tree.sizes(file)).expect("size");
    state.set_zoom_root(folder);
    state.set_selected(Some(file));
    let v = state.tree_version();
    assert_eq!(state.remove_node(file), Ok(size));
    assert!(state.tree_version() > v);
    assert_eq!(state.selected(), None, "a removed selection is dropped");
    assert_eq!(state.zoom_root(), folder, "the zoom root is still live");
    state.with_result(|r, exts| {
        assert_eq!(r.tree.total().allocated, total.allocated - size.allocated);
        assert_eq!(
            exts.by_ext(ext).map(|row| row.count),
            ext_total.map(|c| c - 1).filter(|&c| c > 0)
        );
    });
    // Removing the zoom root itself zooms out to the nearest live folder.
    assert!(state.remove_node(folder).is_ok());
    assert_eq!(state.zoom_root(), NodeId(0));
    assert_eq!(
        state.remove_node(folder),
        Err(crate::model::RemoveError::Missing)
    );
    assert_eq!(
        state.remove_node(NodeId(0)),
        Err(crate::model::RemoveError::Root)
    );
}

#[test]
fn a_folder_rescan_splices_the_fresh_folder_back_in() {
    let state = finished_usb();
    let (folder, file) = folder_and_file(&state);
    let before = state
        .with_result(|r, _| r.tree.sizes(folder))
        .expect("sizes");
    state.remove_node(file).expect("remove");
    assert_ne!(state.with_result(|r, _| r.tree.sizes(folder)), Some(before));
    // The demo source regenerates the whole drive; the folder is found in it by path.
    assert!(state.start_rescan(folder, usb_scan()));
    assert!(state.is_rescanning());
    let status = state.rescan_status().expect("status");
    assert!(status.path.ends_with(
        state
            .with_result(|r, _| r.tree.name(folder).to_string())
            .expect("name")
    ));
    let v = state.tree_version();
    let mut polls = 0;
    while state.pump(Duration::from_millis(50)) {
        polls += 1;
        assert!(polls < 10_000);
    }
    assert!(!state.is_rescanning());
    assert!(state.tree_version() > v);
    assert_eq!(state.with_result(|r, _| r.tree.sizes(folder)), Some(before));
    assert!(state
        .with_result(|r, _| r.tree.is_live(folder))
        .unwrap_or(false));
    let toasts = state.toasts().texts();
    assert!(
        toasts.last().is_some_and(|t| t.starts_with("Rescanned")),
        "{toasts:?}"
    );
    // A file is not a folder to rescan.
    let other_file = folder_and_file(&state).1;
    assert!(!state.start_rescan(other_file, usb_scan()));
}

#[test]
fn toasts_keep_the_latest_few() {
    let _clock = agg_gui::clock::scoped_virtual(None);
    let state = AppState::new();
    for i in 0..6 {
        state.notify(format!("note {i}"));
    }
    state.notify_error("bad");
    // The oldest beyond MAX_TOASTS fade out at once.
    agg_gui::clock::advance(agg_gui::widgets::toast::TOAST_FADE_OUT + Duration::from_millis(1));
    assert_eq!(
        state.toasts().texts(),
        vec!["note 3", "note 4", "note 5", "bad"],
        "{MAX_TOASTS} at most"
    );
    // An error stays up longer than the rest.
    agg_gui::clock::advance(agg_gui::widgets::toast::DEFAULT_TOAST_DURATION);
    assert_eq!(state.toasts().texts(), vec!["bad"]);
    agg_gui::clock::advance(agg_gui::widgets::toast::ERROR_TOAST_DURATION);
    assert!(state.toasts().is_empty());
}

#[test]
fn trash_requests_open_and_cancel_the_sheet() {
    let state = AppState::new();
    let sheet = state.trash_sheet();
    let v = state.sheet_version();
    state.request_trash(NodeId(3));
    assert!(sheet.get());
    assert_eq!(state.trash_request(), Some(NodeId(3)));
    assert!(state.sheet_version() > v);
    state.cancel_trash();
    assert!(!sheet.get());
    assert_eq!(state.trash_request(), None);
}
