#!/usr/bin/env bash
# Builds HDTreeMap.app for macOS and zips it: a universal (Apple Silicon + Intel) binary
# made with lipo, packaging/macos/Info.plist with the version from Cargo.toml, and the
# icon from assets/icon/hdtreemap.icns. The bundle is ad-hoc signed only (no Developer
# ID), so Gatekeeper asks before the first launch; the README says how to open it.
#
# Used by .github/workflows/release.yml, and runnable locally:
#   scripts/package-macos.sh [OUT_DIR]      (default: dist/)
# Needs both Rust targets: rustup target add aarch64-apple-darwin x86_64-apple-darwin
set -euo pipefail

cd "$(dirname "$0")/.."
out="${1:-dist}"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
if [ -z "$version" ]; then
    echo "no version in Cargo.toml" >&2
    exit 1
fi

for target in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release -p hdtreemap-native --target "$target"
done

mkdir -p "$out"
app="$out/HDTreeMap.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create \
    target/aarch64-apple-darwin/release/hdtreemap \
    target/x86_64-apple-darwin/release/hdtreemap \
    -output "$app/Contents/MacOS/hdtreemap"
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
cp assets/icon/hdtreemap.icns "$app/Contents/Resources/hdtreemap.icns"
cp README.md LICENSE "$app/Contents/Resources/"
plutil -lint "$app/Contents/Info.plist"
codesign --force --deep --sign - "$app"

zip="$out/HDTreeMap-$version-macos-universal.zip"
rm -f "$zip"
ditto -c -k --keepParent "$app" "$zip"
lipo -info "$app/Contents/MacOS/hdtreemap"
echo "packaged $zip"
