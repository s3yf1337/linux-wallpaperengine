//! Workshop (Steam UGC) source — lazy lifecycle, UGC query, subscribe/download.
//!
//! crate `steamworks` v0.13. SDK callback-based (not async): `Client::init_app`
//! sets `SteamAppId`/`SteamGameId` env vars before `init()` which uses ManualDispatch
//! — we own a background thread that calls `run_callbacks()` ~10 Hz.
//! App id 431960 = Wallpaper Engine. `steam_appid.txt` next to the binary — fallback.
//! `libsteam_api.so` next to the binary, loaded via rpath $ORIGIN (see `.cargo/config.toml`).
//!
//! Flow:
//! 1. toggle on → lazy Client::init_app + callback thread.
//! 2. query_all with text/tag filters → `QueryHandle::fetch(cb)` (async, CallResult registered).
//!    Results converted to `WorkshopItem` and emitted via `workshop-query-results` event.
//! 3. subscribe_item(id, cb) → Steam downloads in background.
//!    Poll loop reads item_state/download_info/install_info, emits `workshop-download-progress`
//!    and `workshop-item-installed` (with local path) when done.
//! 4. On toggle off: drop the Client → callbacks naturally stop, no logout needed.
//!
//! Tags: `QueryResult.tags` — the real string tags authored in the UGC API
//! (comma-separated in m_rgchTags, split in the lib), used directly, no heuristics like Library.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex as SyncMutex;
use steamworks::{
    AppId, AppIDs, Client, CreateQueryError, FileType, ItemState, PublishedFileId, QueryHandle,
    QueryResult, QueryResults, SteamError, UGCQueryType, UGCType,
};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex as AsyncMutex;

/// Wallpaper Engine Steam app id — consumer_app_id for UGC queries.
pub const WP_APP_ID: u32 = 431960;

/// Frontend-facing model — matches `index::Wallpaper` fields so we can reuse
/// the same card/grid/right-panel UI component. `dir` is only filled after install.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkshopItem {
    pub id: String,
    pub title: String,
    /// Workshop type label derived from `FileType` (scene / web / video / collection / ...).
    /// NOT a heuristic: we show the real file type registered by the author in Steamworks.
    #[serde(rename = "type")]
    pub wtype: String,
    /// Real author tags from `QueryResult.tags` (Steam UGC API) — comma-separated split in the lib.
    pub tags: Vec<String>,
    /// Full preview URL from the Steam CDN (for the detail panel).
    pub preview: String,
    /// For the grid we use the same preview URL (CDN serves a compressed thumb at the same URL).
    pub thumb: String,
    /// Local path — only filled after install (`workshop/content/431960/<id>/`).
    #[serde(default)]
    pub dir: String,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub size: String,
    #[serde(default)]
    pub description: String,
    /// steamID owner as decimal string — not tied to friends, but kept for info.
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    pub file_size: u64,
    #[serde(default)]
    pub upvotes: u32,
    #[serde(default)]
    pub downvotes: u32,
    #[serde(default)]
    pub score: f32,
    #[serde(default)]
    pub time_created: u32,
    #[serde(default)]
    pub time_updated: u32,
    #[serde(default)]
    pub url: String,
    /// Whether the user is subscribed (polled from `subscribed_items` at query time).
    #[serde(default)]
    pub subscribed: bool,
    /// Downloaded and installed (ItemState::INSTALLED).
    #[serde(default)]
    pub installed: bool,
}

fn fmt_size(n: u64) -> String {
    if n < 1024 {
        return format!("{n} B");
    }
    let kb = n as f64 / 1024.0;
    if n < 1024 * 1024 {
        return format!("{:.0} KB", kb);
    }
    let mb = n as f64 / (1024.0 * 1024.0);
    if n < 1024 * 1024 * 1024 {
        return format!("{:.1} MB", mb);
    }
    let gb = n as f64 / (1024.0 * 1024.0 * 1024.0);
    format!("{:.1} GB", gb)
}

/// Fallback label from Steam FileType enum (raw API).
fn file_type_label(ft: &FileType) -> &'static str {
    match ft {
        FileType::Community => "scene",
        FileType::Video => "video",
        FileType::WebGuide => "web",
        FileType::Collection => "collection",
        FileType::Art => "art",
        FileType::Game => "game",
        FileType::Software => "software",
        FileType::Concept => "concept",
        FileType::Screenshot => "screenshot",
        FileType::IntegratedGuide => "guide",
        FileType::GameManagedItem => "managed",
        FileType::Clip => "clip",
        FileType::SteamVideo => "steam-video",
        FileType::ControllerBinding => "controller",
        FileType::Microtransaction => "mtx",
        FileType::Merch => "merch",
        FileType::SteamworksAccessInvite => "invite",
    }
}

/// Prefer real author tags Scene/Video/Web (standard WE workshop tags) over
/// FileType::Community which is what almost all WE projects report.
fn wallpaper_type(qr: &QueryResult) -> String {
    for t in &qr.tags {
        match t.to_ascii_lowercase().as_str() {
            "scene" | "video" | "web" | "application" | "collection" => {
                return t.to_ascii_lowercase();
            }
            _ => {}
        }
    }
    file_type_label(&qr.file_type).to_string()
}

fn item_from_query(
    qr: &QueryResult,
    preview_url: String,
    subscribed: bool,
    installed: bool,
    dir: String,
) -> WorkshopItem {
    let size_b = qr.file_size as u64;
    WorkshopItem {
        id: qr.published_file_id.0.to_string(),
        title: qr.title.clone(),
        wtype: wallpaper_type(qr),
        tags: qr.tags.clone(),
        preview: preview_url.clone(),
        thumb: preview_url,
        dir,
        size_bytes: size_b,
        size: if size_b > 0 { fmt_size(size_b) } else { String::new() },
        description: qr.description.clone(),
        author: qr.owner.raw().to_string(),
        file_name: qr.file_name.clone(),
        file_size: size_b,
        upvotes: qr.num_upvotes,
        downvotes: qr.num_downvotes,
        score: qr.score,
        time_created: qr.time_created,
        time_updated: qr.time_updated,
        url: qr.url.clone(),
        subscribed,
        installed,
    }
}

/// Shared Steam state — owned by `AppState`, managed via Arc<AsyncMutex<…>>.
pub struct SteamState {
    /// The live Steam Client — None means Steam Mode off.
    client: AsyncMutex<Option<Client>>,
    /// Stop flag for the callback loop.
    stop_flag: Arc<SyncMutex<bool>>,
}

impl SteamState {
    pub fn new() -> Self {
        Self {
            client: AsyncMutex::new(None),
            stop_flag: Arc::new(SyncMutex::new(false)),
        }
    }

    pub async fn is_enabled(&self) -> bool {
        self.client.lock().await.is_some()
    }
}

/// `steam_mode_on` — lazy init + start callback loop. Idempotent.
/// Returns Err with human-readable cause if init fails (steam not running / not logged in / no license).
/// Frontend rolls back the toggle on Err and shows toast.
#[tauri::command]
pub async fn steam_mode_on(
    app: AppHandle,
    state: tauri::State<'_, SteamState>,
) -> Result<serde_json::Value, String> {
    {
        let mut g = state.client.lock().await;
        if g.is_some() {
            return Ok(serde_json::json!({ "ok": true, "already": true }));
        }
        let cl = Client::init_app(AppId(WP_APP_ID))
            .map_err(|e| format!("steamworks init failed: {e}"))?;
        *g = Some(cl);
    }
    let cl = {
        let g = state.client.lock().await;
        g.as_ref().unwrap().clone()
    };
    {
        let mut f = state.stop_flag.lock().unwrap();
        *f = false;
    }
    let stop = state.stop_flag.clone();
    std::thread::Builder::new()
        .name("steam-cb".into())
        .spawn(move || loop {
            if let Ok(f) = stop.lock() {
                if *f {
                    break;
                }
            }
            cl.run_callbacks();
            std::thread::sleep(std::time::Duration::from_millis(100));
        })
        .map_err(|e| format!("steam-cb spawn: {e}"))?;

    let _ = app.emit("workshop-mode-changed", serde_json::json!({ "on": true }));
    Ok(serde_json::json!({ "ok": true }))
}

/// `steam_mode_off` — drop the Client, callbacks stop naturally.
#[tauri::command]
pub async fn steam_mode_off(
    app: AppHandle,
    state: tauri::State<'_, SteamState>,
) -> Result<serde_json::Value, String> {
    {
        let mut g = state.client.lock().await;
        if g.is_none() {
            return Ok(serde_json::json!({ "ok": true, "already": true }));
        }
        *g = None;
    }
    {
        let mut f = state.stop_flag.lock().unwrap();
        *f = true;
    }
    let _ = app.emit("workshop-mode-changed", serde_json::json!({ "on": false }));
    Ok(serde_json::json!({ "ok": true }))
}

/// `steam_status` — whether Steam Mode is live.
#[tauri::command]
pub async fn steam_status(state: tauri::State<'_, SteamState>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "on": state.is_enabled().await }))
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QueryArgs {
    pub page: Option<u32>,
    pub search_text: Option<String>,
    pub required_tags: Option<Vec<String>>,
    pub excluded_tags: Option<Vec<String>>,
}

/// `workshop_query` — issue a UGC query for all Wallpaper Engine workshop items,
/// filterable by text/tags. Emits `workshop-query-results` with items array.
///
/// NOTE: `query_all` returns Items only; tags from the response are the real author tags,
/// NOT a heuristic. We request long descriptions + key-value tags + previews (default on).
#[tauri::command]
pub async fn workshop_query(
    app: AppHandle,
    state: tauri::State<'_, SteamState>,
    args: Option<QueryArgs>,
) -> Result<serde_json::Value, String> {
    let client = {
        let g = state.client.lock().await;
        match g.clone() {
            Some(c) => c,
            None => return Err("Steam Mode is off".into()),
        }
    };
    let args = args.unwrap_or_default();
    let page = args.page.unwrap_or(1).max(1);
    let ugc = client.ugc();
    // Text search needs RankedByTextSearch; otherwise publication date is fine.
    let query_type = if args
        .search_text
        .as_deref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
    {
        UGCQueryType::RankedByTextSearch
    } else {
        UGCQueryType::RankedByPublicationDate
    };
    let q: QueryHandle = ugc
        .query_all(
            query_type,
            UGCType::Items,
            AppIDs::ConsumerAppId(AppId(WP_APP_ID)),
            page,
        )
        .map_err(|_: CreateQueryError| "failed to create UGC query".to_string())?;
    let q = q.include_long_desc(true).include_key_value_tags(true);
    let q = if let Some(t) = args.search_text.as_deref().filter(|s| !s.is_empty()) {
        q.set_search_text(t)
    } else {
        q
    };
    let q = if let Some(tags) = args.required_tags.as_ref().filter(|v| !v.is_empty()) {
        let mut qq = q;
        for t in tags {
            qq = qq.add_required_tag(t);
        }
        qq
    } else {
        q
    };
    let q = if let Some(tags) = args.excluded_tags.as_ref().filter(|v| !v.is_empty()) {
        let mut qq = q;
        for t in tags {
            qq = qq.add_excluded_tag(t);
        }
        qq
    } else {
        q
    };

    // Snapshot of subscribed + installed state at query time.
    let subscribed_set: HashSet<u64> = ugc
        .subscribed_items(false)
        .iter()
        .map(|p| p.0)
        .collect();
    let client_for_cb = client.clone();
    let app_h = app.clone();
    q.fetch(move |res: Result<QueryResults<'_>, SteamError>| match res {
        Ok(results) => {
            let total = results.total_results();
            let count = results.returned_results();
            let ugc_cb = client_for_cb.ugc();
            let mut items: Vec<WorkshopItem> = Vec::with_capacity(count as usize);
            for i in 0..count {
                if let Some(qr) = results.get(i) {
                    let preview_url = results.preview_url(i).unwrap_or_default();
                    let pfid = qr.published_file_id.0;
                    let subscribed = subscribed_set.contains(&pfid);
                    let st = ugc_cb.item_state(qr.published_file_id);
                    let installed = st.contains(ItemState::INSTALLED);
                    let dir = if installed {
                        ugc_cb
                            .item_install_info(qr.published_file_id)
                            .map(|i| i.folder)
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    items.push(item_from_query(&qr, preview_url, subscribed, installed, dir));
                }
            }
            let _ = app_h.emit(
                "workshop-query-results",
                serde_json::json!({
                    "ok": true,
                    "page": page,
                    "total": total,
                    "items": items,
                }),
            );
        }
        Err(e) => {
            let _ = app_h.emit(
                "workshop-query-results",
                serde_json::json!({ "ok": false, "error": format!("{e:?}") }),
            );
        }
    });
    Ok(serde_json::json!({ "ok": true, "queued": true }))
}

/// `workshop_subscribe` — subscribe and start background download. Steam handles the transfer;
/// we poll state/install info until done. Emits:
/// - `workshop-subscribed` {ok,id} — sub diligence result
/// - `workshop-download-progress` {id,current,total,done,state} — until installed
/// - `workshop-item-installed` {id,folder,size_on_disk,timestamp} — final
#[tauri::command]
pub async fn workshop_subscribe(
    app: AppHandle,
    state: tauri::State<'_, SteamState>,
    published_file_id: String,
) -> Result<serde_json::Value, String> {
    let client = {
        let g = state.client.lock().await;
        match g.clone() {
            Some(c) => c,
            None => return Err("Steam Mode is off".into()),
        }
    };
    let pfid = PublishedFileId(published_file_id.parse::<u64>().map_err(|e| e.to_string())?);
    let app_h = app.clone();
    let id_str = published_file_id.clone();
    let app_h2 = app.clone();
    let id_str2 = published_file_id.clone();
    let cl2 = client.clone();
    client.ugc().subscribe_item(pfid, move |res: Result<(), SteamError>| match res {
        Ok(()) => {
            let _ = app_h.emit(
                "workshop-subscribed",
                serde_json::json!({ "ok": true, "id": id_str }),
            );
        }
        Err(e) => {
            let _ = app_h.emit(
                "workshop-subscribed",
                serde_json::json!({ "ok": false, "id": id_str, "error": format!("{e:?}") }),
            );
        }
    });
    tokio::spawn(async move {
        poll_download(cl2, pfid, id_str2, app_h2).await;
    });
    Ok(serde_json::json!({ "ok": true, "id": published_file_id }))
}

/// `workshop_unsubscribe` — unsubscribe; Steam may free disk content.
#[tauri::command]
pub async fn workshop_unsubscribe(
    state: tauri::State<'_, SteamState>,
    published_file_id: String,
) -> Result<serde_json::Value, String> {
    let client = {
        let g = state.client.lock().await;
        match g.clone() {
            Some(c) => c,
            None => return Err("Steam Mode is off".into()),
        }
    };
    let pfid = PublishedFileId(published_file_id.parse::<u64>().map_err(|e| e.to_string())?);
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), SteamError>>();
    client.ugc().unsubscribe_item(pfid, move |res| {
        let _ = tx.send(res);
    });
    // Pump callbacks for up to 5s — CallResult dispatch happens on run_callbacks.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let result = loop {
        if let Ok(r) = rx.try_recv() {
            break r;
        }
        if std::time::Instant::now() > deadline {
            return Err("unsubscribe: timeout".into());
        }
        client.run_callbacks();
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    match result {
        Ok(()) => Ok(serde_json::json!({ "ok": true })),
        Err(e) => Err(format!("unsubscribe: {e}")),
    }
}

/// Polling loop: until INSTALLED or timeout (10 min), emit progress.
async fn poll_download(client: Client, pfid: PublishedFileId, id_str: String, app: AppHandle) {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(600);
    loop {
        if start.elapsed() > timeout {
            let _ = app.emit(
                "workshop-download-progress",
                serde_json::json!({ "id": id_str, "error": "timeout", "done": false }),
            );
            return;
        }
        let st = client.ugc().item_state(pfid);
        if st.contains(ItemState::INSTALLED) {
            if let Some(info) = client.ugc().item_install_info(pfid) {
                let _ = app.emit(
                    "workshop-item-installed",
                    serde_json::json!({
                        "id": id_str,
                        "folder": info.folder,
                        "size_on_disk": info.size_on_disk,
                        "timestamp": info.timestamp,
                    }),
                );
                let _ = app.emit(
                    "workshop-download-progress",
                    serde_json::json!({
                        "id": id_str,
                        "done": true,
                        "current": info.size_on_disk,
                        "total": info.size_on_disk,
                        "state": st.bits(),
                    }),
                );
            } else {
                let _ = app.emit(
                    "workshop-download-progress",
                    serde_json::json!({ "id": id_str, "done": true, "state": st.bits() }),
                );
            }
            return;
        }
        if st.contains(ItemState::DOWNLOADING) {
            if let Some((cur, tot)) = client.ugc().item_download_info(pfid) {
                let _ = app.emit(
                    "workshop-download-progress",
                    serde_json::json!({
                        "id": id_str,
                        "done": false,
                        "current": cur,
                        "total": tot,
                        "state": st.bits(),
                    }),
                );
            }
        } else if !st.is_empty() {
            let _ = app.emit(
                "workshop-download-progress",
                serde_json::json!({ "id": id_str, "done": false, "state": st.bits() }),
            );
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}