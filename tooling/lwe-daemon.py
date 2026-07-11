#!/usr/bin/env python3
"""Local control daemon for the wallpaper gallery.

http://127.0.0.1:45127
  GET  /list
  GET  /rescan
  GET  /launch?id=ID[&volume=&fps=&silent=0|1&scaling=&no_fullscreen_pause=0|1&disable_mouse=0|1&noautomute=0|1]
  GET  /stop
  GET  /current
  GET  /health
  GET  /settings
  GET  /settings/set?...   (persist playback defaults)
  GET  /open?id=ID         (xdg-open workshop folder)
  GET  /monitors
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

INDEX_SCRIPT = os.path.expanduser("~/.local/bin/lwe-index.py")
CACHE = os.path.expanduser("~/.cache/lwe_index.json")
SETTINGS_PATH = os.path.expanduser("~/.cache/lwe_settings.json")
PORT = 45127
AUTO_RESCAN_SEC = 45

DEFAULT_SETTINGS = {
    "volume": 15,  # engine default
    "fps": 30,
    "silent": False,
    "scaling": "fill",  # stretch|fit|fill|default
    "no_fullscreen_pause": False,  # False = pause on fullscreen (engine default)
    "disable_mouse": False,
    "noautomute": False,
}

RUNNING = {"id": None, "proc": None, "opts": {}}
STATE = {
    "items": [],
    "last_rescan": None,
    "settings": dict(DEFAULT_SETTINGS),
    "lock": threading.Lock(),
}


def _load_index_module():
    import importlib.util

    spec = importlib.util.spec_from_file_location("lwe_index", INDEX_SCRIPT)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {INDEX_SCRIPT}")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


try:
    lwe_index = _load_index_module()
except Exception as e:
    print(f"WARN: failed to load lwe-index.py: {e}", flush=True)
    lwe_index = None


def load_settings() -> dict:
    try:
        with open(SETTINGS_PATH, encoding="utf-8") as f:
            data = json.load(f)
        if isinstance(data, dict):
            out = dict(DEFAULT_SETTINGS)
            out.update({k: data[k] for k in DEFAULT_SETTINGS if k in data})
            return out
    except Exception:
        pass
    return dict(DEFAULT_SETTINGS)


def save_settings(s: dict) -> None:
    os.makedirs(os.path.dirname(SETTINGS_PATH), exist_ok=True)
    with open(SETTINGS_PATH, "w", encoding="utf-8") as f:
        json.dump(s, f, indent=1)


def resolve_wayland():
    import glob

    xdg = os.environ.get("XDG_RUNTIME_DIR") or f"/run/user/{os.getuid()}"
    candidates = sorted(glob.glob(os.path.join(xdg, "wayland-*")))
    wd = os.environ.get("WAYLAND_DISPLAY")
    if wd and os.path.exists(os.path.join(xdg, wd)):
        return wd
    for c in candidates:
        base = os.path.basename(c)
        if os.path.exists(c):
            return base
    return None


def make_env(opts: dict | None = None):
    env = os.environ.copy()
    wd = resolve_wayland()
    if wd:
        env["WAYLAND_DISPLAY"] = wd
    o = opts or {}
    if o.get("fps") is not None:
        env["LWE_FPS"] = str(int(o["fps"]))
    if o.get("silent"):
        env["LWE_SILENT"] = "1"
    elif o.get("volume") is not None:
        env["LWE_VOLUME"] = str(int(o["volume"]))
        env.pop("LWE_SILENT", None)
    if o.get("scaling"):
        env["LWE_SCALING"] = str(o["scaling"])
    env["LWE_NO_FULLSCREEN_PAUSE"] = "1" if o.get("no_fullscreen_pause") else "0"
    env["LWE_DISABLE_MOUSE"] = "1" if o.get("disable_mouse") else "0"
    env["LWE_NOAUTOMUTE"] = "1" if o.get("noautomute") else "0"
    return env


def load_index_disk():
    try:
        with open(CACHE, encoding="utf-8") as f:
            data = json.load(f)
        return data if isinstance(data, list) else []
    except Exception:
        return []


def get_items():
    with STATE["lock"]:
        if STATE["items"]:
            return list(STATE["items"])
    items = load_index_disk()
    with STATE["lock"]:
        STATE["items"] = items
    return list(items)


def do_rescan() -> dict:
    if lwe_index is None:
        try:
            subprocess.run(
                [sys.executable, INDEX_SCRIPT, "--rescan"],
                check=False,
                capture_output=True,
                text=True,
                timeout=120,
            )
        except Exception as e:
            return {"ok": False, "error": str(e)}
        items = load_index_disk()
        with STATE["lock"]:
            STATE["items"] = items
        return {
            "ok": True,
            "items": items,
            "added": [],
            "removed": [],
            "updated": [],
            "skipped_incomplete": [],
            "count": len(items),
        }

    with STATE["lock"]:
        prev = list(STATE["items"]) if STATE["items"] else load_index_disk()
    result = lwe_index.rescan(prev)
    with STATE["lock"]:
        STATE["items"] = result["items"]
        STATE["last_rescan"] = result
    return {
        "ok": True,
        "items": result["items"],
        "added": result["added"],
        "removed": result["removed"],
        "updated": result["updated"],
        "skipped_incomplete": result["skipped_incomplete"],
        "count": result["count"],
        "scanned_at": result.get("scanned_at"),
    }


def _parse_bool(v, default=False):
    if v is None:
        return default
    return str(v).lower() in ("1", "true", "yes", "on")


def opts_from_query(q: dict) -> dict:
    with STATE["lock"]:
        base = dict(STATE["settings"])
    # overrides from query
    if "volume" in q:
        try:
            base["volume"] = max(0, min(100, int(q["volume"][0])))
        except ValueError:
            pass
    if "fps" in q:
        try:
            base["fps"] = max(1, min(240, int(q["fps"][0])))
        except ValueError:
            pass
    if "silent" in q:
        base["silent"] = _parse_bool(q["silent"][0])
    if "scaling" in q:
        sc = q["scaling"][0].lower()
        if sc in ("stretch", "fit", "fill", "default"):
            base["scaling"] = sc
    if "no_fullscreen_pause" in q:
        base["no_fullscreen_pause"] = _parse_bool(q["no_fullscreen_pause"][0])
    if "disable_mouse" in q:
        base["disable_mouse"] = _parse_bool(q["disable_mouse"][0])
    if "noautomute" in q:
        base["noautomute"] = _parse_bool(q["noautomute"][0])
    # volume 0 → silent
    if base.get("volume", 15) == 0:
        base["silent"] = True
    return base


def kill_engine_strays():
    """Kill only real engine processes (by /proc/pid/exe), never shell/scripts."""
    try:
        for name in os.listdir("/proc"):
            if not name.isdigit():
                continue
            try:
                exe = os.readlink(f"/proc/{name}/exe")
            except OSError:
                continue
            # deleted binaries show as 'path (deleted)'
            if "linux-wallpaperengine" in os.path.basename(exe).replace(" (deleted)", ""):
                if "/opt/linux-wallpaperengine/" in exe or exe.endswith("linux-wallpaperengine"):
                    try:
                        os.kill(int(name), 15)
                    except OSError:
                        pass
    except Exception:
        pass


def launch(id_: str, opts: dict | None = None):
    o = opts if opts is not None else load_settings()
    RUNNING["id"] = id_
    RUNNING["opts"] = o
    if RUNNING["proc"] and RUNNING["proc"].poll() is None:
        RUNNING["proc"].terminate()
        try:
            RUNNING["proc"].wait(timeout=5)
        except Exception:
            RUNNING["proc"].kill()
    kill_engine_strays()
    log = open(os.path.expanduser("~/.cache/lwe_launch.log"), "ab", buffering=0)
    RUNNING["proc"] = subprocess.Popen(
        ["lwe-launch", id_],
        env=make_env(o),
        stdout=log,
        stderr=log,
        stdin=subprocess.DEVNULL,
    )


def open_folder(id_: str) -> dict:
    items = get_items()
    path = None
    for it in items:
        if it.get("id") == id_:
            path = it.get("dir")
            break
    if not path:
        # fallback direct workshop path
        path = os.path.expanduser(
            f"~/.local/share/Steam/steamapps/workshop/content/431960/{id_}"
        )
    if not os.path.isdir(path):
        return {"ok": False, "error": "folder not found", "path": path}
    try:
        subprocess.Popen(
            ["xdg-open", path],
            env=make_env(),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            stdin=subprocess.DEVNULL,
        )
        return {"ok": True, "path": path}
    except Exception as e:
        return {"ok": False, "error": str(e), "path": path}


def list_monitors() -> list:
    try:
        out = subprocess.check_output(
            ["hyprctl", "-j", "monitors"],
            env=make_env(),
            text=True,
            timeout=3,
        )
        mons = json.loads(out)
        return [
            {
                "name": m.get("name"),
                "width": m.get("width"),
                "height": m.get("height"),
                "id": m.get("id"),
            }
            for m in mons
        ]
    except Exception:
        return []


def auto_rescan_loop():
    time.sleep(2)
    while True:
        try:
            result = do_rescan()
            if result.get("ok") and result.get("added"):
                print(
                    f"[auto-rescan] +{len(result['added'])} new: "
                    f"{', '.join(result['added'][:8])}",
                    flush=True,
                )
        except Exception as e:
            print(f"[auto-rescan] error: {e}", flush=True)
        time.sleep(AUTO_RESCAN_SEC)


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _send(self, code, body, ctype="text/plain"):
        if isinstance(body, (dict, list)):
            body = json.dumps(body, ensure_ascii=False)
            ctype = "application/json"
        data = body.encode("utf-8") if isinstance(body, str) else body
        try:
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Access-Control-Allow-Origin", "*")
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def do_OPTIONS(self):
        try:
            self.send_response(204)
            self.send_header("Access-Control-Allow-Origin", "*")
            self.send_header("Access-Control-Allow-Methods", "GET, OPTIONS")
            self.send_header("Access-Control-Allow-Headers", "*")
            self.end_headers()
        except (BrokenPipeError, ConnectionResetError):
            pass

    def do_GET(self):
        u = urlparse(self.path)
        q = parse_qs(u.query)
        if u.path == "/list":
            self._send(200, get_items())
        elif u.path == "/rescan":
            try:
                self._send(200, do_rescan())
            except Exception as e:
                self._send(500, {"ok": False, "error": str(e)})
        elif u.path == "/launch":
            id_ = q.get("id", [""])[0]
            if not id_:
                self._send(400, {"ok": False, "error": "missing id"})
                return
            try:
                opts = opts_from_query(q)
                # persist last-used opts
                with STATE["lock"]:
                    STATE["settings"] = opts
                save_settings(opts)
                launch(id_, opts)
                self._send(200, {"ok": True, "id": id_, "opts": opts})
            except Exception as e:
                self._send(500, {"ok": False, "error": str(e)})
        elif u.path == "/stop":
            if RUNNING["proc"] and RUNNING["proc"].poll() is None:
                RUNNING["proc"].terminate()
                try:
                    RUNNING["proc"].wait(timeout=3)
                except Exception:
                    try:
                        RUNNING["proc"].kill()
                    except Exception:
                        pass
            kill_engine_strays()
            RUNNING["id"] = None
            RUNNING["proc"] = None
            self._send(200, {"ok": True})
        elif u.path == "/current":
            self._send(200, {"id": RUNNING["id"], "opts": RUNNING.get("opts") or {}})
        elif u.path == "/health":
            self._send(
                200,
                {
                    "ok": True,
                    "count": len(get_items()),
                    "current": RUNNING["id"],
                },
            )
        elif u.path == "/settings":
            with STATE["lock"]:
                s = dict(STATE["settings"])
            self._send(200, {"ok": True, "settings": s})
        elif u.path == "/settings/set":
            try:
                opts = opts_from_query(q)
                with STATE["lock"]:
                    STATE["settings"] = opts
                save_settings(opts)
                self._send(200, {"ok": True, "settings": opts})
            except Exception as e:
                self._send(500, {"ok": False, "error": str(e)})
        elif u.path == "/open":
            id_ = q.get("id", [""])[0]
            if not id_:
                self._send(400, {"ok": False, "error": "missing id"})
                return
            self._send(200, open_folder(id_))
        elif u.path == "/monitors":
            self._send(200, {"ok": True, "monitors": list_monitors()})
        else:
            self._send(404, {"ok": False, "error": "not found"})


def main():
    with STATE["lock"]:
        STATE["items"] = load_index_disk()
        STATE["settings"] = load_settings()

    t = threading.Thread(target=auto_rescan_loop, name="auto-rescan", daemon=True)
    t.start()

    srv = ThreadingHTTPServer(("127.0.0.1", PORT), H)
    print(f"lwe-daemon listening on http://127.0.0.1:{PORT}", flush=True)
    print(f"auto-rescan every {AUTO_RESCAN_SEC}s", flush=True)
    srv.serve_forever()


if __name__ == "__main__":
    main()
