//! Workshop library index — port of tooling/lwe-index.py (1:1 behaviour).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

const MIN_DIR_BYTES: u64 = 64;
const AUDIO_EXT: &[&str] = &[".mp3", ".ogg", ".wav", ".flac", ".m4a", ".opus", ".aac"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wallpaper {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub wtype: String,
    pub tags: Vec<String>,
    /// Full workshop preview (detail panel). May be large JPG / animated GIF.
    pub preview: String,
    /// Small static JPEG for the grid (~384px). Empty = fall back to preview.
    #[serde(default)]
    pub thumb: String,
    pub dir: String,
    pub mtime: f64,
    pub file: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub size: String,
    #[serde(default)]
    pub audio: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RescanResult {
    pub ok: bool,
    pub items: Vec<Wallpaper>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub updated: Vec<String>,
    pub skipped_incomplete: Vec<String>,
    pub count: usize,
    pub scanned_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn workshop_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(".local/share/Steam/steamapps/workshop/content/431960")
}

pub fn cache_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(".cache/lwe_index.json")
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn read_json_object(path: &Path) -> Option<Value> {
    let raw = fs::read_to_string(path).ok()?;
    // Python used errors="ignore"; lossy is close enough for workshop JSON
    let data: Value = serde_json::from_str(&raw).ok()?;
    if data.is_object() {
        Some(data)
    } else {
        None
    }
}

fn normalize_tags(tags: &Value) -> Vec<String> {
    match tags {
        Value::String(s) if !s.trim().is_empty() => vec![s.trim().to_string()],
        Value::Array(arr) => {
            let mut out = Vec::new();
            for t in arr {
                if let Some(s) = t.as_str() {
                    if !s.trim().is_empty() {
                        out.push(s.trim().to_string());
                    }
                } else if let Some(obj) = t.as_object() {
                    for k in ["label", "value", "category", "name"] {
                        if let Some(Value::String(s)) = obj.get(k) {
                            if !s.trim().is_empty() {
                                out.push(s.trim().to_string());
                                break;
                            }
                        }
                    }
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

fn find_preview(d: &Path, data: &Value) -> String {
    let mut candidates: Vec<String> = Vec::new();
    if let Some(p) = data.get("preview").and_then(|v| v.as_str()) {
        candidates.push(p.to_string());
    }
    for name in [
        "preview.jpg",
        "preview.jpeg",
        "preview.png",
        "preview.gif",
        "preview.webp",
    ] {
        candidates.push(name.to_string());
    }
    for name in candidates {
        let p = d.join(&name);
        if p.is_file() {
            if let Ok(meta) = p.metadata() {
                if meta.len() > 0 {
                    if let Ok(r) = p.canonicalize() {
                        return r.display().to_string();
                    }
                    return p.display().to_string();
                }
            }
        }
    }
    String::new()
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

/// Single walk: total size + audio presence (was two full WalkDirs per wallpaper).
fn dir_stats(d: &Path, wtype: &str, file_field: &str) -> (u64, bool) {
    if wtype == "video" {
        // still need size; audio known
        let mut total = 0u64;
        for entry in WalkDir::new(d).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                if let Ok(meta) = entry.metadata() {
                    total = total.saturating_add(meta.len());
                }
            }
        }
        return (total, true);
    }

    let mut total = 0u64;
    let mut audio = false;
    if !file_field.is_empty() {
        let path = Path::new(file_field);
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let e = format!(".{}", ext.to_lowercase());
            if AUDIO_EXT.contains(&e.as_str()) {
                audio = true;
            }
        }
    }
    for entry in WalkDir::new(d).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            total = total.saturating_add(meta.len());
        }
        if !audio {
            let ext = entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| format!(".{}", e.to_lowercase()))
                .unwrap_or_default();
            if AUDIO_EXT.contains(&ext.as_str()) {
                audio = true;
            }
        }
    }
    (total, audio)
}

pub fn thumbs_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(".cache/lwe_thumbs")
}

/// Grid-friendly static thumb. Workshop previews are often 0.5–1MB JPGs or animated GIFs;
/// decoding those for every card melts WebKitGTK. Output: ~384px JPEG in ~/.cache/lwe_thumbs.
pub fn ensure_thumb(id: &str, preview: &str) -> String {
    if preview.is_empty() || id.is_empty() {
        return String::new();
    }
    let src = Path::new(preview);
    if !src.is_file() {
        return String::new();
    }
    let out_dir = thumbs_dir();
    let _ = fs::create_dir_all(&out_dir);
    let out = out_dir.join(format!("{id}.jpg"));

    // Reuse if thumb is newer-or-equal than source.
    if let (Ok(sm), Ok(om)) = (src.metadata(), out.metadata()) {
        let src_m = sm.modified().ok();
        let out_m = om.modified().ok();
        if om.len() > 0 {
            if let (Some(s), Some(o)) = (src_m, out_m) {
                if o >= s {
                    return out.display().to_string();
                }
            } else {
                return out.display().to_string();
            }
        }
    }

    // Prefer magick (IM7), fall back to convert (IM6). First frame of GIF via [0].
    let src_s = preview.to_string();
    let src_frame = format!("{src_s}[0]");
    let out_s = out.display().to_string();
    let try_cmd = |bin: &str| -> bool {
        std::process::Command::new(bin)
            .args([
                &src_frame,
                "-auto-orient",
                "-resize",
                "384x384>",
                "-strip",
                "-quality",
                "78",
                &out_s,
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    if try_cmd("magick") || try_cmd("convert") {
        if out.is_file() {
            return out.display().to_string();
        }
    }
    String::new()
}

pub fn parse_dir(d: &Path) -> Option<Wallpaper> {
    if !d.is_dir() {
        return None;
    }
    let name = d.file_name()?.to_str()?;
    if !name.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let pj = d.join("project.json");
    if !pj.is_file() {
        return None;
    }
    let st = pj.metadata().ok()?;
    if st.len() < MIN_DIR_BYTES {
        return None;
    }
    let mtime = st
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let data = read_json_object(&pj)?;
    let wtype = data
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .trim()
        .to_lowercase();
    let title = data
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| name.to_string());

    let file_field = data
        .get("file")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // still-downloading stub
    if (wtype.is_empty() || wtype == "?") && file_field.is_empty() {
        return None;
    }

    let (size_b, audio) = dir_stats(d, &wtype, &file_field);
    let desc = data
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let dir_resolved = d
        .canonicalize()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| d.display().to_string());

    let preview = find_preview(d, &data);
    let thumb = ensure_thumb(name, &preview);

    Some(Wallpaper {
        id: name.to_string(),
        title,
        wtype: if wtype.is_empty() {
            "?".into()
        } else {
            wtype.clone()
        },
        tags: data
            .get("tags")
            .map(normalize_tags)
            .unwrap_or_default(),
        preview,
        thumb,
        dir: dir_resolved,
        mtime,
        file: file_field.clone(),
        description: desc,
        size_bytes: size_b,
        size: fmt_size(size_b),
        audio,
    })
}

pub fn load_cache() -> Vec<Wallpaper> {
    let path = cache_path();
    if !path.exists() {
        eprintln!("[lwe] cache missing: {}", path.display());
        return Vec::new();
    }
    match fs::read_to_string(&path) {
        Ok(raw) => match serde_json::from_str::<Vec<Wallpaper>>(&raw) {
            Ok(v) => {
                eprintln!("[lwe] cache loaded {} items from {}", v.len(), path.display());
                v
            }
            Err(e) => {
                eprintln!("[lwe] cache deserialize error: {e}");
                // fall back: rebuild from disk
                rebuild()
            }
        },
        Err(e) => {
            eprintln!("[lwe] cache read error: {e}");
            Vec::new()
        }
    }
}

pub fn save_cache(items: &mut Vec<Wallpaper>) {
    items.sort_by(|a, b| {
        b.mtime
            .partial_cmp(&a.mtime)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    if let Some(parent) = cache_path().parent() {
        let _ = fs::create_dir_all(parent);
    }
    // Compact JSON: faster write + smaller ~/.cache/lwe_index.json (pretty was ~2×).
    if let Ok(raw) = serde_json::to_string(items) {
        let _ = fs::write(cache_path(), raw);
    }
}

/// Backfill thumbs for cache entries that predate the thumb field (cheap: skips if file exists).
pub fn ensure_thumbs(items: &mut [Wallpaper]) {
    for w in items.iter_mut() {
        if w.thumb.is_empty() && !w.preview.is_empty() {
            w.thumb = ensure_thumb(&w.id, &w.preview);
        }
    }
}

/// Incremental scan matching Python rescan().
pub fn rescan(prev: Option<Vec<Wallpaper>>) -> RescanResult {
    let prev = prev.unwrap_or_else(load_cache);
    let by_id: HashMap<String, Wallpaper> = prev.into_iter().map(|w| (w.id.clone(), w)).collect();

    let mut present_ids: HashSet<String> = HashSet::new();
    let mut incomplete: Vec<String> = Vec::new();
    let mut added: Vec<String> = Vec::new();
    let mut updated: Vec<String> = Vec::new();
    let mut new_items: Vec<Wallpaper> = Vec::new();

    let ws = workshop_dir();
    if ws.exists() {
        if let Ok(rd) = fs::read_dir(&ws) {
            for ent in rd.flatten() {
                let d = ent.path();
                if !d.is_dir() {
                    continue;
                }
                let Some(name) = d.file_name().and_then(|n| n.to_str()).map(|s| s.to_string())
                else {
                    continue;
                };
                if !name.chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }
                present_ids.insert(name.clone());
                let pj = d.join("project.json");
                if !pj.is_file() {
                    incomplete.push(name.clone());
                    if let Some(old) = by_id.get(&name) {
                        new_items.push(old.clone());
                    }
                    continue;
                }
                let mtime = match pj.metadata().and_then(|m| m.modified()) {
                    Ok(t) => t
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0),
                    Err(_) => {
                        incomplete.push(name.clone());
                        if let Some(old) = by_id.get(&name) {
                            new_items.push(old.clone());
                        }
                        continue;
                    }
                };

                let old = by_id.get(&name);
                if let Some(old) = old {
                    if (old.mtime - mtime).abs() < 0.001 && !old.title.is_empty() {
                        let mut kept = old.clone();
                        // Keep fast path, but still fill missing thumbs without re-parsing.
                        if kept.thumb.is_empty() && !kept.preview.is_empty() {
                            kept.thumb = ensure_thumb(&kept.id, &kept.preview);
                        }
                        new_items.push(kept);
                        continue;
                    }
                }

                match parse_dir(&d) {
                    Some(parsed) => {
                        if old.is_none() {
                            added.push(name.clone());
                        } else if let Some(old) = old {
                            if old.title != parsed.title
                                || old.wtype != parsed.wtype
                                || old.preview != parsed.preview
                                || old.tags != parsed.tags
                            {
                                updated.push(name.clone());
                            }
                        }
                        new_items.push(parsed);
                    }
                    None => {
                        incomplete.push(name.clone());
                        if let Some(old) = old {
                            new_items.push(old.clone());
                        }
                    }
                }
            }
        }
    }

    let mut removed: Vec<String> = by_id
        .keys()
        .filter(|id| !present_ids.contains(*id))
        .cloned()
        .collect();
    removed.sort();
    added.sort();
    updated.sort();
    incomplete.sort();

    save_cache(&mut new_items);
    let count = new_items.len();
    RescanResult {
        ok: true,
        items: new_items,
        added,
        removed,
        updated,
        skipped_incomplete: incomplete,
        count,
        scanned_at: now_secs(),
        error: None,
    }
}

pub fn rebuild() -> Vec<Wallpaper> {
    let mut items = Vec::new();
    let ws = workshop_dir();
    if ws.exists() {
        if let Ok(rd) = fs::read_dir(&ws) {
            for ent in rd.flatten() {
                let d = ent.path();
                if let Some(it) = parse_dir(&d) {
                    items.push(it);
                }
            }
        }
    }
    save_cache(&mut items);
    items
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoveLocalResult {
    pub id: String,
    pub disk_deleted: bool,
    pub cache_removed: bool,
    pub count: usize,
}

/// Permanently drop a workshop wallpaper from the local library.
/// Deletes `workshop/content/431960/<id>` (if present) + thumb cache entry, then rewrites index cache.
/// `id` must be digits-only (workshop published file id).
pub fn remove_local(id: &str) -> Result<RemoveLocalResult, String> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return Err("invalid workshop id".into());
    }
    let ws = workshop_dir();
    let dir = ws.join(id);
    let mut disk_deleted = false;

    if dir.exists() {
        // Refuse path escape: resolved dir must stay under workshop root.
        let ws_abs = fs::canonicalize(&ws).map_err(|e| format!("workshop dir: {e}"))?;
        let dir_abs = fs::canonicalize(&dir).map_err(|e| format!("item dir: {e}"))?;
        if !dir_abs.starts_with(&ws_abs) {
            return Err("refusing to delete path outside workshop content".into());
        }
        if dir_abs == ws_abs {
            return Err("refusing to delete workshop root".into());
        }
        fs::remove_dir_all(&dir_abs).map_err(|e| format!("delete {id}: {e}"))?;
        disk_deleted = true;
        eprintln!("[lwe] remove_local deleted {}", dir_abs.display());
    }

    // Drop cached grid thumb if any
    let thumb = thumbs_dir().join(format!("{id}.jpg"));
    if thumb.is_file() {
        let _ = fs::remove_file(&thumb);
    }

    let mut items = load_cache();
    let before = items.len();
    items.retain(|w| w.id != id);
    let cache_removed = before != items.len();
    save_cache(&mut items);

    Ok(RemoveLocalResult {
        id: id.to_string(),
        disk_deleted,
        cache_removed,
        count: items.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_local_rejects_bad_id() {
        assert!(remove_local("").is_err());
        assert!(remove_local("abc").is_err());
        assert!(remove_local("../431960").is_err());
        assert!(remove_local("12ab").is_err());
    }

    #[test]
    fn remove_local_missing_is_ok() {
        let r = remove_local("999000000000099").expect("missing should succeed");
        assert!(!r.disk_deleted);
        assert_eq!(r.id, "999000000000099");
    }

    #[test]
    fn remove_local_deletes_dummy_folder() {
        let id = "999000000000042";
        let dir = workshop_dir().join(id);
        if !workshop_dir().is_dir() {
            eprintln!("skip: workshop dir missing");
            return;
        }
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir dummy");
        fs::write(dir.join("project.json"), r#"{"title":"lwe-purge-test-dummy"}"#)
            .expect("write project");
        assert!(dir.is_dir());
        let r = remove_local(id).expect("purge dummy");
        assert!(r.disk_deleted, "expected disk_deleted=true");
        assert!(!dir.exists(), "dummy folder must be gone");
    }
}
