pub mod engine;
pub mod http_daemon;
pub mod index;
pub mod settings;
pub mod state;
mod steam;

use index::{RescanResult, Wallpaper};
use state::SharedState;
use steam::SteamState;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};

/// Tauri-managed app state (same core as the headless HTTP daemon).
pub struct AppState {
    pub inner: Arc<SharedState>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(SharedState::new()),
        }
    }
}

#[tauri::command]
fn list_wallpapers(state: State<'_, AppState>) -> Vec<Wallpaper> {
    let guard = state.inner.items.lock().unwrap();
    eprintln!("[lwe] list_wallpapers called, in-memory={}", guard.len());
    if !guard.is_empty() {
        return guard.clone();
    }
    drop(guard);
    let items = index::load_cache();
    eprintln!("[lwe] list_wallpapers cache fallback={}", items.len());
    let mut g = state.inner.items.lock().unwrap();
    *g = items.clone();
    items
}

#[tauri::command]
async fn rescan(state: State<'_, AppState>) -> Result<RescanResult, String> {
    let prev = {
        let g = state.inner.items.lock().map_err(|e| e.to_string())?;
        if g.is_empty() {
            None
        } else {
            Some(g.clone())
        }
    };
    let result = tokio::task::spawn_blocking(move || index::rescan(prev))
        .await
        .map_err(|e| format!("rescan join error: {e}"))?;
    if result.ok {
        let mut g = state.inner.items.lock().map_err(|e| e.to_string())?;
        *g = result.items.clone();
    }
    Ok(result)
}

#[tauri::command]
async fn launch(
    id: String,
    opts: Option<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    if id.trim().is_empty() {
        return Err("missing id".into());
    }
    let base = state.inner.settings.lock().unwrap().clone();
    let merged = if let Some(patch) = opts {
        settings::merge(&base, &patch)
    } else {
        base
    };
    {
        let mut g = state.inner.settings.lock().unwrap();
        *g = merged.clone();
    }
    settings::save(&merged);

    {
        let mut ch = state.inner.child.lock().await;
        if let Some(mut c) = ch.take() {
            let _ = c.kill().await;
        }
    }

    let child = engine::spawn_engine(&id, &merged).await?;
    {
        let mut ch = state.inner.child.lock().await;
        *ch = Some(child);
    }
    {
        let mut cur = state.inner.current_id.lock().unwrap();
        *cur = Some(id.clone());
    }

    Ok(serde_json::json!({
        "ok": true,
        "id": id,
        "opts": merged,
    }))
}

#[tauri::command]
async fn stop(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    {
        let mut ch = state.inner.child.lock().await;
        if let Some(mut c) = ch.take() {
            let _ = c.kill().await;
        }
    }
    engine::kill_engine().await;
    {
        let mut cur = state.inner.current_id.lock().unwrap();
        *cur = None;
    }
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
fn current(state: State<'_, AppState>) -> serde_json::Value {
    let id = state.inner.current_id.lock().unwrap().clone();
    let opts = state.inner.settings.lock().unwrap().clone();
    serde_json::json!({ "id": id, "opts": opts })
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> serde_json::Value {
    let s = state.inner.settings.lock().unwrap().clone();
    serde_json::json!({ "ok": true, "settings": s })
}

#[tauri::command]
fn set_settings(
    opts: serde_json::Value,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let base = state.inner.settings.lock().unwrap().clone();
    let merged = settings::merge(&base, &opts);
    {
        let mut g = state.inner.settings.lock().unwrap();
        *g = merged.clone();
    }
    settings::save(&merged);
    Ok(serde_json::json!({ "ok": true, "settings": merged }))
}

#[tauri::command]
async fn open_folder(id: String, state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let path = {
        let items = state.inner.items.lock().unwrap();
        items
            .iter()
            .find(|w| w.id == id)
            .map(|w| w.dir.clone())
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_default()
                    .join(format!(
                        ".local/share/Steam/steamapps/workshop/content/431960/{id}"
                    ))
                    .display()
                    .to_string()
            })
    };
    if !std::path::Path::new(&path).is_dir() {
        return Ok(serde_json::json!({
            "ok": false,
            "error": "folder not found",
            "path": path
        }));
    }
    let status = tokio::process::Command::new("xdg-open")
        .arg(&path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(serde_json::json!({ "ok": true, "path": path }))
    } else {
        Ok(serde_json::json!({
            "ok": false,
            "error": format!("xdg-open exit {:?}", status.code()),
            "path": path
        }))
    }
}

#[tauri::command]
async fn list_monitors() -> serde_json::Value {
    let mons = engine::list_monitors().await;
    serde_json::json!({ "ok": true, "monitors": mons })
}

#[tauri::command]
fn health(state: State<'_, AppState>) -> serde_json::Value {
    let count = state.inner.items.lock().unwrap().len();
    let current = state.inner.current_id.lock().unwrap().clone();
    serde_json::json!({
        "ok": true,
        "count": count,
        "current": current,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "0");
    std::env::remove_var("LIBGL_ALWAYS_SOFTWARE");
    std::env::remove_var("GALLIUM_DRIVER");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_focus();
                let _ = w.unminimize();
            }
        }))
        .manage(AppState::new())
        .manage(SteamState::new())
        .invoke_handler(tauri::generate_handler![
            list_wallpapers,
            rescan,
            launch,
            stop,
            current,
            get_settings,
            set_settings,
            open_folder,
            list_monitors,
            health,
            steam::steam_mode_on,
            steam::steam_mode_off,
            steam::steam_status,
            steam::workshop_query,
            steam::workshop_subscribe,
            steam::workshop_unsubscribe,
        ])
        .setup(|app| {
            let state = app.state::<AppState>();

            // Prefer standalone systemd daemon if already bound; otherwise embed HTTP for browser gallery.
            http_daemon::spawn_embedded(state.inner.clone());

            // seed index if empty — never block the event loop on a full walk+thumb pass
            let empty = state.inner.items.lock().unwrap().is_empty();
            if empty {
                let items = index::load_cache();
                if items.is_empty() {
                    let handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        let result = tokio::task::spawn_blocking(|| index::rescan(None))
                            .await
                            .ok();
                        if let Some(result) = result {
                            if let Some(st) = handle.try_state::<AppState>() {
                                if let Ok(mut g) = st.inner.items.lock() {
                                    *g = result.items;
                                }
                            }
                            let _ = handle.emit("library-updated", ());
                        }
                    });
                } else {
                    let handle = app.handle().clone();
                    {
                        let mut g = state.inner.items.lock().unwrap();
                        *g = items.clone();
                    }
                    tauri::async_runtime::spawn(async move {
                        let items = tokio::task::spawn_blocking(move || {
                            let mut items = items;
                            index::ensure_thumbs(&mut items);
                            index::save_cache(&mut items);
                            items
                        })
                        .await
                        .ok();
                        if let Some(items) = items {
                            if let Some(st) = handle.try_state::<AppState>() {
                                if let Ok(mut g) = st.inner.items.lock() {
                                    *g = items;
                                }
                            }
                            let _ = handle.emit("library-updated", ());
                        }
                    });
                }
            } else {
                let mut items = state.inner.items.lock().unwrap().clone();
                let need = items
                    .iter()
                    .any(|w| w.thumb.is_empty() && !w.preview.is_empty());
                if need {
                    let handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        let items = tokio::task::spawn_blocking(move || {
                            index::ensure_thumbs(&mut items);
                            index::save_cache(&mut items);
                            items
                        })
                        .await
                        .ok();
                        if let Some(items) = items {
                            if let Some(st) = handle.try_state::<AppState>() {
                                if let Ok(mut g) = st.inner.items.lock() {
                                    *g = items;
                                }
                            }
                            let _ = handle.emit("library-updated", ());
                        }
                    });
                }
            }

            // Auto-rescan every 45s + notify UI (even if HTTP daemon already owns rescan, refresh memory).
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(
                            http_daemon::AUTO_RESCAN_SEC,
                        ))
                        .await;
                        let prev = handle
                            .try_state::<AppState>()
                            .and_then(|st| st.inner.items.lock().ok().map(|g| g.clone()));
                        let result = tokio::task::spawn_blocking(move || index::rescan(prev))
                            .await
                            .ok();
                        if let Some(result) = result {
                            if result.ok {
                                if let Some(st) = handle.try_state::<AppState>() {
                                    if let Ok(mut g) = st.inner.items.lock() {
                                        *g = result.items;
                                    }
                                }
                                let _ = handle.emit("library-updated", ());
                            }
                        }
                    }
                });
            }

            #[cfg(target_os = "linux")]
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.with_webview(|wv| {
                    use webkit2gtk::{HardwareAccelerationPolicy, SettingsExt, WebViewExt};
                    let webview = wv.inner();
                    if let Some(settings) = webview.settings() {
                        settings.set_hardware_acceleration_policy(
                            HardwareAccelerationPolicy::Always,
                        );
                        settings.set_enable_webgl(true);
                        eprintln!(
                            "[lwe] WebKit HA policy={:?}",
                            settings.hardware_acceleration_policy()
                        );
                    } else {
                        eprintln!("[lwe] WebKit settings() = None");
                    }
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// bins use wpengine_lib::{http_daemon, index, ...}
