#!/usr/bin/env bash
# Build a macOS Reach.app (and zip) for local distribution.
#
# Usage (from repo root):
#   ./scripts/package-reach.sh
#   ./scripts/package-reach.sh --skip-build    # reuse existing release binary
#   ./scripts/package-reach.sh --rebuild-icon # regenerate packaging/AppIcon.icns
#
# Output:
#   dist/Reach.app
#   dist/Reach-macos.zip
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

SKIP_BUILD=0
REBUILD_ICON=0
for arg in "$@"; do
  case "$arg" in
    --skip-build) SKIP_BUILD=1 ;;
    --rebuild-icon) REBUILD_ICON=1 ;;
    -h|--help)
      echo "Usage: $0 [--skip-build] [--rebuild-icon]"
      exit 0
      ;;
    *)
      echo "Unknown arg: $arg" >&2
      exit 1
      ;;
  esac
done

DIST="$ROOT/dist"
APP="$DIST/Reach.app"
CONTENTS="$APP/Contents"
MACOS="$CONTENTS/MacOS"
RESOURCES="$CONTENTS/Resources"
PKG="$ROOT/games/reach/packaging"
ICON_ICNS="$PKG/AppIcon.icns"
ICON_SRC="$PKG/AppIcon.png"

TARGET_DIR="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import sys,json; print(json.load(sys.stdin)["target_directory"])')"
BIN_SRC="$TARGET_DIR/release/reach"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This script builds a macOS .app bundle. On other OSes, run:" >&2
  echo "  cargo build -p reach --release" >&2
  echo "  then ship the binary next to games/reach/levels and games/reach/assets" >&2
  exit 1
fi

if [[ "$SKIP_BUILD" -eq 0 ]]; then
  echo "==> cargo build -p reach --release"
  cargo build -p reach --release
fi

if [[ ! -x "$BIN_SRC" ]]; then
  echo "Missing release binary at $BIN_SRC — run without --skip-build" >&2
  exit 1
fi

if [[ "$REBUILD_ICON" -eq 1 ]]; then
  if [[ ! -f "$ICON_SRC" ]]; then
    echo "Missing icon source: $ICON_SRC" >&2
    exit 1
  fi
  echo "==> rebuilding $ICON_ICNS"
  ICONSET="$(mktemp -d -t ReachIcon)/AppIcon.iconset"
  mkdir -p "$ICONSET"
  AT="$(printf '@')"
  make_icon() {
    local size="$1"
    local name="$2"
    local tmp
    tmp="$(mktemp -t reach-icon).png"
    sips -z "$size" "$size" "$ICON_SRC" --out "$tmp" >/dev/null
    mv "$tmp" "$ICONSET/$name"
  }
  make_icon 16 icon_16x16.png
  make_icon 32 "icon_16x16${AT}2x.png"
  make_icon 32 icon_32x32.png
  make_icon 64 "icon_32x32${AT}2x.png"
  make_icon 128 icon_128x128.png
  make_icon 256 "icon_128x128${AT}2x.png"
  make_icon 256 icon_256x256.png
  make_icon 512 "icon_256x256${AT}2x.png"
  make_icon 512 icon_512x512.png
  make_icon 1024 "icon_512x512${AT}2x.png"
  iconutil -c icns "$ICONSET" -o "$ICON_ICNS"
  rm -rf "$(dirname "$ICONSET")"
fi

if [[ ! -f "$ICON_ICNS" ]]; then
  echo "Missing $ICON_ICNS — add it or pass --rebuild-icon" >&2
  exit 1
fi

echo "==> assembling $APP"
rm -rf "$APP"
mkdir -p "$MACOS" "$RESOURCES"

cp "$BIN_SRC" "$MACOS/reach"
chmod +x "$MACOS/reach"
cp -R "$ROOT/games/reach/levels" "$RESOURCES/levels"
cp -R "$ROOT/games/reach/assets" "$RESOURCES/assets"
cp "$ICON_ICNS" "$RESOURCES/AppIcon.icns"

cat > "$CONTENTS/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>reach</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleIdentifier</key>
  <string>dev.kerabit.reach</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>Reach</string>
  <key>CFBundleDisplayName</key>
  <string>Reach</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>0.1.0</string>
  <key>LSMinimumSystemVersion</key>
  <string>11.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

ZIP="$DIST/Reach-macos.zip"
echo "==> zipping $ZIP"
rm -f "$ZIP"
(
  cd "$DIST"
  ditto -c -k --keepParent "Reach.app" "Reach-macos.zip"
)

echo ""
echo "Done."
echo "  App:  $APP"
echo "  Zip:  $ZIP"
echo "Open with: open \"$APP\""
echo "Or double-click Reach.app after unzipping Reach-macos.zip."
