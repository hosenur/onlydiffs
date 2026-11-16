#!/usr/bin/env bash
# Builds the native app as a macOS bundle plus the archive the updater
# downloads:
#
#   target/release/bundle/onlydiffs-native.app
#   target/release/bundle/onlydiffs-native-<arch>.app.tar.gz
#
# With TAURI_SIGNING_PRIVATE_KEY (and _PASSWORD) set, it also writes the
# archive's `.sig`, using the Tauri CLI from apps/desktop. Without a `.sig`
# no installed copy will take the update.
set -euo pipefail

here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"

version=$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)
arch=$(uname -m)
[ "$arch" = arm64 ] && arch=aarch64

cargo build --release

out=target/release/bundle
app="$out/onlydiffs-native.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/onlydiffs "$app/Contents/MacOS/onlydiffs"
cp ../desktop/src-tauri/icons/icon.icns "$app/Contents/Resources/icon.icns"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>onlydiffs</string>
  <key>CFBundleDisplayName</key><string>onlydiffs</string>
  <key>CFBundleExecutable</key><string>onlydiffs</string>
  <key>CFBundleIdentifier</key><string>dev.hosenur.onlydiffs.native</string>
  <key>CFBundleIconFile</key><string>icon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

# An ad-hoc seal over the whole bundle. It proves nothing about who built it
# (the minisign signature does that) but the updater's `codesign --verify`
# catches a damaged copy with it.
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"

reported=$("$app/Contents/MacOS/onlydiffs" --version)
[ "$reported" = "$version" ] || { echo "the bundle reports $reported, Cargo.toml says $version" >&2; exit 1; }

archive="$out/onlydiffs-native-$arch.app.tar.gz"
rm -f "$archive" "$archive.sig"
tar -czf "$archive" -C "$out" onlydiffs-native.app

if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
  # Same key and same CLI as the Tauri app's updater artifacts.
  (cd ../desktop && bun run tauri signer sign "$here/$archive")
  test -s "$archive.sig"
fi

echo "$app"
echo "$archive"
[ -f "$archive.sig" ] && echo "$archive.sig" || echo "(unsigned: set TAURI_SIGNING_PRIVATE_KEY to sign)"
