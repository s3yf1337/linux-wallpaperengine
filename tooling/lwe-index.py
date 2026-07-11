#!/usr/bin/env python3
"""Index Wallpaper Engine workshop backgrounds + optional HTML gallery.

Steam downloads land in:
  ~/.local/share/Steam/steamapps/workshop/content/431960/<id>/

Usage:
  lwe-index.py --rebuild            # full rebuild of ~/.cache/lwe_index.json
  lwe-index.py --rescan             # incremental: only new/changed/removed
  lwe-index.py --gallery            # rescan + write ~/wallpapers.html
  lwe-index.py --list               # print title [type] id
  lwe-index.py --json               # dump current index as JSON to stdout
"""
from __future__ import annotations

import json
import os
import sys
import time
from pathlib import Path
from typing import Any

WORKSHOP = Path.home() / ".local/share/Steam/steamapps/workshop/content/431960"
CACHE = Path.home() / ".cache/lwe_index.json"
GALLERY = Path.home() / "wallpapers.html"
DAEMON = "http://127.0.0.1:45127"

# Steam sometimes writes project.json mid-download; require a non-empty title/type
# or a file field so we don't index half-downloaded packs.
MIN_DIR_BYTES = 64


def _read_json(path: Path) -> dict[str, Any]:
    try:
        raw = path.read_text(encoding="utf-8", errors="ignore")
        data = json.loads(raw)
        return data if isinstance(data, dict) else {}
    except Exception:
        return {}


def _normalize_tags(tags: Any) -> list[str]:
    out: list[str] = []
    if not tags:
        return out
    if isinstance(tags, str):
        return [tags] if tags.strip() else []
    if isinstance(tags, list):
        for t in tags:
            if isinstance(t, str) and t.strip():
                out.append(t.strip())
            elif isinstance(t, dict):
                # rare: {"category":"Anime"} or {"label":"..."}
                for k in ("label", "value", "category", "name"):
                    if isinstance(t.get(k), str) and t[k].strip():
                        out.append(t[k].strip())
                        break
    return out


def _find_preview(d: Path, data: dict[str, Any]) -> str:
    candidates = []
    if data.get("preview"):
        candidates.append(str(data["preview"]))
    candidates += [
        "preview.jpg",
        "preview.jpeg",
        "preview.png",
        "preview.gif",
        "preview.webp",
    ]
    for name in candidates:
        p = d / name
        if p.is_file() and p.stat().st_size > 0:
            return str(p.resolve())
    return ""


_AUDIO_EXT = {".mp3", ".ogg", ".wav", ".flac", ".m4a", ".opus", ".aac"}


def _dir_size_bytes(d: Path) -> int:
    total = 0
    try:
        for root, _dirs, files in os.walk(d):
            for name in files:
                try:
                    total += (Path(root) / name).stat().st_size
                except OSError:
                    pass
    except OSError:
        pass
    return total


def _fmt_size(n: int) -> str:
    if n < 1024:
        return f"{n} B"
    for unit, div in (("KB", 1024), ("MB", 1024**2), ("GB", 1024**3)):
        if n < div * 1024 or unit == "GB":
            return f"{n / div:.1f} {unit}" if unit != "KB" else f"{n / div:.0f} {unit}"
    return f"{n} B"


def _has_audio(d: Path, wtype: str, file_field: str) -> bool:
    # video wallpapers almost always carry audio track
    if wtype == "video":
        return True
    try:
        for p in d.rglob("*"):
            if p.is_file() and p.suffix.lower() in _AUDIO_EXT:
                return True
    except OSError:
        pass
    # file field sometimes points at media
    if file_field and Path(file_field).suffix.lower() in _AUDIO_EXT:
        return True
    return False


def parse_dir(d: Path) -> dict[str, Any] | None:
    """Parse one workshop folder. Returns None if not ready / not a wallpaper."""
    if not d.is_dir() or not d.name.isdigit():
        return None
    pj = d / "project.json"
    if not pj.is_file():
        return None
    try:
        st = pj.stat()
        if st.st_size < MIN_DIR_BYTES:
            return None
        mtime = st.st_mtime
    except OSError:
        return None

    data = _read_json(pj)
    if not data:
        return None

    wtype = str(data.get("type") or "?").strip().lower()
    title = (data.get("title") or "").strip() or d.name
    # still-downloading stub sometimes has empty type/file
    if wtype in ("", "?") and not data.get("file"):
        return None

    file_field = data.get("file") or ""
    size_b = _dir_size_bytes(d)
    desc = data.get("description") or ""
    if isinstance(desc, str):
        desc = desc.strip()
    else:
        desc = ""

    return {
        "id": d.name,
        "title": title,
        "type": wtype,
        "tags": _normalize_tags(data.get("tags")),
        "preview": _find_preview(d, data),
        "dir": str(d.resolve()),
        "mtime": mtime,
        "file": file_field,
        "description": desc,
        "size_bytes": size_b,
        "size": _fmt_size(size_b),
        "audio": _has_audio(d, wtype, str(file_field)),
    }


def load_cache() -> list[dict[str, Any]]:
    if not CACHE.exists():
        return []
    try:
        data = json.loads(CACHE.read_text(encoding="utf-8"))
        return data if isinstance(data, list) else []
    except Exception:
        return []


def save_cache(items: list[dict[str, Any]]) -> None:
    CACHE.parent.mkdir(parents=True, exist_ok=True)
    # stable order: newest mtime first, then id
    items = sorted(items, key=lambda x: (-float(x.get("mtime") or 0), x["id"]))
    CACHE.write_text(json.dumps(items, ensure_ascii=False, indent=1), encoding="utf-8")


def collect_full() -> list[dict[str, Any]]:
    items: list[dict[str, Any]] = []
    if not WORKSHOP.exists():
        return items
    for d in WORKSHOP.iterdir():
        it = parse_dir(d)
        if it:
            items.append(it)
    return items


def rescan(prev: list[dict[str, Any]] | None = None) -> dict[str, Any]:
    """Incremental scan. Re-parses dirs whose project.json mtime changed or is new.

    Returns:
      {
        items: [...],
        added: [id,...],
        removed: [id,...],
        updated: [id,...],
        skipped_incomplete: [id,...],  # dirs present but not parseable yet
      }
    """
    if prev is None:
        prev = load_cache()
    by_id = {it["id"]: it for it in prev}

    present_ids: set[str] = set()
    incomplete: list[str] = []
    added: list[str] = []
    updated: list[str] = []
    new_items: list[dict[str, Any]] = []

    if WORKSHOP.exists():
        for d in WORKSHOP.iterdir():
            if not d.is_dir() or not d.name.isdigit():
                continue
            present_ids.add(d.name)
            pj = d / "project.json"
            if not pj.is_file():
                incomplete.append(d.name)
                # keep old entry if we had one (Steam temporarily missing file?)
                if d.name in by_id:
                    new_items.append(by_id[d.name])
                continue

            try:
                mtime = pj.stat().st_mtime
            except OSError:
                incomplete.append(d.name)
                if d.name in by_id:
                    new_items.append(by_id[d.name])
                continue

            old = by_id.get(d.name)
            if old and abs(float(old.get("mtime") or 0) - mtime) < 0.001 and old.get("title"):
                # unchanged — keep cached entry (cheap path)
                new_items.append(old)
                continue

            parsed = parse_dir(d)
            if not parsed:
                incomplete.append(d.name)
                if old:
                    new_items.append(old)
                continue

            new_items.append(parsed)
            if old is None:
                added.append(d.name)
            else:
                # content changed
                if (
                    old.get("title") != parsed["title"]
                    or old.get("type") != parsed["type"]
                    or old.get("preview") != parsed["preview"]
                    or old.get("tags") != parsed["tags"]
                ):
                    updated.append(d.name)

    removed = sorted(set(by_id) - present_ids)
    save_cache(new_items)
    return {
        "items": new_items,
        "added": sorted(added),
        "removed": removed,
        "updated": sorted(updated),
        "skipped_incomplete": sorted(incomplete),
        "count": len(new_items),
        "scanned_at": time.time(),
    }


def rebuild() -> list[dict[str, Any]]:
    items = collect_full()
    save_cache(items)
    return items


def list_items(items: list[dict[str, Any]]) -> None:
    for it in items:
        print(f"{it['title']} [{it['type']}] {it['id']}")


def gallery(items: list[dict[str, Any]]) -> Path:
    """Install the designed gallery shell next to the user home.

    The HTML lives in the same directory as this script (tooling/wallpapers.html)
    and talks to the daemon live — we only copy it, never regenerate design.
    """
    src = Path(__file__).resolve().parent / "wallpapers.html"
    # when installed as ~/.local/bin/lwe-index.py, design lives in the project
    candidates = [
        src,
        Path.home() / "AAAcode/wpengine/tooling/wallpapers.html",
        Path.home() / "wallpapers.html",
    ]
    for c in candidates:
        if c.is_file() and c.stat().st_size > 1000:
            if c.resolve() != GALLERY.resolve():
                GALLERY.write_text(c.read_text(encoding="utf-8"), encoding="utf-8")
            return GALLERY
    raise FileNotFoundError("wallpapers.html design not found; expected next to lwe-index or in ~/AAAcode/wpengine/tooling/")


def main() -> None:
    mode = sys.argv[1] if len(sys.argv) > 1 else "--list"
    if mode == "--rebuild":
        items = rebuild()
        print(f"indexed {len(items)} wallpapers -> {CACHE}")
    elif mode == "--rescan":
        result = rescan()
        print(
            f"rescan: total={result['count']} "
            f"+{len(result['added'])} -{len(result['removed'])} "
            f"~{len(result['updated'])} incomplete={len(result['skipped_incomplete'])}"
        )
        if result["added"]:
            print("added:", ", ".join(result["added"]))
        if result["removed"]:
            print("removed:", ", ".join(result["removed"]))
        if result["updated"]:
            print("updated:", ", ".join(result["updated"]))
        if result["skipped_incomplete"]:
            print("incomplete (no project.json yet):", ", ".join(result["skipped_incomplete"]))
    elif mode == "--list":
        items = load_cache() or rebuild()
        list_items(items)
    elif mode == "--json":
        items = load_cache() or rebuild()
        print(json.dumps(items, ensure_ascii=False, indent=1))
    elif mode == "--gallery":
        result = rescan()
        g = gallery(result["items"])
        print(f"gallery -> {g} ({result['count']} wallpapers)")
        if result["added"]:
            print("new:", ", ".join(result["added"]))
    else:
        print("unknown mode; use --rebuild|--rescan|--gallery|--list|--json", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
