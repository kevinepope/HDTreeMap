# Changelog

All notable changes to HDTreeMap are documented here.

## Unreleased

### Added
- M0 scaffold: a workspace with `hdtreemap-core`, `hdtreemap-native` (the `hdtreemap`
  binary) and `hdtreemap-wasm`, plus the `demo/` page for GitHub Pages.
- Start screen listing mounted volumes with a used/free bar and sizes, and a
  "Choose Folder…" button with the native folder picker. A folder given on the command
  line opens straight on that target.
- `PlatformServices` (volumes, folder picker, reveal, open, trash) with a native
  implementation and a simulated `DemoPlatform` offering six descriptive demo drives.
- CI on Linux, macOS and Windows (fmt, clippy, tests), a wasm check, and a Pages deploy.
