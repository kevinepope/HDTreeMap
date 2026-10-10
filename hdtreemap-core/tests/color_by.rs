//! Headless tests of colouring the map by top-level folder (View ▸ Colour By, the
//! toolbar's "By Type / By Folder" switch): the map re-rasterises without a new layout, the
//! legend lists the zoom root's top folders with their colours, a click on one selects it,
//! zooming recolours by the new root's children, and the menu and switch show the scheme.

mod common;

use std::rc::Rc;

use common::{demo_map, Harness};
use hdtreemap_core::platform::DemoPlatform;
use hdtreemap_core::render::ColorBy;
use hdtreemap_core::ui::commands::{Command, Commands};
use hdtreemap_core::ui::ext_legend::{Column, ExtLegend, EXT_LEGEND_ID};
use hdtreemap_core::ui::main_view::COLOR_BY_ID;

fn legend(h: &Harness) -> &ExtLegend {
    h.widget::<ExtLegend>(EXT_LEGEND_ID)
}

#[test]
fn colouring_by_top_folder_recolours_the_map_and_lists_folders_in_the_legend() {
    let mut h = demo_map("dev-workstation");
    let platform: Rc<dyn hdtreemap_core::PlatformServices> = Rc::new(DemoPlatform::unpaced());
    let commands = Commands::new(&h.state, &platform);
    assert_eq!(
        commands.checked(Command::ColorBy(ColorBy::Extension)),
        Some(true)
    );
    let before = h.state.map_counters();
    assert!(legend(&h).rows().iter().all(|r| r.node.is_none()));

    commands.run(Command::ColorBy(ColorBy::TopFolder));
    h.frame();
    assert_eq!(h.state.color_by(), ColorBy::TopFolder);
    assert_eq!(
        commands.checked(Command::ColorBy(ColorBy::TopFolder)),
        Some(true)
    );
    let after = h.state.map_counters();
    assert_eq!(after.layouts, before.layouts, "no new layout");
    assert!(after.rasters > before.rasters, "the map was repainted");

    let palette = h.state.palette().expect("palette");
    assert_eq!(palette.color_by(), ColorBy::TopFolder);
    let rows = legend(&h).rows();
    let children = h
        .state
        .with_result(|r, _| r.tree.children_by_size(r.tree.root()))
        .expect("tree");
    assert_eq!(rows.len(), children.len().min(1000));
    assert_eq!(rows[0].node, Some(children[0]));
    assert_eq!(rows[0].color, palette.folder_color(children[0]));
    assert_ne!(
        rows[0].color, rows[1].color,
        "top folders get their own hues"
    );
    assert!(legend(&h)
        .cell_text(0, Column::Extension)
        .is_some_and(|t| !t.starts_with('.')));

    // A click on a folder row selects it; another clears it.
    let rect = legend(&h).row_rect(0).expect("row 0 in view");
    let (x, y) = h.point_at_local(EXT_LEGEND_ID, rect);
    h.click_at(x, y);
    assert_eq!(h.state.selected(), Some(children[0]));
    h.click_at(x, y);
    assert_eq!(h.state.selected(), None);

    // Zooming in colours by the new root's children.
    let folder = children
        .iter()
        .copied()
        .find(|&c| {
            h.state
                .with_result(|r, _| r.tree.node(c).is_some_and(|n| n.is_dir()))
                .unwrap_or(false)
        })
        .expect("a top folder");
    assert!(h.state.set_zoom_root(folder));
    h.frame();
    let zoomed = h.state.palette().expect("palette");
    let inner = h
        .state
        .with_result(|r, _| r.tree.children_by_size(folder))
        .expect("tree");
    assert_eq!(
        zoomed.folders().first().map(|f| f.0),
        inner.first().copied()
    );
    assert_eq!(legend(&h).rows()[0].node, inner.first().copied());

    // Back by type.
    commands.run(Command::ColorBy(ColorBy::Extension));
    h.frame();
    assert!(legend(&h).rows().iter().all(|r| r.node.is_none()));
}

#[test]
fn the_toolbar_switch_picks_the_scheme() {
    let mut h = demo_map("usb-stick");
    let (x, y) = h.point_in(COLOR_BY_ID, 0.75, 0.5);
    h.click_at(x, y);
    assert_eq!(h.state.color_by(), ColorBy::TopFolder);
    let (x, y) = h.point_in(COLOR_BY_ID, 0.25, 0.5);
    h.click_at(x, y);
    assert_eq!(h.state.color_by(), ColorBy::Extension);
}
