#!/bin/bash
# run-mac.command: double-click in Finder (or run from a terminal) to build and launch
# HDTreeMap on macOS. It builds an optimised release binary the first time, which takes a
# few minutes; later launches only rebuild what changed. Arguments are passed through to
# the app, e.g. `./run-mac.command ~/Downloads` or `./run-mac.command --demo laptop-ssd`.
#
# The build expects the agg-gui checkout beside this repo (../agg-gui), as in the
# rust-apps superproject; see README "Build from source".

set -u
cd "$(dirname "$0")" || exit 1

# Finder starts .command files in a fresh login shell, which may not have cargo on PATH.
if ! command -v cargo >/dev/null 2>&1 && [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
fi

fail() {
    echo
    echo "HDTreeMap: $1"
    # Keep the Terminal window open so the message can be read after a double-click.
    read -r -p "Press Return to close."
    exit 1
}

command -v cargo >/dev/null 2>&1 || fail "cargo was not found. Install Rust from https://rustup.rs and try again."
[ -d ../agg-gui ] || fail "../agg-gui was not found. Clone https://github.com/larsbrubaker/agg-gui beside this folder (or use the rust-apps superproject)."

echo "Building HDTreeMap (release)..."
cargo build --release -p hdtreemap-native || fail "the build failed (see the errors above)."

echo "Starting HDTreeMap..."
exec ./target/release/hdtreemap "$@"
