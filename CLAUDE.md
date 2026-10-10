# HDTreeMap — working charter

HDTreeMap is a small, fast disk-usage treemap viewer for macOS, Windows and Linux, built on
agg-gui and shipped native plus WebAssembly (a GitHub Pages demo on simulated drives).
`docs/PLAN.md` is the spec: the feature set, architecture and milestones (M0–M6). Read it
before writing code, and keep it current when a decision changes.

## The pillars

**No stubs, no shortcuts.** Every function is complete and production-ready. No `todo!()`,
no `unimplemented!()`, no placeholder screens or partial implementations. If a dependency
isn't ready, build it first.

**Test-first bug fixing.** 1) Write a failing test that reproduces the bug. 2) Fix it.
3) Confirm the test passes. Never commit a bug fix that isn't covered by a test. Never
weaken, `#[ignore]` or delete a test to make it pass.

**All UI through agg-gui.** The strongest invariant. The shells (`hdtreemap-native`,
`hdtreemap-wasm`) create the window or canvas, supply platform services and get out of the
way: no widget construction, no user-facing strings, no HTML/CSS UI beyond the bare canvas.
Build screens from agg-gui's stock widgets (`FlexColumn`, `Button`, `Label`, `ProgressBar`,
`TreeView`, `Table`, `PopupMenu`, `ModalSheet`, ...); write a custom widget only where the
app genuinely needs one (the treemap view).

**Icons are Font Awesome** (4.7, agg-gui's bundled face) by Unicode code point, named in
`hdtreemap-core/src/fonts.rs` (`fa::*`). No image icons.

## Crate layout

```
hdtreemap-core/     rlib: everything testable, no platform code
  src/fonts.rs        theme + fonts, Font Awesome code points (fa::*)
  src/theme.rs        Theme { Light (default), Dark, System }: agg-gui Visuals per theme plus
                      HDTreeMap's map greys, outline and icon colours
  src/format.rs       human-readable sizes
  src/icon.rs         the app icon, drawn by the treemap layout + rasteriser (window icon,
                      release assets)
  src/platform.rs     PlatformServices trait (volumes, picker, reveal/open/trash, open_url,
                      scan_source), VolumeInfo, volume-list cleanup (dedupe by VolumeKey)
  src/platform/demo.rs  DemoPlatform: the web demo's simulated drives
  src/platform/mountinfo.rs  Linux /proc/self/mountinfo: parse, mounts to skip (pure)
  src/model/          arena Tree (40-byte Node, LOSSY_NAME flag, exact_path), TreeBuilder
                      (name-slab limit, running extension totals, time-sliced sort),
                      SizeMode, ExtTable; splice.rs: replace_subtree
  src/scan/           ScanSource (poll, partial tree), ScanProgress/ScanErrors; native:
                      walk.rs (jwalk walk -> records), assemble.rs (records -> tree),
                      fs.rs (FsScanner: worker sends record batches, poll assembles),
                      fs_meta.rs (per-OS metadata, root resolution, mounts, firmlinks),
                      apfs.rs (macOS bulk metadata, clone accounting), walk_stats.rs
  src/settings.rs     remembered settings + recent scans as JSON; SettingsStore trait
  src/demo_fs/        DemoScanner + per-drive recipes for the simulated drives (all targets)
  src/layout/         squarified cushion layout (Y-down physical pixels), aggregates, hit grid
  src/render/         palette (by type or top folder) + cushion rasteriser (rayon row bands
                      natively), HDTREEMAP_STATS (stats_line; a shell can redirect it)
  src/state.rs        AppState (screen, running scan, finished result, with_result over a
                      live or finished ScanView); state/pump.rs polls scans and folder
                      rescans once per frame (an agg-gui ui_thread drain hook); live.rs:
                      throttled live ticks of the partial map; persist.rs: settings and
                      recent scans; view.rs: zoom, hover, selection, highlight, shading,
                      colour scheme, size mode, versions;
                      theme.rs: theme choice + reported OS appearance, theme_version;
                      derived.rs: shared palette and zoom-root ExtTable; volumes.rs: cached
                      volume list; edits.rs: remove_node, folder rescan + splice;
                      notices.rs: the agg-gui Toasts queue, trash/About sheets, context-menu requests, quit
  src/ui/             widgets. mod.rs: root Stack (menu bar + screen, overlays);
                      start_screen (drives, Recent), main_view (toolbar, breadcrumb,
                      scan_strip while scanning, splitters, status bar; shown live from
                      the start of a scan), treemap_view/ (custom TreemapView;
                      geometry.rs raster size, highlight.rs), folder_tree/ (TreeView wrapper, rows.rs lazy
                      rows), ext_legend.rs (Table), describe.rs; commands.rs (menu
                      commands), node_actions.rs (reveal/open/copy/zoom/rescan/trash),
                      menus.rs (menu spec, in-window MenuBar, shortcuts), context_menu.rs
                      (PopupMenu host), sheets.rs (trash confirm, About); the root
                      sits in agg-gui's ToastHost (toasts from AppState::notify)
  tests/              headless UI tests (common/ harness with a recording FakePlatform),
                      scanner tests, live_map, color_by, actions/menus tests,
                      file_line_count
  examples/           bench_treemap (layout/raster timing), bench_views (whole-frame
                      timing), bench_live_scan (live vs blocking scan), make_icons
                      (writes assets/icon/: PNGs, .ico, .icns; committed)
hdtreemap-native/   bin `hdtreemap`: agg-gui-shell + NativePlatform (sysinfo, rfd, opener, trash),
                    CLI (`--demo [ID]`, `--dump-top N [PATH | --demo ID]`, `--highlight EXT`),
                    menu_bar.rs (macOS system menu from the core's spec), show.rs
                    (HDTREEMAP_SCREENSHOT_SHOW input replay), persist.rs (settings.json,
                    window.json in the OS config dir), a ShellHost for Quit and saving
                    theme.rs: OS appearance via winit for Match System, title-bar theme
hdtreemap-wasm/     cdylib: agg-gui-web-shell + WebPlatform (DemoPlatform that opens links),
                    prefers-color-scheme for Match System,
                    localStorage settings, ?drive=ID / ?stats, hdtreemap_status() export
demo/               static page + bun/vite bundle for GitHub Pages
```

Add a module when it holds real code, not before.

## PlatformServices and ScanSource

Two seams keep the core free of platform code and let the browser demo run the real app:

- **`PlatformServices`** (`platform.rs`): what the UI needs from the OS — list volumes,
  pick a folder, reveal, open, trash, open a web link, `is_demo`, and `scan_source` (which `ScanSource`
  scans a target). `NativePlatform` (in
  `hdtreemap-native`) talks to the real OS; `DemoPlatform` (in the core, all targets) lists
  descriptive simulated drives and reports actions as simulated. Pure logic that the native
  impl relies on (filtering pseudo filesystems, de-duplicating mounts) lives in the core so
  it is unit-tested on every OS.
- **`ScanSource`** (`scan/mod.rs`): how a tree is produced. `FsScanner` (native only, jwalk +
  rayon) walks the real disk; `DemoScanner` (all targets, time-sliced for single-threaded
  wasm) streams a synthetic tree whose shape comes from the chosen demo drive's profile id
  (`VolumeInfo::id`). Both feed the same incremental tree builder, so the UI cannot tell
  them apart.

Widgets never ask "am I on the web?" through `cfg`; they ask the platform (`is_demo`).

## Local development uses agg-gui as a path dep — improve it as you go

`Cargo.toml` patches `agg-gui` and `agg-gui-shell` to the sibling checkout `../agg-gui/`,
and `agg-gui-web-shell` is a path dependency there. When HDTreeMap needs a capability
agg-gui lacks, add it to agg-gui itself (then Lars publishes a new version) — never a
local workaround. CI clones agg-gui beside the repo so the paths resolve there too.

## Build and test

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --package hdtreemap-core --package hdtreemap-native --package hdtreemap-wasm
cargo run -p hdtreemap-native [-- PATH]
cargo check -p hdtreemap-wasm --target wasm32-unknown-unknown
wasm-pack build hdtreemap-wasm --target web --out-dir ../demo/public/pkg --no-typescript
```

`HDTREEMAP_SCREENSHOT=out.png cargo run -p hdtreemap-native` captures a frame and exits;
`HDTREEMAP_SCREENSHOT_SHOW=context-menu|trash|menu|about|color-folder|toast|hover` opens that UI first
(`HDTREEMAP_SCREENSHOT_AT=fx,fy` places the hover; with
`HDTREEMAP_IN_WINDOW_MENU=1` for the in-window bar on macOS). Never confirm a trash on real
files while testing: point it at a temporary folder.
`HDTREEMAP_STATS=1` prints treemap layout / raster times, per-frame layout+paint time,
live ticks, and a scan's slowest folders (`?stats` on the web, to the console).
`HDTREEMAP_SETTINGS_DIR=dir` keeps settings and the window size there (screenshot runs
without it use neither).
`cargo run --release -p hdtreemap-native -- --dump-top 15 PATH` checks the scanner headless.
Run `scripts/pre-commit-check.sh` (or `.ps1`) before every commit; it mirrors CI.
On macOS, `run-mac.command` (double-click, or `./run-mac.command [PATH | --demo ID]`) builds
the release binary and launches it.

## Workflow: commit as you go

Work in small, complete steps and commit each one as soon as it is green (fmt, clippy
`-D warnings`, tests): one logical change per commit, with a message that says what changed
and why. Don't batch a milestone into one commit, and don't leave a green step uncommitted
while starting the next. Push `main` when a milestone step lands, then watch CI
(`gh run list`) and fix anything red before moving on.

## Performance

Never guess: measure first (a stats env var in the style of Critters' `CRITTERS_STATS`).
Average frame time stays under 10 ms when nothing changed. Each optimisation must show up
in the numbers or it is reverted.

## Coding standards

- Files stay at or below 800 lines (`file_line_count` test). Split into real modules;
  never squeeze under by deleting comments or blank lines.
- Every file starts with a comment block saying what it is for and how it relates to the
  rest of the project.
- No `unwrap()` outside `main` and tests; prefer `Result` / `Option`. `expect` is fine in
  `main` for a start-up failure with a clear message.
- No `unsafe` unless there is no alternative; document every `unsafe` block.
- Rust naming conventions; doc comments where they clarify intent.
