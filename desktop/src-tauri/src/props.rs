//! Per-wallpaper properties from `project.json` (`general.properties`)
//! plus user overrides persisted under `~/.cache/lwe_props/<id>.json`.
//!
//! Engine wiring: overrides become repeated `--set-property key=value` flags.
//! Web wallpapers also receive the same values via CEF `applyUserProperties`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropOption {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropDef {
    pub key: String,
    /// bool | slider | combo | color | text | file | textinput | scenetexture | group | other
    #[serde(rename = "type")]
    pub ptype: String,
    pub text: String,
    /// Current value (defaults merged with saved overrides).
    pub value: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<PropOption>>,
    #[serde(default)]
    pub order: i64,
    /// True for scheme-color / pure UI chrome that we hide by default.
    #[serde(default)]
    pub hidden: bool,
}

pub fn props_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(".cache/lwe_props")
}

pub fn props_path(id: &str) -> PathBuf {
    props_dir().join(format!("{id}.json"))
}

pub fn load_overrides(id: &str) -> Map<String, Value> {
    let path = props_path(id);
    let Ok(raw) = fs::read_to_string(&path) else {
        return Map::new();
    };
    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

pub fn save_overrides(id: &str, overrides: &Map<String, Value>) -> Result<(), String> {
    if !id.chars().all(|c| c.is_ascii_digit()) {
        return Err("invalid id".into());
    }
    let dir = props_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = props_path(id);
    // Drop empty map → delete file
    if overrides.is_empty() {
        let _ = fs::remove_file(&path);
        return Ok(());
    }
    let raw = serde_json::to_string_pretty(overrides).map_err(|e| e.to_string())?;
    fs::write(&path, raw).map_err(|e| e.to_string())
}

fn workshop_project(id: &str) -> Option<PathBuf> {
    let p = dirs::home_dir()?
        .join(".local/share/Steam/steamapps/workshop/content/431960")
        .join(id)
        .join("project.json");
    if p.is_file() {
        Some(p)
    } else {
        None
    }
}

fn read_json(path: &Path) -> Option<Value> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn as_stringish(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => if *b { "1".into() } else { "0".into() },
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn is_hidden_prop(key: &str, text: &str) -> bool {
    key.eq_ignore_ascii_case("schemecolor")
        || text.starts_with("ui_browse_properties_")
        || text.eq_ignore_ascii_case("ui_browse_properties_scheme_color")
}

/// Parse `general.properties` from a project.json object into PropDef list (defaults only).
pub fn parse_properties_from_project(data: &Value) -> Vec<PropDef> {
    let Some(props) = data
        .pointer("/general/properties")
        .and_then(|v| v.as_object())
    else {
        return Vec::new();
    };

    let mut out: Vec<PropDef> = Vec::new();
    for (key, meta) in props {
        let Some(obj) = meta.as_object() else {
            continue;
        };
        let ptype = obj
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("group")
            .to_lowercase();
        // Groups / empty types are non-interactive section headers.
        if ptype == "group" || ptype.is_empty() {
            continue;
        }
        let text = obj
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or(key.as_str())
            .to_string();
        let order = obj
            .get("order")
            .and_then(|v| v.as_i64())
            .or_else(|| obj.get("index").and_then(|v| v.as_i64()))
            .unwrap_or(0);
        let value = obj.get("value").cloned().unwrap_or(Value::Null);
        let min = obj.get("min").and_then(|v| v.as_f64());
        let max = obj.get("max").and_then(|v| v.as_f64());
        let step = obj.get("step").and_then(|v| v.as_f64());
        let options = obj.get("options").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|o| {
                    let o = o.as_object()?;
                    let label = o
                        .get("label")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string();
                    let value = match o.get("value") {
                        Some(Value::String(s)) => s.clone(),
                        Some(Value::Number(n)) => n.to_string(),
                        Some(Value::Bool(b)) => b.to_string(),
                        _ => return None,
                    };
                    Some(PropOption { label, value })
                })
                .collect::<Vec<_>>()
        });

        out.push(PropDef {
            key: key.clone(),
            ptype,
            text: text.clone(),
            value,
            min,
            max,
            step,
            options,
            order,
            hidden: is_hidden_prop(key, &text),
        });
    }
    out.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.key.cmp(&b.key)));
    out
}

/// Load property definitions for a workshop id and merge saved overrides into `.value`.
pub fn get_wallpaper_properties(id: &str) -> Result<Vec<PropDef>, String> {
    if !id.chars().all(|c| c.is_ascii_digit()) {
        return Err("invalid id".into());
    }
    let path = workshop_project(id).ok_or_else(|| "project.json not found".to_string())?;
    let data = read_json(&path).ok_or_else(|| "failed to parse project.json".to_string())?;
    let mut defs = parse_properties_from_project(&data);
    let overrides = load_overrides(id);
    for def in &mut defs {
        if let Some(v) = overrides.get(&def.key) {
            def.value = v.clone();
        }
    }
    Ok(defs)
}

/// Persist overrides (only keys that differ is fine; we store the full map the UI sends).
pub fn set_wallpaper_properties(id: &str, props: &Map<String, Value>) -> Result<(), String> {
    save_overrides(id, props)
}

/// Convert a UI value map into `--set-property key=value` pairs for the engine CLI.
pub fn to_set_property_flags(props: &Map<String, Value>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (k, v) in props {
        if k.is_empty() || k == "schemecolor" {
            // scheme colour is UI chrome; engine accepts it but we skip noise by default
            // still allow if explicitly set as non-default? Keep skip for now.
            continue;
        }
        let s = match v {
            Value::Bool(b) => if *b { "1".into() } else { "0".into() },
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
            Value::Null => continue,
            other => as_stringish(other),
        };
        out.push((k.clone(), s));
    }
    out
}

/// Merge saved overrides with a patch from the launch payload.
pub fn merge_override_maps(base: Map<String, Value>, patch: &Value) -> Map<String, Value> {
    let mut out = base;
    if let Some(obj) = patch.as_object() {
        for (k, v) in obj {
            if v.is_null() {
                out.remove(k);
            } else {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    out
}

/// Convenience: load overrides for id, optionally merge launch patch, return set-property pairs.
pub fn resolve_for_launch(id: &str, launch_props: Option<&Value>) -> HashMap<String, String> {
    let mut map = load_overrides(id);
    if let Some(p) = launch_props {
        map = merge_override_maps(map, p);
        // persist what the UI sent so next select shows the same
        let _ = save_overrides(id, &map);
    }
    to_set_property_flags(&map)
        .into_iter()
        .collect()
}

/// Build a JSON object of current values for debugging / web inject side-channels.
pub fn values_object(defs: &[PropDef]) -> Value {
    let mut m = Map::new();
    for d in defs {
        m.insert(d.key.clone(), d.value.clone());
    }
    Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_combo_and_slider() {
        let data = json!({
            "general": {
                "properties": {
                    "clock": {
                        "type": "combo",
                        "text": "Clock",
                        "value": "1",
                        "order": 100,
                        "options": [
                            {"label": "12Hr", "value": "1"},
                            {"label": "24Hr", "value": "2"}
                        ]
                    },
                    "musicvolume": {
                        "type": "slider",
                        "text": "Music Volume",
                        "value": 0.8,
                        "min": 0,
                        "max": 1,
                        "step": 0.1,
                        "order": 106
                    },
                    "schemecolor": {
                        "type": "color",
                        "text": "ui_browse_properties_scheme_color",
                        "value": "0 0 0",
                        "order": 0
                    }
                }
            }
        });
        let defs = parse_properties_from_project(&data);
        assert_eq!(defs.len(), 3);
        let clock = defs.iter().find(|d| d.key == "clock").unwrap();
        assert_eq!(clock.ptype, "combo");
        assert_eq!(clock.options.as_ref().unwrap().len(), 2);
        let scheme = defs.iter().find(|d| d.key == "schemecolor").unwrap();
        assert!(scheme.hidden);
        let flags = to_set_property_flags(&{
            let mut m = Map::new();
            m.insert("clock".into(), json!("2"));
            m.insert("musicvolume".into(), json!(0.5));
            m.insert("schemecolor".into(), json!("1 0 0"));
            m
        });
        assert!(flags.iter().any(|(k, v)| k == "clock" && v == "2"));
        assert!(flags.iter().any(|(k, v)| k == "musicvolume" && v == "0.5"));
        assert!(!flags.iter().any(|(k, _)| k == "schemecolor"));
    }
}
