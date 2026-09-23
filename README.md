<img src="desktop/src-tauri/icons/128x128@2x.png" alt="Quintodrome logo" align="right" height="96" />

# Quintodrome

A [Tauri](https://tauri.app) desktop app that wraps
[Navidrome](https://www.navidrome.org) — your self-hosted music server — into a
native macOS/Windows application.

<img width="1179" height="904" alt="Screenshot 2026-09-23 at 1 13 43 AM" src="https://github.com/user-attachments/assets/228be4d4-7ef8-45eb-8b41-7cbde60dd484" />

The server is bundled as a sidecar binary and managed automatically, so there's
nothing to install, configure, or keep running: launch the app and it starts
(and later shuts down) its own Navidrome instance for you.

> This repository is a fork of [Navidrome](https://github.com/navidrome/navidrome).
> Everything outside `desktop/` is the upstream server + web UI; `desktop/` is
> the Quintodrome wrapper around it.

## What it does

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

Batteries included:

- **Auto-provisioning** — on a fresh install the app creates the initial admin
  user (`admin` / `admin`) and logs in as it automatically, skipping both the
  "create an admin user" and login screens.
- **Opted-out telemetry** — Navidrome's insights collector is disabled by
  default.
- **Masked address bar** — the internal `127.0.0.1:4533` origin is shown as
  `http://quintodrome`.
- **External links** (Last.fm, artist pages, …) open in your default browser.
- **Native editing shortcuts** — Cmd+X/C/V/A/Z work in the web UI.
- **Swipe navigation** — two-finger swipe back/forward on macOS.

## Quick start

```sh
# one-time: Node >= 24 (see .nvmrc), Go >= 1.26, Rust
just run
```

`just run` builds the Navidrome sidecar and launches the app. No `just`? See
the [desktop README](desktop/README.md) for the plain commands, or install it
with `brew install just`.

To wipe Navidrome's persistent state (database, cache) and start fresh:

```sh
just clean-state
```

## Building a distributable

```sh
just bundle          # or: ./desktop/scripts/build.sh
```

This produces the platform's packaged app under
`desktop/src-tauri/target/release/bundle/` (e.g. `macos/Quintodrome.app` on
macOS). Cross-platform binaries are built in CI — see
[`.github/workflows/desktop.yml`](.github/workflows/desktop.yml), which produces
`.app`/`.dmg` on macOS and `.exe`/`.msi` on Windows for tagged releases.

> Artifacts are unsigned (no Apple/Windows code-signing certs), so users will
> see Gatekeeper/SmartScreen prompts.

## Configuration

Environment variables override the defaults:

| Variable                    | Default                        | Purpose                                  |
| --------------------------- | ------------------------------ | ---------------------------------------- |
| `QUINTODROME_HOST`          | `127.0.0.1`                    | Address the webview connects to          |
| `QUINTODROME_PORT`          | `4533`                         | Port for the Navidrome server            |
| `QUINTODROME_MUSIC_FOLDER`  | OS music dir (e.g. `~/Music`)  | Where your music lives                   |
| `QUINTODROME_ADMIN_PASSWORD`| `admin`                        | Password for the auto-created admin user (fresh installs only) |
| `QUINTODROME_PUBLIC_URL`    | `http://quintodrome`           | Friendly origin shown in the address bar |

The data folder (database, cache) is always the OS app-data directory
(e.g. `~/Library/Application Support/com.quintodrome.desktop` on macOS).

## Repository layout

```
.
├── desktop/          # Quintodrome: the Tauri wrapper (Rust + toolbar HTML)
│   ├── src-tauri/    #   sidecar spawn/health-check, lifecycle, menu, clipboard
│   ├── src/          #   toolbar.html (nav bar) + index.html (splash)
│   └── scripts/      #   build.sh, icon generator
├── ui/               # Navidrome web UI (React)
├── server/           # Navidrome HTTP server
├── core/             # Navidrome core (scanning, transcoding, …)
├── cmd/              # Navidrome entrypoint / CLI
└── Justfile          # run / bundle / sidecar / clean-state
```

For the details of building and running the desktop app directly, see
[`desktop/README.md`](desktop/README.md).

## Development

Prerequisites:

- Rust (stable)
- Node.js ≥ 24 (pinned in `.nvmrc`)
- Go ≥ 1.26 (for building the Navidrome binary)

The Navidrome server is built with `make build`; the wrapper stages it as
`desktop/src-tauri/binaries/navidrome-<target-triple>` and bundles it as a Tauri
sidecar. See [`desktop/scripts/build.sh`](desktop/scripts/build.sh).

## Acknowledgements

Quintodrome is built on [Navidrome](https://github.com/navidrome/navidrome),
an open-source web-based music collection server and streamer. All credit for
the music server and web UI goes to the Navidrome project.
