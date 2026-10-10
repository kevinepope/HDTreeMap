# HDTreeMap

A small, fast disk-usage treemap viewer for macOS, Windows and Linux, written in Rust on
[agg-gui](https://github.com/larsbrubaker/agg-gui). Point it at a drive or folder and it
shows every file as a shaded rectangle whose area is proportional to its size, so the big
space hogs stand out at a glance.

**Live demo:** <https://larsbrubaker.github.io/HDTreeMap/> runs the app in your browser
against simulated drives. It never reads anything on your computer.

## Status

Early development. Milestones M0–M2 are done: pick a drive or folder, watch the parallel
scan, then explore the result as a cushion treemap. The linked folder tree and extension
legend come next. See [docs/PLAN.md](docs/PLAN.md) for the full plan.

## Features

- Parallel scan with live progress and Cancel. Stays on one filesystem, doesn't follow
  symlinks, and counts hard links once. Unreadable folders are counted, not fatal.
- Squarified treemap with cushion shading (or flat), coloured by file extension; folders
  too small to open and leftover small files show as grey blocks, so all space is shown.
- Hover for the path, size and share of each item (status bar and tooltip); click to
  select; double-click to zoom into a folder; Backspace, the mouse back button or the
  breadcrumb to zoom out.
- "Size on disk" or apparent size, switchable without rescanning.
- Status bar with totals, unreadable items and free space.

## Planned features

- Linked folder tree and extension legend.
- Reveal in Finder / Explorer / file manager, Open, Copy Path, Rescan, and Move to Trash
  (with confirmation; never a permanent delete).
- The same app in the browser as a demo, on simulated drives.

## Build and run

You need a Rust toolchain (1.87 or newer). The workspace expects
[agg-gui](https://github.com/larsbrubaker/agg-gui) checked out next to this repository
(`../agg-gui`), as it is in the `rust-apps` superproject:

```sh
git clone https://github.com/larsbrubaker/agg-gui.git ../agg-gui
```

Native app:

```sh
cargo run --release             # start screen
cargo run --release -- ~/Code   # open straight on a folder
```

Web demo (needs [wasm-pack](https://rustwasm.github.io/wasm-pack/) and [bun](https://bun.sh/)):

```sh
wasm-pack build hdtreemap-wasm --target web --out-dir ../demo/public/pkg --no-typescript
cd demo
bun install
bun run dev
```

Checks (the same as CI): `scripts/pre-commit-check.sh` (or `.ps1` on Windows).

## License

MIT. See [LICENSE](LICENSE).
