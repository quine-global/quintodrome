#!/usr/bin/env bash
# Build the Navidrome sidecar binary and package the Quintodrome desktop app.
#
#   Usage: ./desktop/scripts/build.sh [--skip-tauri]
#
# The script:
#   1. Builds the Navidrome server for the host platform (via `make build`).
#   2. Copies it into src-tauri/binaries/ under the target-triple name that
#      tauri-plugin-shell expects for a sidecar (e.g. navidrome-aarch64-apple-darwin).
#   3. Builds the Tauri application (unless --skip-tauri is passed).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DESKTOP="$ROOT/desktop"
SRC_TAURI="$DESKTOP/src-tauri"

SKIP_TAURI=false
if [[ "${1:-}" == "--skip-tauri" ]]; then
  SKIP_TAURI=true
fi

echo "==> Building Navidrome server binary..."
(cd "$ROOT" && make build)

TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
BIN_NAME="navidrome-$TRIPLE"
case "$TRIPLE" in
  *windows*) BIN_NAME="$BIN_NAME.exe" ;;
esac

echo "==> Staging sidecar as binaries/$BIN_NAME"
mkdir -p "$SRC_TAURI/binaries"
cp "$ROOT/navidrome" "$SRC_TAURI/binaries/$BIN_NAME"
chmod +x "$SRC_TAURI/binaries/$BIN_NAME"

if [[ "$SKIP_TAURI" == true ]]; then
  echo "==> Skipping Tauri build (--skip-tauri)."
  exit 0
fi

if [[ "$(uname -s)" == Darwin ]]; then
  # Sparkle, the updater the macOS app links against and embeds.
  echo "==> Fetching Sparkle..."
  bash "$DESKTOP/scripts/download-sparkle.sh" "$SRC_TAURI"
  export SPARKLE_FRAMEWORK_PATH="$SRC_TAURI"
fi

echo "==> Building Quintodrome Tauri app..."
(cd "$DESKTOP" && npx --yes @tauri-apps/cli@2 build)

echo "==> Done."
