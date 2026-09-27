# Quintodrome Desktop

A [Tauri](https://tauri.app) desktop wrapper that hosts [Navidrome](https://www.navidrome.org)
as its own web app.

On launch the app:

1. Checks whether a Navidrome server is already listening on `127.0.0.1:4533`
   (via its `/ping` health endpoint). If so, it reuses it as-is.
2. Otherwise it spawns the bundled `navidrome` binary as a **separate sidecar
   process**, pointed at the OS app-data/music directories, and waits for it to
   become ready.
3. Loads the Navidrome UI in the content webview, below a thin toolbar with
   back/forward/reload buttons and an address bar.
4. On exit, sends `SIGTERM` to the spawned server so it shuts down gracefully
   (falls back to a hard kill after 3s).

On a **fresh install** (no existing Navidrome data) the app also auto-creates
the initial admin user (`admin` / `admin`) behind the scenes, and auto-logs-in
as that user on startup — you skip both Navidrome's "create an admin user"
first-run screen and the login page. Change the password in Navidrome settings
afterwards; override it via `QUINTODROME_ADMIN_PASSWORD`.

## How to launch

From the repo root:

```sh
# 0. one-time: make sure Node >= 24 is active (already pinned in .nvmrc)
nodenv local 24.15.0        # or `nvm use` / ensure `node --version` is v24+

# 1. build the Navidrome server + stage it as the sidecar
./desktop/scripts/build.sh --skip-tauri

# 2. launch the app
cd desktop/src-tauri && cargo run
```

A window opens on the splash screen, the app starts Navidrome (or reuses a
running one), then loads the UI. Navidrome is reachable at
<http://127.0.0.1:4533> for the first-run admin setup.

To build a standalone `.app` instead:

```sh
./desktop/scripts/build.sh
# → desktop/src-tauri/target/release/bundle/macos/Quintodrome.app
```

## Layout

```
desktop/
├── src/                 # toolbar.html (nav bar) + index.html (loading splash)
├── src-tauri/
│   ├── src/             # Rust: sidecar spawn, health-check, lifecycle, toolbar
│   ├── binaries/        # navidrome-<target-triple> sidecar (built, not committed)
│   ├── icons/           # App icons
│   ├── tauri.conf.json
│   └── Cargo.toml
└── scripts/
    ├── build.sh         # Build navidrome + stage sidecar + tauri build
    └── gen_icon.go      # Regenerates the source icon (optional)
```

## Prerequisites

- Rust (stable) with the `aarch64-apple-darwin` / `x86_64-apple-darwin` (or
  matching host) target.
- Node.js >= 24 (the repo already pins this in `.nvmrc`; via `nodenv` you can
  run `nodenv local 24.15.0`).
- Go >= 1.26 (for building the Navidrome binary).

## Build

```sh
./desktop/scripts/build.sh
```

This builds the Navidrome server (`make build`), stages it under
`src-tauri/binaries/navidrome-<target-triple>` (with the executable bit set),
then runs `npx tauri build`. The packaged app lands in
`desktop/src-tauri/target/release/bundle/`.

To build the sidecar without running the full Tauri bundle:

```sh
./desktop/scripts/build.sh --skip-tauri
```

## Run (development)

```sh
# 1. build + stage the sidecar binary
./desktop/scripts/build.sh --skip-tauri

# 2. run the app
cd desktop/src-tauri && cargo run
```

The Rust `build.rs` copies the staged sidecar next to the dev binary
(`target/debug/navidrome`) automatically, so plain `cargo run` works — no
`tauri dev` CLI needed.

## Configuration

Environment variables override the defaults:

| Variable                    | Default                  | Purpose                         |
| --------------------------- | ------------------------ | ------------------------------- |
| `QUINTODROME_HOST`          | `127.0.0.1`              | Address the webview connects to |
| `QUINTODROME_PORT`          | `4533`                   | Port for the Navidrome server   |
| `QUINTODROME_MUSIC_FOLDER`  | OS music dir (e.g. `~/Music`) | Where your music lives      |
| `QUINTODROME_ADMIN_PASSWORD` | `admin`                 | Password for the auto-created admin user (fresh installs only) |
| `QUINTODROME_PUBLIC_URL`    | `http://quintodrome`     | Friendly origin shown in the address bar |

The data folder (DB, cache) is always the OS app-data directory
(e.g. `~/Library/Application Support/com.quineglobal.drome` on macOS).

> Note: if a Navidrome server is already running on the configured host/port,
> the app simply loads it and does **not** start a second instance.
