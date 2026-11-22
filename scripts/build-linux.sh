#!/usr/bin/env bash
# Build BudsPulse Linux packages (AppImage / .deb / .rpm) inside WSL Ubuntu or any Debian/Ubuntu box.
#   bash scripts/build-linux.sh [output-dir]
# Needs: rustup toolchain, nodejs+npm, libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
#        libxdo-dev libssl-dev libdbus-1-dev patchelf (see README).
set -euo pipefail
SRC="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-$SRC/release/Linux}"
WORK="$HOME/budspulse-build"
. "$HOME/.cargo/env" 2>/dev/null || true

# build on the native Linux filesystem (much faster than /mnt/c and no CRLF/permission issues)
mkdir -p "$WORK"
rsync -a --delete --exclude node_modules --exclude 'src-tauri/target' --exclude 'tools/linux-check/target' --exclude dist --exclude release "$SRC/" "$WORK/"
cd "$WORK"
npm ci --no-audit --no-fund --loglevel=error
export APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=true
# keep the builder's home folder out of the binaries (rustc embeds source paths in panic messages)
CARGO_DIR="${CARGO_HOME:-$HOME/.cargo}"
US=$'\x1f'
export CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$HOME=~${US}--remap-path-prefix=$CARGO_DIR=/cargo${US}--remap-path-prefix=$WORK=/budspulse"
npx tauri build --bundles appimage,deb,rpm

mkdir -p "$OUT"
B=src-tauri/target/release/bundle
cp -f "$B"/appimage/*.AppImage "$B"/deb/*.deb "$B"/rpm/*.rpm "$OUT"/
cp -f src-tauri/target/release/budspulse "$OUT"/budspulse
chmod +x "$OUT"/*.AppImage "$OUT"/budspulse
if grep -aq "$HOME" "$OUT"/budspulse; then echo "error: binary still contains $HOME" >&2; exit 1; fi
ls -la "$OUT"
