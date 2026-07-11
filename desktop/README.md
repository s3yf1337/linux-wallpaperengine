# Wallpaper Engine Linux — Tauri 2 desktop app

Desktop shell around the existing gallery UI. Orchestration is Rust (Tauri
commands); the C++ `linux-wallpaperengine` binary stays external.

## Layout

```
desktop/
  ui/index.html          # gallery frontend (invoke, not fetch)
  package.json           # @tauri-apps/cli only
  src-tauri/
    src/
      lib.rs             # commands + AppState
      index.rs           # workshop scan (port of lwe-index.py)
      engine.rs          # spawn/kill engine (port of lwe-launch)
      settings.rs        # ~/.cache/lwe_settings.json
    capabilities/default.json
    tauri.conf.json      # decorations:false, asset protocol scope
  tooling/ (parent)      # legacy Python scripts — still work standalone
```

## Dev / build

```bash
cd ~/AAAcode/wpengine/desktop
cargo tauri dev          # hot-reload backend; frontend is static ui/
cargo tauri build        # release binary + deb if configured
```

Binary (debug): `src-tauri/target/debug/wpengine`  
Requires installed engine: `/opt/linux-wallpaperengine/linux-wallpaperengine`

## Runtime deps

- Hyprland (`hyprctl` for multi-monitor launch)
- Wayland session (`WAYLAND_DISPLAY` auto-probed)
- Steam workshop at `~/.local/share/Steam/steamapps/workshop/content/431960/`
