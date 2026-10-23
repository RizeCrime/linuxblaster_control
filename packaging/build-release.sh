#!/usr/bin/env bash
# Builds the release packages for the current machine's architecture into dist/:
#
#   linuxblaster-control-<version>-<arch>.tar.gz    any distro (with install.sh)
#   linuxblaster-control_<version>-1_<arch>.deb     Debian, Ubuntu, Mint, Pop!_OS, ...
#   linuxblaster-control-<version>-1.<arch>.rpm     Fedora, openSUSE, ...
#   Linuxblaster_Control-<version>-<arch>.AppImage  any distro, no installation
#
# Used by .github/workflows/release.yml, but runs anywhere with:
#   - a Rust toolchain, cargo-deb and cargo-generate-rpm
#   - the build dependencies: pkg-config, libudev-dev, libasound2-dev
#   - curl and file (for the AppImage tooling)
#
# Usage: packaging/build-release.sh [version]
# The version defaults to the one in Cargo.toml. A leading "v" is ignored,
# so a release tag like "v2.3.0" can be passed directly.

set -euo pipefail

cd "$(dirname "$0")/.."

ARCH=$(uname -m)
VERSION=${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)}
VERSION=${VERSION#v}
DIST=dist

echo "==> Building Linuxblaster Control $VERSION for $ARCH"

# Packages take their version from Cargo.toml, so keep it in sync with the tag.
sed -i "0,/^version = \".*\"/s//version = \"$VERSION\"/" Cargo.toml

cargo build --release
BINARY=target/release/linuxblaster_control
strip "$BINARY"

rm -rf "$DIST"
mkdir -p "$DIST"

# ─── tar.gz ──────────────────────────────────────────────────────────────────

echo "==> tar.gz"
NAME="linuxblaster-control-$VERSION-$ARCH"
STAGE=$(mktemp -d)
mkdir "$STAGE/$NAME"
cp "$BINARY" \
    packaging/install.sh \
    packaging/linuxblaster-control.desktop \
    packaging/linuxblaster-control.png \
    packaging/linuxblaster-control.svg \
    packaging/70-linuxblaster-control.rules \
    README.md LICENSE \
    "$STAGE/$NAME/"
tar -C "$STAGE" -czf "$DIST/$NAME.tar.gz" "$NAME"
rm -rf "$STAGE"

# ─── .deb ────────────────────────────────────────────────────────────────────

echo "==> .deb"
cargo deb --no-build --no-strip --output "$DIST/"

# ─── .rpm ────────────────────────────────────────────────────────────────────

echo "==> .rpm"
cargo generate-rpm --output "$DIST/"

# ─── AppImage ────────────────────────────────────────────────────────────────

echo "==> AppImage"
TOOLS=target/appimage-tools
APPIMAGETOOL="$TOOLS/appimagetool-$ARCH.AppImage"
if [ ! -x "$APPIMAGETOOL" ]; then
    mkdir -p "$TOOLS"
    curl -fsSL -o "$APPIMAGETOOL" \
        "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"
    chmod +x "$APPIMAGETOOL"
fi

APPDIR=$(mktemp -d)/AppDir
install -Dm755 "$BINARY" "$APPDIR/usr/bin/linuxblaster_control"
install -Dm644 packaging/linuxblaster-control.desktop "$APPDIR/usr/share/applications/linuxblaster-control.desktop"
install -Dm644 packaging/linuxblaster-control.png "$APPDIR/usr/share/icons/hicolor/256x256/apps/linuxblaster-control.png"
cp packaging/linuxblaster-control.desktop packaging/linuxblaster-control.png "$APPDIR/"
ln -s usr/bin/linuxblaster_control "$APPDIR/AppRun"

# The binary only links glibc, libudev and libasound, which every desktop
# distro ships; the GUI libraries are loaded at runtime from the system.
# Bundling them would break more than it fixes, so nothing else goes in.
# (APPIMAGE_EXTRACT_AND_RUN: works in containers/CI without FUSE)
APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$ARCH" VERSION="$VERSION" \
    "$APPIMAGETOOL" --no-appstream "$APPDIR" \
    "$DIST/Linuxblaster_Control-$VERSION-$ARCH.AppImage"
rm -rf "$(dirname "$APPDIR")"

echo "==> Done:"
ls -l "$DIST"
