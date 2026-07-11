# Wallpaper Engine on Linux — tooling + engine

Complete toolkit to run Steam Wallpaper Engine backgrounds on CachyOS/Hyprland/Wayland,
on top of the [Almamu/linux-wallpaperengine](https://github.com/Almamu/linux-wallpaperengine) fork.

This is a fork of [Almamu/linux-wallpaperengine](https://github.com/Almamu/linux-wallpaperengine)
with added CEF 149 support and a **Rust** tooling layer (no Python).

## Layout

```
linux-wallpaperengine/        # the engine (Almamu fork) + CEF tweaks, no build/ committed
├── patches/
│   └── cef-fixes.patch   # engine tweaks as a single diff (against upstream b016d7d)
├── tooling/              # thin shell helpers + browser gallery shell
│   ├── lwe-launch        # launch wallpaper on all monitors (Hyprland layer-shell)
│   ├── lwe-select        # TUI picker via fzf + jq
│   ├── wallpapers.html   # browser gallery (talks to Rust daemon on :45127)
│   └── lwe-daemon.service
├── desktop/              # Tauri 2 GUI + Rust backend (index, engine, Steam UGC, HTTP daemon bins)
│   ├── ui/index.html
│   └── src-tauri/
│       ├── src/index.rs          # workshop indexer (was lwe-index.py)
│       ├── src/http_daemon.rs    # HTTP control API (was lwe-daemon.py)
│       ├── src/engine.rs         # spawn/kill engine, monitors
│       ├── src/steam.rs          # Steam Workshop UGC
│       └── src/bin/
│           ├── lwe_daemon.rs     # cargo bin → lwe-daemon
│           └── lwe_index.rs      # cargo bin → lwe-index
├── Makefile
└── README.md
```

## What the engine tweaks fix (patches/cef-fixes.patch)

Three bugs that broke web wallpapers on recent kernel/Mesa:

1. **CEF 135 → 149** (`CMakeLists.txt`) — the old CEF couldn't start the GPU process
   (ANGLE-Vulkan error -3 on Linux 6.12+).
2. **ICU data crash** (`WebBrowserContext.cpp`) — CEF 149 split `icudtl.dat` and
   `libcef.so` into separate folders; we set `resources_dir_path`/`locales_dir_path` explicitly
   + install to `/opt` where everything lives in one directory.
3. **Black desktop** (`RenderHandler.cpp`) — `texture()` returned an FBO id instead of
   a texture id, so CEF wrote pixels into the wrong GL object. One-line fix.

(`BrowserClient.*`, `WPSchemeHandler.cpp` — diagnostic LoadHandler/DisplayHandler.)

## Installing the tooling

```bash
make install          # cargo build --release lwe-daemon + lwe-index,
                      # install to ~/.local/bin, gallery to ~/wallpapers.html,
                      # systemd --user unit, enable daemon
make uninstall        # removes installed scripts/unit (keeps gallery + cache)
make build-tooling    # just cargo build the Rust bins
make status
```

Requires: `cargo`, `jq` (for `lwe-select`), `fzf` (for `lwe-select`), Hyprland/`hyprctl` for multi-monitor launch.

## Building the engine from source

```bash
make build-engine     # cmake + make in ./build (downloads CEF ~300M),
                      # then sudo make install into /opt/linux-wallpaperengine
```

Build notes: see `patches/cef-fixes.patch` and commit history.

## Usage

- **Desktop GUI (preferred):** `cd desktop && npm run tauri dev`
- **Browser gallery:** `firefox file://~/wallpapers.html` (needs `lwe-daemon` on `:45127`)
- **TUI:** `lwe-select`
- **Direct:** `lwe-launch <workshop_id>`
- **Index only:** `lwe-index --rescan` / `--rebuild` / `--list` / `--json`

New Steam workshop downloads are picked up automatically (daemon auto-rescan every 45s).

## How it works

Steam downloads wallpapers to `~/.local/share/Steam/steamapps/workshop/content/431960/<id>/`.
`lwe-index` (Rust) parses each folder's `project.json` → `~/.cache/lwe_index.json`.
`lwe-daemon` (Rust HTTP on `127.0.0.1:45127`) serves `/list /rescan /launch /stop /settings …`
to the browser gallery. The Tauri app uses the **same Rust modules** via `invoke` (and can
embed the HTTP server if the systemd unit is not already bound to the port).

## Desktop app (Tauri 2)

Native GUI on **Tauri 2** (Rust backend + webview frontend) with Steam Workshop integration.

### Features
- **Library browser** — grid of installed wallpapers, search, type filters, **NEW** badge
- **Favorites** — star any wallpaper; persisted in `localStorage`
- **Steam Workshop** — subscribe/unsubscribe; unsubscribe purges the local folder (`remove_local`)
- **Launch** — applies wallpaper on all monitors (Hyprland layer-shell)
- **Rescan** — re-index workshop (same code path as `lwe-index --rescan`)

### Build & run
```bash
cd desktop
npm install
npm run tauri dev        # dev build + window
npm run tauri build      # deb under src-tauri/target/release/bundle/
```

> `libsteam_api.so` is required at build time (git-ignored `*.so`; staged by `build.rs`).
> Steam client must be running for Workshop UGC calls.

## No Python

The old Python daemon/index (`lwe-daemon.py`, `lwe-index.py`) were removed.
Everything is Rust (`desktop/src-tauri`) + thin bash helpers (`lwe-launch`, `lwe-select`).
If you still have `~/.local/bin/lwe-*.py`, `make install` / `make uninstall` drops them.
