//! Headless HTTP control API on 127.0.0.1:45127 (Rust port of tooling/lwe-daemon.py).
//! Same routes as the old Python daemon so wallpapers.html / CLI keep working.

use crate::engine;
use crate::index;
use crate::settings;
use crate::state::SharedState;
use axum::extract::{Query, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tower_http::cors::{Any, CorsLayer};

pub const PORT: u16 = 45127;
pub const AUTO_RESCAN_SEC: u64 = 45;

type App = Arc<SharedState>;

fn json_ok(v: Value) -> Response {
    (StatusCode::OK, Json(v)).into_response()
}

fn json_err(code: StatusCode, msg: impl Into<String>) -> Response {
    (
        code,
        Json(json!({ "ok": false, "error": msg.into() })),
    )
        .into_response()
}

async fn route_list(State(st): State<App>) -> Response {
    let items = {
        let g = st.items.lock().unwrap();
        if !g.is_empty() {
            g.clone()
        } else {
            drop(g);
            let items = index::load_cache();
            let mut g = st.items.lock().unwrap();
            *g = items.clone();
            items
        }
    };
    (StatusCode::OK, Json(items)).into_response()
}

async fn route_rescan(State(st): State<App>) -> Response {
    let prev = {
        let g = st.items.lock().unwrap();
        if g.is_empty() {
            None
        } else {
            Some(g.clone())
        }
    };
    let result = tokio::task::spawn_blocking(move || index::rescan(prev))
        .await
        .unwrap_or_else(|e| index::RescanResult {
            ok: false,
            items: vec![],
            added: vec![],
            removed: vec![],
            updated: vec![],
            skipped_incomplete: vec![],
            count: 0,
            scanned_at: 0.0,
            error: Some(format!("join: {e}")),
        });
    if result.ok {
        let mut g = st.items.lock().unwrap();
        *g = result.items.clone();
    }
    (StatusCode::OK, Json(result)).into_response()
}

#[derive(Debug, Deserialize)]
struct LaunchQ {
    id: Option<String>,
    volume: Option<String>,
    fps: Option<String>,
    silent: Option<String>,
    scaling: Option<String>,
    no_fullscreen_pause: Option<String>,
    disable_mouse: Option<String>,
    noautomute: Option<String>,
}

fn parse_bool_s(v: &str) -> bool {
    matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

fn opts_from_query(base: &settings::PlaybackSettings, q: &LaunchQ) -> settings::PlaybackSettings {
    let mut patch = serde_json::Map::new();
    if let Some(v) = &q.volume {
        if let Ok(n) = v.parse::<u64>() {
            patch.insert("volume".into(), json!(n));
        }
    }
    if let Some(v) = &q.fps {
        if let Ok(n) = v.parse::<u64>() {
            patch.insert("fps".into(), json!(n));
        }
    }
    if let Some(v) = &q.silent {
        patch.insert("silent".into(), json!(parse_bool_s(v)));
    }
    if let Some(v) = &q.scaling {
        patch.insert("scaling".into(), json!(v));
    }
    if let Some(v) = &q.no_fullscreen_pause {
        patch.insert("no_fullscreen_pause".into(), json!(parse_bool_s(v)));
    }
    if let Some(v) = &q.disable_mouse {
        patch.insert("disable_mouse".into(), json!(parse_bool_s(v)));
    }
    if let Some(v) = &q.noautomute {
        patch.insert("noautomute".into(), json!(parse_bool_s(v)));
    }
    settings::merge(base, &Value::Object(patch))
}

async fn route_launch(State(st): State<App>, Query(q): Query<LaunchQ>) -> Response {
    let id = q.id.clone().unwrap_or_default();
    if id.trim().is_empty() {
        return json_err(StatusCode::BAD_REQUEST, "missing id");
    }
    let base = st.settings.lock().unwrap().clone();
    let merged = opts_from_query(&base, &q);
    {
        let mut g = st.settings.lock().unwrap();
        *g = merged.clone();
    }
    settings::save(&merged);

    {
        let mut ch = st.child.lock().await;
        if let Some(mut c) = ch.take() {
            let _ = c.kill().await;
        }
    }

    // HTTP gallery has no property UI yet — use saved overrides if any.
    let prop_flags: Vec<(String, String)> = crate::props::resolve_for_launch(&id, None)
        .into_iter()
        .collect();
    match engine::spawn_engine(&id, &merged, &prop_flags).await {
        Ok(child) => {
            {
                let mut ch = st.child.lock().await;
                *ch = Some(child);
            }
            {
                let mut cur = st.current_id.lock().unwrap();
                *cur = Some(id.clone());
            }
            json_ok(json!({ "ok": true, "id": id, "opts": merged }))
        }
        Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn route_stop(State(st): State<App>) -> Response {
    {
        let mut ch = st.child.lock().await;
        if let Some(mut c) = ch.take() {
            let _ = c.kill().await;
        }
    }
    engine::kill_engine().await;
    {
        let mut cur = st.current_id.lock().unwrap();
        *cur = None;
    }
    json_ok(json!({ "ok": true }))
}

async fn route_current(State(st): State<App>) -> Response {
    let id = st.current_id.lock().unwrap().clone();
    let opts = st.settings.lock().unwrap().clone();
    json_ok(json!({ "id": id, "opts": opts }))
}

async fn route_health(State(st): State<App>) -> Response {
    let count = st.items.lock().unwrap().len();
    let current = st.current_id.lock().unwrap().clone();
    json_ok(json!({ "ok": true, "count": count, "current": current }))
}

async fn route_settings(State(st): State<App>) -> Response {
    let s = st.settings.lock().unwrap().clone();
    json_ok(json!({ "ok": true, "settings": s }))
}

async fn route_settings_set(State(st): State<App>, Query(q): Query<LaunchQ>) -> Response {
    let base = st.settings.lock().unwrap().clone();
    let merged = opts_from_query(&base, &q);
    {
        let mut g = st.settings.lock().unwrap();
        *g = merged.clone();
    }
    settings::save(&merged);
    json_ok(json!({ "ok": true, "settings": merged }))
}

#[derive(Debug, Deserialize)]
struct IdQ {
    id: Option<String>,
}

async fn route_open(State(st): State<App>, Query(q): Query<IdQ>) -> Response {
    let id = q.id.unwrap_or_default();
    if id.is_empty() {
        return json_err(StatusCode::BAD_REQUEST, "missing id");
    }
    let path = {
        let items = st.items.lock().unwrap();
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
        return json_ok(json!({ "ok": false, "error": "folder not found", "path": path }));
    }
    let status = tokio::process::Command::new("xdg-open")
        .arg(&path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;
    match status {
        Ok(s) if s.success() => json_ok(json!({ "ok": true, "path": path })),
        Ok(s) => json_ok(json!({
            "ok": false,
            "error": format!("xdg-open exit {:?}", s.code()),
            "path": path
        })),
        Err(e) => json_ok(json!({ "ok": false, "error": e.to_string(), "path": path })),
    }
}

async fn route_monitors() -> Response {
    let mons = engine::list_monitors().await;
    json_ok(json!({ "ok": true, "monitors": mons }))
}

async fn route_not_found() -> Response {
    json_err(StatusCode::NOT_FOUND, "not found")
}

pub fn router(state: App) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::OPTIONS])
        .allow_headers(Any);

    Router::new()
        .route("/list", get(route_list))
        .route("/rescan", get(route_rescan))
        .route("/launch", get(route_launch))
        .route("/stop", get(route_stop))
        .route("/current", get(route_current))
        .route("/health", get(route_health))
        .route("/settings", get(route_settings))
        .route("/settings/set", get(route_settings_set))
        .route("/open", get(route_open))
        .route("/monitors", get(route_monitors))
        .fallback(route_not_found)
        .layer(cors)
        .layer(axum::middleware::from_fn(no_store))
        .with_state(state)
}

async fn no_store(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let mut res = next.run(req).await;
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    res
}

/// Background auto-rescan every AUTO_RESCAN_SEC (same as Python daemon).
pub fn spawn_auto_rescan(state: App) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        loop {
            let prev = {
                let g = state.items.lock().unwrap();
                if g.is_empty() {
                    None
                } else {
                    Some(g.clone())
                }
            };
            let result = tokio::task::spawn_blocking(move || index::rescan(prev))
                .await
                .ok();
            if let Some(result) = result {
                if result.ok {
                    if !result.added.is_empty() {
                        eprintln!(
                            "[auto-rescan] +{} new: {}",
                            result.added.len(),
                            result.added.iter().take(8).cloned().collect::<Vec<_>>().join(", ")
                        );
                    }
                    let mut g = state.items.lock().unwrap();
                    *g = result.items;
                }
            }
            tokio::time::sleep(Duration::from_secs(AUTO_RESCAN_SEC)).await;
        }
    });
}

/// Run the daemon forever (headless binary entry).
pub async fn run_forever() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let state = Arc::new(SharedState::new());
    {
        let mut g = state.items.lock().unwrap();
        if g.is_empty() {
            let items = index::load_cache();
            *g = items;
        }
    }
    spawn_auto_rescan(state.clone());

    let app = router(state);
    let addr = SocketAddr::from(([127, 0, 0, 1], PORT));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("lwe-daemon listening on http://127.0.0.1:{PORT}");
    eprintln!("auto-rescan every {AUTO_RESCAN_SEC}s");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Start HTTP daemon as a background task (optional embed in Tauri).
/// Auto-rescan is owned by the GUI loop when embedded.
///
/// Must NOT use bare `tokio::spawn` here — Tauri's `setup` is not on a Tokio
/// runtime ("there is no reactor running"). Own runtime thread instead.
pub fn spawn_embedded(state: App) {
    let _ = std::thread::Builder::new()
        .name("lwe-http".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .thread_name("lwe-http-worker")
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("[lwe] HTTP runtime failed: {e}");
                    return;
                }
            };
            rt.block_on(async move {
                let app = router(state);
                let addr = SocketAddr::from(([127, 0, 0, 1], PORT));
                match tokio::net::TcpListener::bind(addr).await {
                    Ok(listener) => {
                        eprintln!("[lwe] embedded HTTP daemon on http://127.0.0.1:{PORT}");
                        if let Err(e) = axum::serve(listener, app).await {
                            eprintln!("[lwe] HTTP daemon stopped: {e}");
                        }
                    }
                    Err(e) => {
                        // already running (e.g. systemd lwe-daemon) — fine
                        eprintln!("[lwe] HTTP daemon not started (port {PORT}): {e}");
                    }
                }
            });
        });
}

