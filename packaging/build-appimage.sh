#!/usr/bin/env bash
# Build the Mia Agent AppImage (x86_64) from the release binary.
# Usage (from the repo root):
#   ./packaging/build-appimage.sh [version]
# Output: Mia_Agent-<version>-x86_64.AppImage in the repo root.
#
# The binary links only libc/libm/libgcc_s, but at runtime it dlopens
# the graphics stack (Vulkan, X11/Wayland, xkbcommon) — the host needs
# mesa-vulkan-drivers + libxkbcommon installed, same as the .rpm
# Requires. Nothing else is bundled: the AppImage is just the binary,
# its .desktop file and its icon.
set -euo pipefail

VERSION="${1:-0.1.0}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# MIA_BIN lets CI point at a cross-target binary, e.g.
# target/x86_64-unknown-linux-gnu/release/mia.
BIN="${MIA_BIN:-$ROOT/target/release/mia}"
APPDIR="$ROOT/Mia.AppDir"
APPIMAGETOOL="$ROOT/.appimagetool/appimagetool-x86_64.AppImage"

if [ ! -x "$BIN" ]; then
    echo "Manca il binario release: prima 'cargo build --release -p mia-ui --bin mia'" >&2
    exit 1
fi

# appimagetool: download once (it's itself an AppImage; no FUSE needed,
# we run it with --appimage-extract-and-run).
if [ ! -x "$APPIMAGETOOL" ]; then
    mkdir -p "$(dirname "$APPIMAGETOOL")"
    echo "Scarico appimagetool..."
    curl -fsSL -o "$APPIMAGETOOL" \
        https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
    chmod +x "$APPIMAGETOOL"
fi

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" \
    "$APPDIR/usr/share/icons/hicolor/128x128/apps"
cp "$BIN" "$APPDIR/usr/bin/mia"
cp "$ROOT/packaging/mia.desktop" "$APPDIR/usr/share/applications/mia.desktop"
cp "$ROOT/packaging/mia.desktop" "$APPDIR/mia.desktop"
cp "$ROOT/packaging/mia.svg" "$APPDIR/usr/share/icons/hicolor/128x128/apps/mia.svg"
cp "$ROOT/packaging/mia.svg" "$APPDIR/mia.svg"
ln -sf usr/bin/mia "$APPDIR/AppRun"
# The workspace files are mode 0660/0770; AppImages run as other users,
# so everything in the AppDir must be world-readable (and +x for dirs/bin).
chmod -R a+rX "$APPDIR"

ARCH=x86_64 "$APPIMAGETOOL" --appimage-extract-and-run "$APPDIR" \
    "$ROOT/Mia_Agent-${VERSION}-x86_64.AppImage"

echo "AppImage pronta: $ROOT/Mia_Agent-${VERSION}-x86_64.AppImage"
ls -la "$ROOT/Mia_Agent-${VERSION}-x86_64.AppImage"
