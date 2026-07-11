# Wallpaper Engine on Linux — tooling + engine

Complete toolkit to run Steam Wallpaper Engine backgrounds on CachyOS/Hyprland/Wayland,
on top of the [Almamu/linux-wallpaperengine](https://github.com/Almamu/linux-wallpaperengine) fork.

This is a fork of [Almamu/linux-wallpaperengine](https://github.com/Almamu/linux-wallpaperengine)
with added CEF 149 support and a tooling layer for Steam Wallpaper Engine on CachyOS/Hyprland/Wayland.

## Layout

```
linux-wallpaperengine/        # the engine (Almamu fork) + my CEF tweaks, no build/ committed
├── patches/
│   └── cef-fixes.patch   # my engine tweaks as a single diff (against upstream b016d7d)
├── tooling/         # my layer on top of the engine
│   ├── lwe-index.py       # workshop indexer + gallery generator (incremental rescan)
│   ├── lwe-daemon.py      # HTTP daemon (127.0.0.1:45127): /list /rescan /launch /stop
│   ├── lwe-launch         # launch wallpapers on all monitors (Hyprland layer-shell)
│   ├── lwe-select         # TUI picker via fzf
│   ├── wallpapers.html    # web gallery (pulls data from the daemon)
│   └── lwe-daemon.service # systemd --user unit
├── desktop/        # Tauri 2 shell (ui/index.html + src-tauri Rust)
├── Makefile         # install / uninstall / build-engine / patch-engine
└── README.md
```

## What my engine tweaks fix (patches/cef-fixes.patch)

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
make install          # copies scripts to ~/.local/bin, the gallery to ~,
                      # the unit to ~/.config/systemd/user, and enables the daemon
make uninstall        # removes everything installed
```

## Building the engine from source

```bash
make build-engine     # cmake + make in ./build (downloads CEF ~300M),
                      # then sudo make install into /opt/linux-wallpaperengine
```

Build notes and known gotchas are kept in this README and the commit history
(`patches/cef-fixes.patch` documents the CEF 149 changes).

## Usage

- Web gallery: `firefox file://~/wallpapers.html` (or `tooling/wallpapers.html`)
  — search, filter by type, a Rescan button, and a NEW badge on newly added wallpapers.
- TUI: `lwe-select`
- Directly: `lwe-launch <workshop_id>`
- New wallpapers from Steam are picked up automatically (the daemon re-scans the workshop every 45s).

## How it works

Steam downloads wallpapers to `~/.local/share/Steam/steamapps/workshop/content/431960/<id>/`.
`lwe-index.py` parses each folder's `project.json` → `~/.cache/lwe_index.json`.
The daemon serves this index to the gallery and runs `lwe-launch` when you press Apply.

## Desktop app (Tauri 2)

```
desktop/                 # Tauri 2 shell
  ui/index.html          # gallery (invoke)
  src-tauri/             # Rust orchestration (ex-Python)
```

```bash
cd desktop && cargo tauri dev
# or
./desktop/src-tauri/target/debug/wpengine
```

Python tooling under `tooling/` still works standalone (daemon on :45127).
