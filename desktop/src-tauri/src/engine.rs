//! Wallpaper process control — port of lwe-launch + daemon launch/stop/monitors.

use crate::settings::PlaybackSettings;
use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::{Child, Command};

const ENGINE_BIN: &str = "/opt/linux-wallpaperengine/linux-wallpaperengine";

pub fn assets_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(".local/share/Steam/steamapps/common/wallpaper_engine/assets")
}

/// Resolve WAYLAND_DISPLAY like the Python daemon (probe XDG_RUNTIME_DIR).
pub fn resolve_wayland() -> Option<String> {
    let xdg = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| {
        format!("/run/user/{}", nix::unistd::getuid())
    });
    if let Ok(wd) = env::var("WAYLAND_DISPLAY") {
        let p = Path::new(&xdg).join(&wd);
        if p.exists() {
            return Some(wd);
        }
    }
    // prefer wayland-0, wayland-1, ...
    let mut cands: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&xdg) {
        for ent in rd.flatten() {
            let name = ent.file_name().to_string_lossy().to_string();
            if name.starts_with("wayland-") && !name.contains('.') {
                cands.push(name);
            }
        }
    }
    cands.sort();
    cands.into_iter().next()
}

#[derive(Debug, Clone, Serialize)]
pub struct MonitorInfo {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub id: i64,
}

/// List monitors via hyprctl -j monitors (same as lwe-launch).
pub async fn list_monitors() -> Vec<MonitorInfo> {
    let mut cmd = Command::new("hyprctl");
    cmd.args(["-j", "monitors"]);
    if let Some(wd) = resolve_wayland() {
        cmd.env("WAYLAND_DISPLAY", wd);
    }
    let out = match cmd.output().await {
        Ok(o) if o.status.success() => o.stdout,
        _ => return Vec::new(),
    };
    let v: serde_json::Value = match serde_json::from_slice(&out) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let Some(arr) = v.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|m| {
            Some(MonitorInfo {
                name: m.get("name")?.as_str()?.to_string(),
                width: m.get("width")?.as_u64()? as u32,
                height: m.get("height")?.as_u64()? as u32,
                id: m.get("id")?.as_i64().unwrap_or(0),
            })
        })
        .collect()
}

/// Kill any running engine binary (pkill -x equivalent; name >15 so use pkill -f carefully).
pub async fn kill_engine() {
    // Prefer killing by full path to avoid matching this process / shell.
    let _ = Command::new("pkill")
        .args(["-f", "/opt/linux-wallpaperengine/linux-wallpaperengine"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
}

/// Build argv matching lwe-launch (global flags + per-monitor --screen-root/--bg/--scaling).
/// `properties` are wallpaper-specific overrides → `--set-property key=value`.
pub fn build_args(
    id: &str,
    opts: &PlaybackSettings,
    monitors: &[MonitorInfo],
    properties: &[(String, String)],
) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    args.push("--assets-dir".into());
    args.push(assets_dir().display().to_string());

    args.push("--fps".into());
    args.push(opts.fps.to_string());

    if opts.silent || opts.volume == 0 {
        args.push("--silent".into());
    } else {
        args.push("--volume".into());
        args.push(opts.volume.to_string());
    }
    if opts.no_fullscreen_pause {
        args.push("--no-fullscreen-pause".into());
    }
    if opts.disable_mouse {
        args.push("--disable-mouse".into());
    }
    if opts.noautomute {
        args.push("--noautomute".into());
    }

    for (k, v) in properties {
        if k.is_empty() {
            continue;
        }
        args.push("--set-property".into());
        args.push(format!("{k}={v}"));
    }

    if !monitors.is_empty() {
        for m in monitors {
            args.push("--screen-root".into());
            args.push(m.name.clone());
            args.push("--bg".into());
            args.push(id.to_string());
            // --scaling applies to previous --screen-root/--bg
            if !opts.scaling.is_empty() {
                args.push("--scaling".into());
                args.push(opts.scaling.clone());
            }
        }
    } else {
        if !opts.scaling.is_empty() {
            args.push("--scaling".into());
            args.push(opts.scaling.clone());
        }
        args.push(id.to_string());
    }
    args
}

pub async fn spawn_engine(
    id: &str,
    opts: &PlaybackSettings,
    properties: &[(String, String)],
) -> Result<Child, String> {
    if !Path::new(ENGINE_BIN).is_file() {
        return Err(format!("binary not found: {ENGINE_BIN}"));
    }
    kill_engine().await;
    let monitors = list_monitors().await;
    let args = build_args(id, opts, &monitors, properties);
    eprintln!("[lwe] spawn engine: {ENGINE_BIN} {}", args.join(" "));

    let mut cmd = Command::new(ENGINE_BIN);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(false);

    if let Some(wd) = resolve_wayland() {
        cmd.env("WAYLAND_DISPLAY", wd);
    }
    // inherit rest of env (DISPLAY, XDG_RUNTIME_DIR, etc.)

    cmd.spawn().map_err(|e| format!("spawn failed: {e}"))
}
