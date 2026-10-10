//! The breadcrumb above the treemap: the path from the scan root to the zoom root, one
//! clickable segment per folder (clicking zooms out to it), Font Awesome chevrons between,
//! and the current folder last in bold. Long paths keep the first segment and as many of the
//! last ones as fit [`CHAR_BUDGET`], with "…" for the middle ([`visible_segments`]); a long
//! folder name is cut in the middle to [`MAX_SEGMENT_WIDTH`] (agg-gui's `elide_to_width`).
//!
//! The main view (`ui/main_view.rs`) wraps it in a `Rebuilder` keyed on
//! `AppState::map_version`, so it follows every zoom.

use std::sync::Arc;

use agg_gui::font_settings::default_font_size_or;
use agg_gui::{elide_to_width, Button, EllipsisMode, FlexRow, HAnchor, Label, VAnchor, Widget};

use crate::fonts::{fa, Fonts};
use crate::model::NodeId;
use crate::state::{AppState, ScanTarget};

/// Widget id of the breadcrumb row.
pub const BREADCRUMB_ID: &str = "breadcrumb";

/// About how many characters of segment names the row shows before eliding.
pub const CHAR_BUDGET: usize = 72;
/// Wider segment names are shortened in the middle (logical pixels).
pub const MAX_SEGMENT_WIDTH: f64 = 220.0;
/// Characters a chevron separator counts for.
const SEPARATOR_CHARS: usize = 3;

/// Widget id of the breadcrumb segment `depth` steps below the scan root.
pub fn segment_id(depth: usize) -> String {
    format!("breadcrumb-{depth}")
}

/// Which segments to show, given each one's length in characters: `Some(i)` for segment
/// `i`, `None` for the "…" standing in for the elided middle. Always shows the first and
/// last segment, then as many before the last as fit `budget`.
pub fn visible_segments(lengths: &[usize], budget: usize) -> Vec<Option<usize>> {
    let n = lengths.len();
    let total: usize = lengths.iter().sum::<usize>() + n.saturating_sub(1) * SEPARATOR_CHARS;
    if n <= 2 || total <= budget {
        return (0..n).map(Some).collect();
    }
    // First, "…", last: the minimum.
    let mut used = lengths[0] + lengths[n - 1] + 1 + 2 * SEPARATOR_CHARS;
    let mut first_tail = n - 1;
    while first_tail > 1 {
        let next = lengths[first_tail - 1] + SEPARATOR_CHARS;
        if used + next > budget {
            break;
        }
        used += next;
        first_tail -= 1;
    }
    let mut shown = vec![Some(0)];
    if first_tail > 1 {
        shown.push(None);
    }
    shown.extend((first_tail..n).map(Some));
    shown
}

/// `name` cut in the middle to [`MAX_SEGMENT_WIDTH`] at the segment buttons' text size.
fn shorten(name: &str, fonts: &Fonts) -> String {
    let size = default_font_size_or(14.0);
    elide_to_width(
        &fonts.ui,
        name,
        size,
        MAX_SEGMENT_WIDTH,
        EllipsisMode::Middle,
    )
}

/// Build the breadcrumb for the current zoom root of `state`'s finished scan.
pub fn build(state: &AppState, target: &ScanTarget, fonts: &Fonts) -> Box<dyn Widget> {
    let segments: Vec<(NodeId, String)> = state
        .with_result(|r, _| {
            let tree = &r.tree;
            let zoom = state.zoom_root();
            let mut chain: Vec<NodeId> = tree.ancestors(zoom).collect();
            chain.reverse();
            chain.push(zoom);
            chain
                .into_iter()
                .map(|id| {
                    let name = if id == tree.root() {
                        target.display_name()
                    } else {
                        tree.name(id).to_string()
                    };
                    (id, shorten(&name, fonts))
                })
                .collect()
        })
        // Before a scan has anything to show: just its root.
        .unwrap_or_else(|| vec![(NodeId(0), shorten(&target.display_name(), fonts))]);
    let lengths: Vec<usize> = segments.iter().map(|(_, n)| n.chars().count()).collect();
    let mut row = FlexRow::new().with_gap(4.0).with_h_anchor(HAnchor::STRETCH);
    let last = segments.len().saturating_sub(1);
    for (shown, slot) in visible_segments(&lengths, CHAR_BUDGET)
        .into_iter()
        .enumerate()
    {
        if shown > 0 {
            row = row.add(chevron(fonts));
        }
        let Some(depth) = slot else {
            row = row.add(Box::new(
                Label::new("\u{2026}", Arc::clone(&fonts.ui))
                    .with_dim(true)
                    .with_v_anchor(VAnchor::CENTER),
            ));
            continue;
        };
        let (id, name) = &segments[depth];
        if depth == last {
            row = row.add_flex(
                Box::new(
                    Label::new(name.clone(), Arc::clone(&fonts.ui))
                        .with_strong(true)
                        .with_ellipsis_if_clipped(true)
                        .with_v_anchor(VAnchor::CENTER)
                        .with_name(segment_id(depth)),
                ),
                1.0,
            );
        } else {
            let zoom_state = state.clone();
            let id = *id;
            row = row.add(Box::new(
                Button::new(name.clone(), Arc::clone(&fonts.ui))
                    .with_ghost()
                    .with_compact()
                    .with_focus_on_click(false)
                    .on_click(move || {
                        zoom_state.set_zoom_root(id);
                    })
                    .with_v_anchor(VAnchor::CENTER)
                    .with_name(segment_id(depth)),
            ));
        }
    }
    Box::new(row.with_name(BREADCRUMB_ID))
}

fn chevron(fonts: &Fonts) -> Box<dyn Widget> {
    Box::new(
        Label::new(fa::CHEVRON_RIGHT.to_string(), Arc::clone(&fonts.ui))
            .with_font_size(10.0)
            .with_dim(true)
            .with_v_anchor(VAnchor::CENTER),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_paths_show_every_segment() {
        assert_eq!(visible_segments(&[], 10), Vec::<Option<usize>>::new());
        assert_eq!(visible_segments(&[50], 10), vec![Some(0)]);
        assert_eq!(visible_segments(&[50, 50], 10), vec![Some(0), Some(1)]);
        assert_eq!(
            visible_segments(&[5, 5, 5], 30),
            vec![Some(0), Some(1), Some(2)]
        );
    }

    #[test]
    fn long_paths_elide_the_middle() {
        // 10 segments of 10 chars: 100 + 27 separators.
        let lengths = [10; 10];
        let shown = visible_segments(&lengths, 60);
        assert_eq!(shown[0], Some(0));
        assert_eq!(shown[1], None);
        assert_eq!(shown.last(), Some(&Some(9)));
        // first (10) + … (1) + last (10) + 2 separators (6) = 27; each more is 13: two fit.
        assert_eq!(shown, vec![Some(0), None, Some(7), Some(8), Some(9)]);
        // Nothing fits but the ends.
        assert_eq!(
            visible_segments(&[40, 40, 40], 50),
            vec![Some(0), None, Some(2)]
        );
    }

    #[test]
    fn long_names_are_shortened_in_the_middle() {
        let fonts = crate::fonts::install_theme_and_fonts(1.0).expect("fonts");
        assert_eq!(shorten("short", &fonts), "short");
        let long = "a".repeat(60) + &"b".repeat(60);
        let short = shorten(&long, &fonts);
        assert!(short.chars().count() < long.chars().count());
        assert!(short.starts_with("aaaa") && short.ends_with("bbbb"));
        assert!(short.contains('\u{2026}'));
    }
}
