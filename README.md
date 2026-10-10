# HDTreeMap

A small, fast disk-usage treemap viewer for macOS, Windows and Linux, written in Rust on
[agg-gui](https://github.com/larsbrubaker/agg-gui). Point it at a drive or folder and it
shows every file as a shaded rectangle whose area is proportional to its size, so the big
space hogs stand out at a glance.

**Live demo:** <https://larsbrubaker.github.io/HDTreeMap/> runs the app in your browser
against simulated drives. It never reads anything on your computer.

## Status

Early development. Milestone M0 (scaffold) is done: the start screen lists your mounted
volumes with used/free bars, and you can choose a folder. Scanning and the treemap itself
come next. See [docs/PLAN.md](docs/PLAN.md) for the full plan.

## Planned features

- Parallel scan with live progress and Cancel. Stays on one filesystem, doesn't follow
  symlinks, and counts hard links once.
- Squarified treemap with cushion shading, coloured by file extension.
- Linked folder tree, extension legend, breadcrumb zoom and status bar.
- "Size on disk" or apparent size, switchable without rescanning.
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
