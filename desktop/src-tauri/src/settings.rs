//! Playback settings — port of daemon DEFAULT_SETTINGS / lwe_settings.json

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackSettings {
    pub volume: u32,
    pub fps: u32,
    pub silent: bool,
    pub scaling: String,
    pub no_fullscreen_pause: bool,
    pub disable_mouse: bool,
    pub noautomute: bool,
}

impl Default for PlaybackSettings {
    fn default() -> Self {
        Self {
            volume: 15,
            fps: 30,
            silent: false,
            scaling: "fill".into(),
            no_fullscreen_pause: false,
            disable_mouse: false,
            noautomute: false,
        }
    }
}

pub fn settings_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(".cache/lwe_settings.json")
}

pub fn load() -> PlaybackSettings {
    let path = settings_path();
    if let Ok(raw) = fs::read_to_string(&path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            let mut s = PlaybackSettings::default();
            if let Some(n) = v.get("volume").and_then(|x| x.as_u64()) {
                s.volume = (n as u32).min(100);
            }
            if let Some(n) = v.get("fps").and_then(|x| x.as_u64()) {
                s.fps = (n as u32).clamp(1, 240);
            }
            if let Some(b) = v.get("silent").and_then(|x| x.as_bool()) {
                s.silent = b;
            }
            if let Some(sc) = v.get("scaling").and_then(|x| x.as_str()) {
                if matches!(sc, "stretch" | "fit" | "fill" | "default") {
                    s.scaling = sc.into();
                }
            }
            if let Some(b) = v.get("no_fullscreen_pause").and_then(|x| x.as_bool()) {
                s.no_fullscreen_pause = b;
            }
            if let Some(b) = v.get("disable_mouse").and_then(|x| x.as_bool()) {
                s.disable_mouse = b;
            }
            if let Some(b) = v.get("noautomute").and_then(|x| x.as_bool()) {
                s.noautomute = b;
            }
            if s.volume == 0 {
                s.silent = true;
            }
            return s;
        }
    }
    PlaybackSettings::default()
}

pub fn save(s: &PlaybackSettings) {
    if let Some(parent) = settings_path().parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string_pretty(s) {
        let _ = fs::write(settings_path(), raw);
    }
}

/// Merge overrides (from frontend launch args) onto base settings.
pub fn merge(base: &PlaybackSettings, patch: &serde_json::Value) -> PlaybackSettings {
    let mut s = base.clone();
    if let Some(n) = patch.get("volume").and_then(|x| x.as_u64()) {
        s.volume = (n as u32).min(100);
    }
    if let Some(n) = patch.get("fps").and_then(|x| x.as_u64()) {
        s.fps = (n as u32).clamp(1, 240);
    }
    if let Some(b) = patch.get("silent").and_then(|x| x.as_bool()) {
        s.silent = b;
    }
    // also accept string "0"/"1" like query params
    if let Some(v) = patch.get("silent").and_then(|x| x.as_str()) {
        s.silent = matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on");
    }
    if let Some(sc) = patch.get("scaling").and_then(|x| x.as_str()) {
        if matches!(sc, "stretch" | "fit" | "fill" | "default") {
            s.scaling = sc.into();
        }
    }
    if let Some(b) = patch.get("no_fullscreen_pause").and_then(|x| x.as_bool()) {
        s.no_fullscreen_pause = b;
    }
    if let Some(v) = patch.get("no_fullscreen_pause").and_then(|x| x.as_str()) {
        s.no_fullscreen_pause = matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on");
    }
    if let Some(b) = patch.get("disable_mouse").and_then(|x| x.as_bool()) {
        s.disable_mouse = b;
    }
    if let Some(v) = patch.get("disable_mouse").and_then(|x| x.as_str()) {
        s.disable_mouse = matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on");
    }
    if let Some(b) = patch.get("noautomute").and_then(|x| x.as_bool()) {
        s.noautomute = b;
    }
    if s.volume == 0 {
        s.silent = true;
    }
    s
}
