#!/usr/bin/env python3
"""App Fabrikası VDS Köprü Sunucusu (sıfır dış bağımlılık, salt standart Python 3 kütüphanesi).

AppFabrikasi koduna dokunmadan, VDS veya yerel makinede çalışarak masaüstü uygulamasına
HTTP JSON API sunar.

Kullanım:
  python3 kopru.py --root /path/to/AppFabrikasi --host 0.0.0.0 --port 8787 --token GIZLI_ANAHTAR
"""
import argparse
import json
import os
import re
import sqlite3
import subprocess
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

AGENTS = [
    "yonetici", "arastirmaci", "tasarim_lideri", "tasarimci", "gorsel_ajani",
    "yazilim_lideri", "test_lideri", "kod_ajani", "test_ajani",
]
QUOTA_POOLS = [
    ("claude", "Claude"),
    ("codex", "Codex"),
    ("agy_gemini", "Gemini"),
    ("agy_claude_gpt", "agy Claude/GPT"),
]
WIKILINK_RE = re.compile(r"\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|[^\]]+)?\]\]")


def tail_lines(path: Path, max_lines=150, max_bytes=48_000):
    try:
        with open(path, "rb") as f:
            f.seek(0, 2)
            size = f.tell()
            start = max(0, size - max_bytes)
            f.seek(start)
            lines = f.read().decode(errors="replace").splitlines()
            if start > 0 and lines:
                lines = lines[1:]
            return lines[-max_lines:]
    except Exception:
        return []


def read_assigned_models(root: Path):
    out = {}
    try:
        cfg = json.loads((root / "config.json").read_text())["ajanlar"]
    except Exception:
        return out
    order = ["low", "medium", "high", "xhigh", "max"]
    for rol, a in cfg.items():
        efor = a.get("efor")
        if isinstance(efor, dict):
            efor = "/".join(sorted(set(efor.values()), key=lambda x: order.index(x) if x in order else 99))
        out[rol] = f"{a.get('cli', '')}·{a.get('model', '')}" + (f" ({efor})" if efor else "")
    return out


def parse_project(ad: str, klasor: str):
    d_path = Path(os.path.expanduser(klasor)) / "durum.json"
    if not d_path.exists():
        return None
    try:
        d = json.loads(d_path.read_text())
    except Exception:
        return None
    gorevler = []
    for gid, gv in (d.get("gorevler") or {}).items():
        gorevler.append({
            "id": gid,
            "adim": gv.get("adim", "bekliyor"),
            "deneme": gv.get("deneme", 0),
            "incelendi": bool(gv.get("incelendi", False)),
            "son_hata": gv.get("son_hata"),
            "inceleme_duzeltmeleri": gv.get("inceleme_duzeltmeleri") or [],
        })
    gorevler.sort(key=lambda t: int(re.sub(r"\D", "", t["id"]) or 9999))
    return {
        "ad": d.get("uygulama_adi") or ad,
        "klasor": klasor,
        "asama": d.get("asama", ""),
        "asama_adi": d.get("asama_adi", "?"),
        "su_an": d.get("su_an") or None,
        "expo_url": d.get("expo_url"),
        "figma_url": d.get("figma_url"),
        "gorevler": gorevler,
        "gorseller": d.get("gorseller") or [],
        "olusturma": d.get("olusturma"),
    }


def service_status():
    try:
        r = subprocess.run(
            ["systemctl", "--user", "is-active", "app-fabrikasi.service"],
            capture_output=True, text=True, timeout=3,
        )
        return r.stdout.strip() or "unknown"
    except Exception:
        return "unknown"


def build_snapshot(root: Path):
    ts = time.time()
    db_path = root / "kayitlar" / "durum.db"
    canli = root / "kayitlar" / "canli"
    durum, jobs, messages, err = {}, [], [], None

    try:
        b = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True, timeout=2)
        b.row_factory = sqlite3.Row
        durum = {r["anahtar"]: json.loads(r["deger"]) for r in b.execute("SELECT anahtar, deger FROM durum")}
        jobs = [dict(r) for r in b.execute(
            "SELECT id, tur, kuyruk, durum, deneme, sonraki_deneme, hata FROM isler WHERE durum NOT IN ('tamam','iptal') ORDER BY id"
        )]
        messages = [dict(r) for r in reversed(b.execute(
            "SELECT id, yon, metin, zaman, kanal, kaynak FROM mesajlar ORDER BY id DESC LIMIT 80"
        ).fetchall())]
        b.close()
    except Exception as e:
        err = f"durum.db okunamadı: {e}"

    quotas = []
    for qid, qname in QUOTA_POOLS:
        k = durum.get(f"kota:{qid}")
        if k:
            p5_s, hf_s = float(k.get("p5_sifir", 0)), float(k.get("hafta_sifir", 0))
            quotas.append({
                "id": qid, "name": qname, "has_data": True,
                "p5": float(k.get("p5", 0)) if p5_s > ts else 0.0,
                "hafta": float(k.get("hafta", 0)) if hf_s > ts else 0.0,
                "p5_sifir": p5_s, "hafta_sifir": hf_s,
                "guncelleme": float(k.get("guncelleme", ts)),
            })
        else:
            quotas.append({
                "id": qid, "name": qname, "has_data": False,
                "p5": 0.0, "hafta": 0.0, "p5_sifir": 0.0, "hafta_sifir": 0.0, "guncelleme": 0.0,
            })

    all_projects = []
    for p in reversed(durum.get("projeler", [])):
        info = parse_project(p.get("ad", ""), p.get("klasor", ""))
        if info:
            all_projects.append(info)

    try:
        raw_aktif = json.loads((canli / "aktif.json").read_text())
        active_agents = {
            k: {
                "etiket": v.get("etiket", ""), "model": v.get("model", ""), "cli": v.get("cli", ""),
                "baslangic": float(v.get("baslangic", ts)), "muadil": bool(v.get("muadil", False)),
                "aciklama": v.get("aciklama", ""),
            }
            for k, v in raw_aktif.items()
        }
    except Exception:
        active_agents = {}

    try:
        mc = json.loads((canli / "yonetici-baglam.json").read_text())
        manager_context = {
            "token": int(mc.get("token", 0)),
            "pencere": int(mc.get("pencere") or 1_000_000),
            "zaman": float(mc.get("zaman", ts)),
            "oturum": mc.get("oturum"),
            "oturum_baslangic": mc.get("oturum_baslangic"),
            "son_ozet": mc.get("son_ozet"),
        }
    except Exception:
        manager_context = None

    return {
        "connected": err is None,
        "error": err,
        "profile_name": "VDS",
        "mode": "remote",
        "service_status": service_status(),
        "paused": bool(durum.get("duraklatildi", False)),
        "manager_session": durum.get("yonetici_oturum"),
        "message_queue_count": sum(1 for j in jobs if j["tur"] == "mesaj"),
        "manager_context": manager_context,
        "quotas": quotas,
        "active_agents": active_agents,
        "assigned_models": read_assigned_models(root),
        "agent_logs": {r: tail_lines(canli / f"{r}.log", 60, 24_000) for r in AGENTS},
        "events_log": tail_lines(canli / "olaylar.log", 100, 48_000),
        "system_log": tail_lines(canli / "sistem.log", 60, 24_000),
        "messages": messages,
        "jobs": jobs,
        "active_project": all_projects[0] if all_projects else None,
        "all_projects": all_projects,
        "timestamp": ts,
    }


def scan_graph(root: Path):
    hafiza = root / "hafiza"
    colors = {"dersler": "#3b7ee6", "hatalar": "#ef4444", "projeler": "#22c55e", "sahip": "#f59e0b"}
    show_arrow = True
    try:
        gj = json.loads((hafiza / ".obsidian" / "graph.json").read_text())
        show_arrow = bool(gj.get("showArrow", True))
        for g in gj.get("colorGroups", []):
            q = g.get("query", "")
            rgb = (g.get("color") or {}).get("rgb")
            if rgb is not None:
                hx = f"#{int(rgb) & 0xFFFFFF:06x}"
                if "path:dersler" in q: colors["dersler"] = hx
                elif "path:hatalar" in q: colors["hatalar"] = hx
                elif "path:projeler" in q: colors["projeler"] = hx
                elif "file:sahip" in q: colors["sahip"] = hx
    except Exception:
        pass

    files = sorted(p for p in hafiza.rglob("*.md") if not any(part.startswith(".") for part in p.relative_to(hafiza).parts))
    stem_to_id, records = {}, []
    for p in files:
        rel = p.relative_to(hafiza).as_posix()
        nid = rel[:-3]
        stem_to_id.setdefault(p.stem, nid)
        stem_to_id[nid] = nid
        records.append((nid, p.stem, rel, p))

    edges, seen, degree = [], set(), {}
    for nid, _, _, p in records:
        txt = p.read_text(errors="replace")
        for m in WIKILINK_RE.finditer(txt):
            tgt = stem_to_id.get(m.group(1).strip())
            if tgt and tgt != nid and (nid, tgt) not in seen:
                seen.add((nid, tgt))
                edges.append({"source": nid, "target": tgt})
                degree[nid] = degree.get(nid, 0) + 1
                degree[tgt] = degree.get(tgt, 0) + 1

    owner = {"sahip", "kararlar", "tasarim-dili", "arastirma-tercihleri", "acik_konular", "projeler"}
    nodes = []
    for nid, stem, rel, _ in records:
        if rel.startswith("dersler/"): grp, col = "Dersler", colors["dersler"]
        elif rel.startswith("hatalar/"): grp, col = "Hatalar", colors["hatalar"]
        elif rel.startswith("projeler/"): grp, col = "Projeler", colors["projeler"]
        elif stem in owner: grp, col = "Sahip & Kurallar", colors["sahip"]
        else: grp, col = "Genel", "#94a3b8"
        nodes.append({
            "id": nid, "label": stem, "path": rel,
            "folder": rel.split("/")[0] if "/" in rel else "kök",
            "group": grp, "color": col, "degree": degree.get(nid, 0),
        })
    groups = [
        {"name": "Dersler", "color": colors["dersler"]},
        {"name": "Hatalar", "color": colors["hatalar"]},
        {"name": "Projeler", "color": colors["projeler"]},
        {"name": "Sahip & Kurallar", "color": colors["sahip"]},
        {"name": "Genel", "color": "#94a3b8"},
    ]
    return {"nodes": nodes, "edges": edges, "groups": groups, "show_arrow": show_arrow}


def _safe_project_dir(root: Path, raw_dir: str):
    if not raw_dir:
        return None
    p = Path(os.path.expanduser(raw_dir)).resolve()
    if not p.is_dir():
        return None
    home = Path.home().resolve()
    if p.is_relative_to(root) or p.is_relative_to(home):
        return p
    return None


def read_project_file_api(root: Path, raw_dir: str, rel_path: str):
    proj = _safe_project_dir(root, raw_dir)
    if not proj:
        return {"ok": False, "path": rel_path, "lines": [], "total_lines": 0, "truncated": False, "error": "Proje klasörü bulunamadı."}
    clean = rel_path.strip().lstrip("./").replace("\\", "/")
    if not clean or ".." in clean:
        return {"ok": False, "path": rel_path, "lines": [], "total_lines": 0, "truncated": False, "error": "Geçersiz dosya yolu."}
    cand = Path(clean) if clean.startswith("/") else (proj / clean)
    try:
        target = cand.resolve()
    except Exception:
        return {"ok": False, "path": clean, "lines": [], "total_lines": 0, "truncated": False, "error": "Dosya çözümlenemedi."}
    if not target.is_relative_to(proj) or not target.is_file():
        return {"ok": False, "path": clean, "lines": [], "total_lines": 0, "truncated": False, "error": f"Dosya bulunamadı: {clean}"}
    rel_display = target.relative_to(proj).as_posix()
    try:
        size = target.stat().st_size
        with open(target, "rb") as f:
            raw = f.read(256_000)
        all_lines = raw.decode(errors="replace").splitlines()
        total = len(all_lines)
        truncated = size > 256_000 or total > 1500
        return {
            "ok": True,
            "path": rel_display,
            "lines": all_lines[:1500],
            "total_lines": total,
            "truncated": truncated,
            "error": None,
        }
    except Exception as e:
        return {"ok": False, "path": rel_display, "lines": [], "total_lines": 0, "truncated": False, "error": str(e)}


def read_project_diff_api(root: Path, raw_dir: str, rel_path: str = ""):
    proj = _safe_project_dir(root, raw_dir)
    if not proj:
        return {"ok": False, "path": rel_path or None, "lines": [], "changed_files": [], "error": "Proje klasörü bulunamadı."}
    changed = []
    try:
        st = subprocess.run(["git", "-C", str(proj), "status", "--porcelain", "-uall"], capture_output=True, text=True, timeout=5)
        if st.returncode == 0:
            for line in st.stdout.splitlines():
                if len(line) >= 4:
                    fp = line[3:].strip().strip('"')
                    if " -> " in fp:
                        fp = fp.split(" -> ", 1)[1].strip('"')
                    if fp:
                        changed.append(fp)
    except Exception:
        pass
    clean = rel_path.strip().lstrip("./").replace("\\", "/") if rel_path else ""
    if ".." in clean:
        clean = ""
    cmd = ["git", "-C", str(proj), "diff", "--no-color", "-U3"]
    if clean:
        cmd.extend(["--", clean])
    diff_lines = []
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=5)
        if r.returncode == 0:
            diff_lines = r.stdout.splitlines()[:1200]
    except Exception:
        pass
    if not diff_lines and clean:
        fres = read_project_file_api(root, raw_dir, clean)
        if fres["ok"] and fres["lines"]:
            cnt = min(len(fres["lines"]), 400)
            diff_lines = ["--- /dev/null", f"+++ b/{fres['path']}", f"@@ -0,0 +1,{cnt} @@"] + [
                f"+{l}" for l in fres["lines"][:400]
            ]
    return {"ok": True, "path": clean or None, "lines": diff_lines, "changed_files": changed, "error": None}


def make_handler(root: Path, token: str):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, fmt, *args):
            pass

        def _auth_ok(self):
            if not token:
                return True
            return self.headers.get("Authorization") == f"Bearer {token}"

        def _send_json(self, data, status=200):
            body = json.dumps(data, ensure_ascii=False).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self):
            if not self._auth_ok():
                return self._send_json({"error": "Yetkisiz"}, 401)
            u = urlparse(self.path)
            q = parse_qs(u.query)
            if u.path == "/api/snapshot":
                return self._send_json(build_snapshot(root))
            if u.path.startswith("/api/logs/"):
                name = u.path[len("/api/logs/"):]
                if ".." in name or "/" in name:
                    return self._send_json([], 400)
                lines = int(q.get("lines", ["1500"])[0])
                return self._send_json(tail_lines(root / "kayitlar" / "canli" / f"{name}.log", lines, 512_000))
            if u.path == "/api/graph":
                return self._send_json(scan_graph(root))
            if u.path == "/api/note":
                nid = q.get("id", [""])[0]
                if not nid or ".." in nid or nid.startswith(("/", "\\")):
                    return self._send_json({"error": "Geçersiz not"}, 400)
                rel = nid if nid.endswith(".md") else f"{nid}.md"
                hafiza_root = (root / "hafiza").resolve()
                p = (hafiza_root / rel).resolve()
                if not p.is_relative_to(hafiza_root):
                    return self._send_json({"error": "Geçersiz not"}, 400)
                if not p.exists():
                    return self._send_json({"error": "Not bulunamadı"}, 404)
                g = scan_graph(root)
                cid = rel[:-3]
                return self._send_json({
                    "id": cid, "path": rel, "title": p.stem,
                    "folder": rel.split("/")[0] if "/" in rel else "kök",
                    "content": p.read_text(errors="replace"),
                    "links": [e["target"] for e in g["edges"] if e["source"] == cid],
                    "backlinks": [e["source"] for e in g["edges"] if e["target"] == cid],
                })
            if u.path == "/api/archives":
                arsiv = root / "kayitlar" / "canli-arsiv"
                items = []
                if arsiv.exists():
                    for d in sorted((x for x in arsiv.iterdir() if x.is_dir()), reverse=True):
                        items.append({"name": d.name, "files": sorted(f.name for f in d.glob("*.log"))})
                return self._send_json({
                    "archives": items,
                    "journal": tail_lines(root / "kayitlar" / "yonetici-gunlugu.md", 400, 128_000),
                })
            if u.path == "/api/archive-file":
                a, f = q.get("archive", [""])[0], q.get("file", [""])[0]
                if ".." in a or "/" in a or ".." in f or "/" in f:
                    return self._send_json([], 400)
                return self._send_json(tail_lines(root / "kayitlar" / "canli-arsiv" / a / f, 2000, 512_000))
            if u.path == "/api/project-file":
                pdir = q.get("dir", [""])[0]
                rpath = q.get("path", [""])[0]
                return self._send_json(read_project_file_api(root, pdir, rpath))
            if u.path == "/api/project-diff":
                pdir = q.get("dir", [""])[0]
                rpath = q.get("path", [""])[0]
                return self._send_json(read_project_diff_api(root, pdir, rpath))
            self._send_json({"error": "Bulunamadı"}, 404)

        def do_POST(self):
            if not self._auth_ok():
                return self._send_json({"error": "Yetkisiz"}, 401)
            length = int(self.headers.get("Content-Length", "0"))
            body = json.loads(self.rfile.read(length).decode("utf-8") or "{}")
            if self.path == "/api/message":
                txt = (body.get("text") or "").strip()
                if not txt:
                    return self._send_json({"ok": False, "message": "Boş mesaj"}, 400)
                db = sqlite3.connect(root / "kayitlar" / "durum.db", timeout=10)
                ts = time.time()
                cur = db.execute(
                    "INSERT INTO mesajlar(yon, metin, zaman, kanal, kaynak) VALUES('gelen', ?, ?, 'ofis', 'sahip')",
                    (txt, ts),
                )
                mid = cur.lastrowid
                veri = json.dumps({"metin": txt, "kanal": "ofis", "mesaj_id": mid}, ensure_ascii=False)
                db.execute(
                    "INSERT INTO isler(tur, kuyruk, veri, olusturma, guncelleme) VALUES('mesaj', 'hizli', ?, ?, ?)",
                    (veri, ts, ts),
                )
                db.commit()
                db.close()
                return self._send_json({"ok": True, "message": "Mesaj yöneticinin masasında."})
            if self.path == "/api/control":
                act = body.get("action", "")
                if act in ("durdur", "devam", "arastir"):
                    r = subprocess.run(["python3", str(root / "orkestrator" / "fabrika.py"), act],
                                       capture_output=True, text=True, timeout=20)
                    return self._send_json({"ok": r.returncode == 0, "message": (r.stdout or r.stderr).strip() or act})
                if act in ("start", "stop", "restart"):
                    if act == "start": (root / "kayitlar" / "ofis-aciliyor").touch()
                    elif act == "stop": (root / "kayitlar" / "ofis-kapaniyor").touch()
                    r = subprocess.run(["systemctl", "--user", act, "app-fabrikasi.service"],
                                       capture_output=True, text=True, timeout=20)
                    return self._send_json({"ok": r.returncode == 0, "message": f"Servis: {act}" if r.returncode == 0 else r.stderr.strip()})
                return self._send_json({"ok": False, "message": f"Bilinmeyen komut: {act}"}, 400)
            self._send_json({"error": "Bulunamadı"}, 404)

    return Handler


def main():
    p = argparse.ArgumentParser(description="App Fabrikası VDS Köprü Sunucusu")
    p.add_argument("--root", default=os.path.expanduser("~/Desktop/AppFabrikasi"))
    p.add_argument("--host", default="127.0.0.1")
    p.add_argument("--port", type=int, default=8787)
    p.add_argument("--token", default=os.environ.get("AF_BRIDGE_TOKEN", ""))
    args = p.parse_args()
    root = Path(os.path.expanduser(args.root)).resolve()
    srv = ThreadingHTTPServer((args.host, args.port), make_handler(root, args.token))
    print(f"App Fabrikası Köprüsü dinliyor: http://{args.host}:{args.port} (kök: {root})")
    srv.serve_forever()


if __name__ == "__main__":
    main()
