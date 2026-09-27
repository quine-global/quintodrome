# Quintodrome desktop app

# Run the desktop app (builds the Navidrome sidecar, then `cargo run`).
# Requires Go >= 1.26 and Node >= 24 (see .nvmrc) for the sidecar build.
run:
    ./desktop/scripts/build.sh --skip-tauri
    cd desktop/src-tauri && cargo run

# Build the packaged .app (desktop/src-tauri/target/release/bundle/).
bundle:
    ./desktop/scripts/build.sh

# Build only the Navidrome sidecar binary.
sidecar:
    ./desktop/scripts/build.sh --skip-tauri

# Remove Navidrome's persistent state (database, cache, backups) for a clean
# first-run. Your music files are NOT touched. Quit the app first.
clean-state:
    #!/usr/bin/env bash
    set -euo pipefail
    case "$(uname -s)" in
      Darwin) dir="$HOME/Library/Application Support/com.quineglobal.drome" ;;
      Linux)  dir="${XDG_DATA_HOME:-$HOME/.local/share}/com.quineglobal.drome" ;;
      MINGW*|MSYS*|CYGWIN*) dir="${APPDATA:-$HOME/AppData/Roaming}/com.quineglobal.drome" ;;
      *) echo "unsupported OS" >&2; exit 1 ;;
    esac
    echo "Removing: $dir"
    rm -rf "$dir"
    echo "Done. Next launch starts fresh (first-run setup)."
