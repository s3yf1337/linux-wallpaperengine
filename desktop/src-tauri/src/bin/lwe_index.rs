//! CLI indexer — Rust replacement for tooling/lwe-index.py
//!
//! Usage:
//!   lwe-index --rebuild | --rescan | --list | --json | --gallery

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

fn gallery_src() -> Option<PathBuf> {
    // Prefer repo copy next to this binary's source tree, then installed path heuristics.
    let home = dirs::home_dir()?;
    let candidates = [
        home.join("AAAcode/linux-wallpaperengine/tooling/wallpapers.html"),
        home.join("wallpapers.html"),
    ];
    for c in candidates {
        if c.is_file() {
            if let Ok(meta) = c.metadata() {
                if meta.len() > 1000 {
                    return Some(c);
                }
            }
        }
    }
    None
}

fn main() {
    let mode = env::args().nth(1).unwrap_or_else(|| "--list".into());
    match mode.as_str() {
        "--rebuild" => {
            let items = wpengine_lib::index::rebuild();
            println!(
                "indexed {} wallpapers -> {}",
                items.len(),
                wpengine_lib::index::cache_path().display()
            );
        }
        "--rescan" => {
            let result = wpengine_lib::index::rescan(None);
            println!(
                "rescan: total={} +{} -{} ~{} incomplete={}",
                result.count,
                result.added.len(),
                result.removed.len(),
                result.updated.len(),
                result.skipped_incomplete.len()
            );
            if !result.added.is_empty() {
                println!("added: {}", result.added.join(", "));
            }
            if !result.removed.is_empty() {
                println!("removed: {}", result.removed.join(", "));
            }
            if !result.updated.is_empty() {
                println!("updated: {}", result.updated.join(", "));
            }
            if !result.skipped_incomplete.is_empty() {
                println!(
                    "incomplete (no project.json yet): {}",
                    result.skipped_incomplete.join(", ")
                );
            }
        }
        "--list" => {
            let items = {
                let c = wpengine_lib::index::load_cache();
                if c.is_empty() {
                    wpengine_lib::index::rebuild()
                } else {
                    c
                }
            };
            for it in items {
                println!("{} [{}] {}", it.title, it.wtype, it.id);
            }
        }
        "--json" => {
            let items = {
                let c = wpengine_lib::index::load_cache();
                if c.is_empty() {
                    wpengine_lib::index::rebuild()
                } else {
                    c
                }
            };
            println!("{}", serde_json::to_string_pretty(&items).unwrap_or_else(|_| "[]".into()));
        }
        "--gallery" => {
            let result = wpengine_lib::index::rescan(None);
            let dest = dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("wallpapers.html");
            match gallery_src() {
                Some(src) => {
                    if src.canonicalize().ok() != dest.canonicalize().ok() {
                        if let Ok(text) = fs::read_to_string(&src) {
                            let _ = fs::write(&dest, text);
                        }
                    }
                    println!(
                        "gallery -> {} ({} wallpapers)",
                        dest.display(),
                        result.count
                    );
                    if !result.added.is_empty() {
                        println!("new: {}", result.added.join(", "));
                    }
                }
                None => {
                    eprintln!(
                        "wallpapers.html design not found; expected in ~/AAAcode/linux-wallpaperengine/tooling/"
                    );
                    process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("unknown mode; use --rebuild|--rescan|--gallery|--list|--json");
            process::exit(1);
        }
    }
}
