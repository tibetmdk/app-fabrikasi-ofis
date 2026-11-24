use crate::models::{
    ActionResult, ActiveAgentInfo, ArchiveOverview, ArchiveProject, ChatMessage, JobItem,
    ManagerContext, OfficeSnapshot, ProjectDiffResult, ProjectFileResult, ProjectInfo, ProjectTask,
    QuotaPool,
};
use rusqlite::{Connection, OpenFlags};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct LocalCache {
    db: Option<(PathBuf, Connection)>,
    config_models: Option<(PathBuf, SystemTime, BTreeMap<String, String>)>,
    service_status: Option<(Instant, String)>,
    log_tails: HashMap<PathBuf, (SystemTime, u64, Vec<String>)>,
}

impl Default for LocalCache {
    fn default() -> Self {
        Self {
            db: None,
            config_models: None,
            service_status: None,
            log_tails: HashMap::new(),
        }
    }
}

fn local_cache() -> &'static Mutex<LocalCache> {
    static CACHE: OnceLock<Mutex<LocalCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(LocalCache::default()))
}

const AGENTS: &[&str] = &[
    "yonetici",
    "arastirmaci",
    "tasarim_lideri",
    "tasarimci",
    "gorsel_ajani",
    "yazilim_lideri",
    "test_lideri",
    "kod_ajani",
    "test_ajani",
];

const QUOTA_POOLS: &[(&str, &str)] = &[
    ("claude", "Claude"),
    ("codex", "Codex"),
    ("agy_gemini", "Gemini"),
    ("agy_claude_gpt", "agy Claude/GPT"),
];

fn now_ts() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn expand_tilde(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(p)
}

pub fn tail_lines(path: &Path, max_lines: usize, max_bytes: u64) -> Vec<String> {
    let Ok(mut f) = File::open(path) else {
        return Vec::new();
    };
    let Ok(len) = f.seek(SeekFrom::End(0)) else {
        return Vec::new();
    };
    let start = len.saturating_sub(max_bytes);
    if f.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut buf = Vec::with_capacity((len - start) as usize);
    if f.read_to_end(&mut buf).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    if start > 0 && !lines.is_empty() {
        lines.remove(0); // drop potentially cut first line
    }
    if lines.len() > max_lines {
        lines.drain(0..lines.len() - max_lines);
    }
    lines
}

fn cached_tail_lines(
    cache: &mut LocalCache,
    path: PathBuf,
    max_lines: usize,
    max_bytes: u64,
) -> Vec<String> {
    let Ok(meta) = fs::metadata(&path) else {
        cache.log_tails.remove(&path);
        return Vec::new();
    };
    let mtime = meta.modified().unwrap_or(UNIX_EPOCH);
    let len = meta.len();
    if let Some((c_mtime, c_len, lines)) = cache.log_tails.get(&path) {
        if *c_mtime == mtime && *c_len == len {
            return lines.clone();
        }
    }
    let lines = tail_lines(&path, max_lines, max_bytes);
    if cache.log_tails.len() > 32 {
        cache.log_tails.clear();
    }
    cache.log_tails.insert(path, (mtime, len, lines.clone()));
    lines
}

fn read_assigned_models(cache: &mut LocalCache, root: &Path) -> BTreeMap<String, String> {
    let cfg_path = root.join("config.json");
    let mtime = fs::metadata(&cfg_path)
        .and_then(|m| m.modified())
        .unwrap_or(UNIX_EPOCH);
    if let Some((ref p, c_mtime, ref models)) = cache.config_models {
        if p == &cfg_path && c_mtime == mtime {
            return models.clone();
        }
    }
    let mut out = BTreeMap::new();
    let Ok(raw) = fs::read_to_string(&cfg_path) else {
        return out;
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return out;
    };
    let Some(ajanlar) = val.get("ajanlar").and_then(|v| v.as_object()) else {
        return out;
    };
    let order = ["low", "medium", "high", "xhigh", "max"];
    for (rol, cfg) in ajanlar {
        let cli = cfg.get("cli").and_then(|v| v.as_str()).unwrap_or("");
        let model = cfg.get("model").and_then(|v| v.as_str()).unwrap_or("");
        let efor_str = match cfg.get("efor") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Object(map)) => {
                let mut vals: Vec<&str> = map.values().filter_map(|v| v.as_str()).collect();
                vals.sort_by_key(|v| order.iter().position(|o| o == v).unwrap_or(99));
                vals.dedup();
                vals.join("/")
            }
            _ => String::new(),
        };
        let label = if efor_str.is_empty() {
            format!("{cli}·{model}")
        } else {
            format!("{cli}·{model} ({efor_str})")
        };
        out.insert(rol.clone(), label);
    }
    cache.config_models = Some((cfg_path, mtime, out.clone()));
    out
}

fn parse_project(ad: &str, klasor: &str) -> Option<ProjectInfo> {
    let dir = expand_tilde(klasor);
    let raw = fs::read_to_string(dir.join("durum.json")).ok()?;
    let d: serde_json::Value = serde_json::from_str(&raw).ok()?;

    let mut gorevler = Vec::new();
    if let Some(obj) = d.get("gorevler").and_then(|v| v.as_object()) {
        for (gid, gval) in obj {
            let adim = gval
                .get("adim")
                .and_then(|v| v.as_str())
                .unwrap_or("bekliyor")
                .to_string();
            let deneme = gval.get("deneme").and_then(|v| v.as_i64()).unwrap_or(0);
            let incelendi = gval
                .get("incelendi")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let son_hata = gval
                .get("son_hata")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let inceleme_duzeltmeleri = gval
                .get("inceleme_duzeltmeleri")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            gorevler.push(ProjectTask {
                id: gid.clone(),
                adim,
                deneme,
                incelendi,
                son_hata,
                inceleme_duzeltmeleri,
            });
        }
    }
    // Natural sort G1, G2, ..., G10, G20
    gorevler.sort_by_key(|t| {
        t.id.trim_start_matches(|c: char| !c.is_ascii_digit())
            .parse::<u32>()
            .unwrap_or(9999)
    });

    let gorseller = d
        .get("gorseller")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Some(ProjectInfo {
        ad: d
            .get("uygulama_adi")
            .and_then(|v| v.as_str())
            .unwrap_or(ad)
            .to_string(),
        klasor: klasor.to_string(),
        asama: d
            .get("asama")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        asama_adi: d
            .get("asama_adi")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string(),
        su_an: d
            .get("su_an")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        expo_url: d
            .get("expo_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        figma_url: d
            .get("figma_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        gorevler,
        gorseller,
        olusturma: d.get("olusturma").and_then(|v| v.as_f64()),
    })
}

fn check_service_status(cache: &mut LocalCache) -> String {
    if let Some((checked_at, ref status)) = cache.service_status {
        if checked_at.elapsed() < Duration::from_secs(5) {
            return status.clone();
        }
    }
    #[cfg(target_os = "linux")]
    let status = {
        if let Ok(out) = Command::new("systemctl")
            .args(["--user", "is-active", "app-fabrikasi.service"])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                s
            } else {
                "unknown".to_string()
            }
        } else {
            "unknown".to_string()
        }
    };
    #[cfg(not(target_os = "linux"))]
    let status = "unknown".to_string();

    cache.service_status = Some((Instant::now(), status.clone()));
    status
}

fn open_ro_db(db_path: &Path) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let _ = conn.busy_timeout(Duration::from_millis(1500));
    let _ = conn.execute_batch("PRAGMA cache_size = -512; PRAGMA temp_store = MEMORY;");
    Ok(conn)
}

pub fn read_snapshot(root_str: &str, profile_name: &str) -> OfficeSnapshot {
    let root = expand_tilde(root_str);
    let ts = now_ts();
    if !root.exists() {
        return OfficeSnapshot {
            connected: false,
            error: Some(format!("Klasör bulunamadı: {}", root.display())),
            profile_name: profile_name.to_string(),
            mode: "local".to_string(),
            service_status: "unknown".to_string(),
            paused: false,
            manager_session: None,
            message_queue_count: 0,
            manager_context: None,
            quotas: vec![],
            active_agents: BTreeMap::new(),
            assigned_models: BTreeMap::new(),
            agent_logs: BTreeMap::new(),
            events_log: vec![],
            system_log: vec![],
            messages: vec![],
            jobs: vec![],
            active_project: None,
            all_projects: vec![],
            timestamp: ts,
        };
    }

    let db_path = root.join("kayitlar").join("durum.db");
    let canli = root.join("kayitlar").join("canli");

    let mut durum_map: HashMap<String, serde_json::Value> = HashMap::new();
    let mut jobs: Vec<JobItem> = Vec::new();
    let mut messages: Vec<ChatMessage> = Vec::new();
    let mut db_error: Option<String> = None;

    let mut cache = local_cache().lock().unwrap_or_else(|e| e.into_inner());

    let need_reopen = match cache.db {
        Some((ref p, _)) => p != &db_path,
        None => true,
    };
    if need_reopen {
        cache.db = match open_ro_db(&db_path) {
            Ok(c) => Some((db_path.clone(), c)),
            Err(e) => {
                db_error = Some(format!("durum.db okunamadı: {e}"));
                None
            }
        };
    }

    let mut query_failed = false;
    if let Some((_, ref conn)) = cache.db {
        if let Ok(mut stmt) = conn.prepare_cached("SELECT anahtar, deger FROM durum") {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            }) {
                for row in rows.flatten() {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&row.1) {
                        durum_map.insert(row.0, v);
                    }
                }
            } else {
                query_failed = true;
            }
        } else {
            query_failed = true;
        }
        if let Ok(mut stmt) = conn.prepare_cached(
            "SELECT id, tur, kuyruk, durum, deneme, sonraki_deneme, hata FROM isler WHERE durum NOT IN ('tamam','iptal') ORDER BY id",
        ) {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok(JobItem {
                    id: r.get(0)?,
                    tur: r.get(1)?,
                    kuyruk: r.get(2)?,
                    durum: r.get(3)?,
                    deneme: r.get(4)?,
                    sonraki_deneme: r.get(5)?,
                    hata: r.get(6)?,
                })
            }) {
                jobs = rows.flatten().collect();
            }
        }
        if let Ok(mut stmt) = conn.prepare_cached(
            "SELECT id, yon, metin, zaman, kanal, kaynak FROM mesajlar ORDER BY id DESC LIMIT 80",
        ) {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok(ChatMessage {
                    id: r.get(0)?,
                    yon: r.get(1)?,
                    metin: r.get(2)?,
                    zaman: r.get(3)?,
                    kanal: r.get(4)?,
                    kaynak: r.get(5)?,
                })
            }) {
                messages = rows.flatten().collect();
                messages.reverse();
            }
        }
    }
    if query_failed {
        cache.db = None;
    }

    let paused = durum_map
        .get("duraklatildi")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let manager_session = durum_map
        .get("yonetici_oturum")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let message_queue_count = jobs.iter().filter(|j| j.tur == "mesaj").count();

    let mut quotas = Vec::new();
    for (qid, qname) in QUOTA_POOLS {
        if let Some(k) = durum_map.get(&format!("kota:{qid}")) {
            let p5_sifir = k.get("p5_sifir").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let hafta_sifir = k.get("hafta_sifir").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let p5 = if p5_sifir > ts {
                k.get("p5").and_then(|v| v.as_f64()).unwrap_or(0.0)
            } else {
                0.0
            };
            let hafta = if hafta_sifir > ts {
                k.get("hafta").and_then(|v| v.as_f64()).unwrap_or(0.0)
            } else {
                0.0
            };
            let guncelleme = k.get("guncelleme").and_then(|v| v.as_f64()).unwrap_or(ts);
            quotas.push(QuotaPool {
                id: qid.to_string(),
                name: qname.to_string(),
                has_data: true,
                p5,
                hafta,
                p5_sifir,
                hafta_sifir,
                guncelleme,
            });
        } else {
            quotas.push(QuotaPool {
                id: qid.to_string(),
                name: qname.to_string(),
                has_data: false,
                p5: 0.0,
                hafta: 0.0,
                p5_sifir: 0.0,
                hafta_sifir: 0.0,
                guncelleme: 0.0,
            });
        }
    }

    let mut all_projects = Vec::new();
    if let Some(arr) = durum_map.get("projeler").and_then(|v| v.as_array()) {
        for p in arr.iter().rev() {
            let ad = p.get("ad").and_then(|v| v.as_str()).unwrap_or("");
            let klasor = p.get("klasor").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(info) = parse_project(ad, klasor) {
                all_projects.push(info);
            }
        }
    }
    let active_project = all_projects.first().cloned();

    let mut active_agents: BTreeMap<String, ActiveAgentInfo> = BTreeMap::new();
    if let Ok(raw) = fs::read_to_string(canli.join("aktif.json")) {
        if let Ok(map) = serde_json::from_str::<HashMap<String, serde_json::Value>>(&raw) {
            for (rol, v) in map {
                active_agents.insert(
                    rol,
                    ActiveAgentInfo {
                        etiket: v
                            .get("etiket")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        model: v
                            .get("model")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        cli: v
                            .get("cli")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        baslangic: v.get("baslangic").and_then(|x| x.as_f64()).unwrap_or(ts),
                        muadil: v.get("muadil").and_then(|x| x.as_bool()).unwrap_or(false),
                        aciklama: v
                            .get("aciklama")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                    },
                );
            }
        }
    }

    let manager_context = fs::read_to_string(canli.join("yonetici-baglam.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .map(|v| ManagerContext {
            token: v.get("token").and_then(|x| x.as_u64()).unwrap_or(0),
            pencere: v.get("pencere").and_then(|x| x.as_u64()).unwrap_or(1_000_000),
            zaman: v.get("zaman").and_then(|x| x.as_f64()).unwrap_or(ts),
            oturum: v.get("oturum").and_then(|x| x.as_str()).map(|s| s.to_string()),
            oturum_baslangic: v.get("oturum_baslangic").and_then(|x| x.as_f64()),
            son_ozet: v.get("son_ozet").cloned().filter(|x| !x.is_null()),
        });

    let mut agent_logs = BTreeMap::new();
    for rol in AGENTS {
        agent_logs.insert(
            rol.to_string(),
            cached_tail_lines(&mut cache, canli.join(format!("{rol}.log")), 60, 24_000),
        );
    }

    let events_log = cached_tail_lines(&mut cache, canli.join("olaylar.log"), 100, 48_000);
    let system_log = cached_tail_lines(&mut cache, canli.join("sistem.log"), 60, 24_000);
    let assigned_models = read_assigned_models(&mut cache, &root);
    let service_status = check_service_status(&mut cache);

    OfficeSnapshot {
        connected: db_error.is_none(),
        error: db_error,
        profile_name: profile_name.to_string(),
        mode: "local".to_string(),
        service_status,
        paused,
        manager_session,
        message_queue_count,
        manager_context,
        quotas,
        active_agents,
        assigned_models,
        agent_logs,
        events_log,
        system_log,
        messages,
        jobs,
        active_project,
        all_projects,
        timestamp: ts,
    }
}

pub fn read_full_log(root_str: &str, name: &str, max_lines: usize) -> Vec<String> {
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return vec![];
    }
    let root = expand_tilde(root_str);
    let path = root.join("kayitlar").join("canli").join(format!("{name}.log"));
    tail_lines(&path, max_lines, 512_000)
}

pub fn read_archives(root_str: &str) -> ArchiveOverview {
    let root = expand_tilde(root_str);
    let arsiv_dir = root.join("kayitlar").join("canli-arsiv");
    let mut archives = Vec::new();
    if let Ok(entries) = fs::read_dir(&arsiv_dir) {
        let mut dirs_vec: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs_vec.sort();
        dirs_vec.reverse();
        for dir in dirs_vec {
            let name = dir
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let mut files = Vec::new();
            if let Ok(f_entries) = fs::read_dir(&dir) {
                for fe in f_entries.flatten() {
                    if let Some(fname) = fe.file_name().to_str() {
                        if fname.ends_with(".log") {
                            files.push(fname.to_string());
                        }
                    }
                }
            }
            files.sort();
            archives.push(ArchiveProject { name, files });
        }
    }
    let journal = tail_lines(&root.join("kayitlar").join("yonetici-gunlugu.md"), 400, 128_000);
    ArchiveOverview { archives, journal }
}

pub fn read_archive_file(root_str: &str, archive_name: &str, file_name: &str) -> Vec<String> {
    if archive_name.contains("..")
        || archive_name.contains('/')
        || file_name.contains("..")
        || file_name.contains('/')
    {
        return vec![];
    }
    let root = expand_tilde(root_str);
    let path = root
        .join("kayitlar")
        .join("canli-arsiv")
        .join(archive_name)
        .join(file_name);
    tail_lines(&path, 2000, 512_000)
}

pub fn send_message(root_str: &str, text: &str) -> ActionResult {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return ActionResult {
            ok: false,
            message: "Boş mesaj gönderilemez.".to_string(),
        };
    }
    let root = expand_tilde(root_str);
    let db_path = root.join("kayitlar").join("durum.db");
    let conn = match Connection::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            return ActionResult {
                ok: false,
                message: format!("Veritabanına bağlanılamadı: {e}"),
            }
        }
    };
    let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
    let ts = now_ts();
    let msg_id = match conn.execute(
        "INSERT INTO mesajlar(yon, metin, zaman, kanal, kaynak) VALUES('gelen', ?, ?, 'ofis', 'sahip')",
        rusqlite::params![trimmed, ts],
    ) {
        Ok(_) => conn.last_insert_rowid(),
        Err(e) => {
            return ActionResult {
                ok: false,
                message: format!("Mesaj kaydedilemedi: {e}"),
            }
        }
    };
    let veri = serde_json::json!({
        "metin": trimmed,
        "kanal": "ofis",
        "mesaj_id": msg_id
    })
    .to_string();
    match conn.execute(
        "INSERT INTO isler(tur, kuyruk, veri, olusturma, guncelleme) VALUES('mesaj', 'hizli', ?, ?, ?)",
        rusqlite::params![veri, ts, ts],
    ) {
        Ok(_) => ActionResult {
            ok: true,
            message: "Mesaj yöneticinin masasında.".to_string(),
        },
        Err(e) => ActionResult {
            ok: false,
            message: format!("İş kuyruğuna eklenemedi: {e}"),
        },
    }
}

pub fn run_fabrika_cmd(root_str: &str, cmd: &str) -> ActionResult {
    let root = expand_tilde(root_str);
    let script = root.join("orkestrator").join("fabrika.py");
    match Command::new("python3").arg(&script).arg(cmd).output() {
        Ok(out) => {
            let msg = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            ActionResult {
                ok: out.status.success(),
                message: if !msg.is_empty() {
                    msg
                } else if !err.is_empty() {
                    err
                } else {
                    format!("Komut tamamlandı: {cmd}")
                },
            }
        }
        Err(e) => ActionResult {
            ok: false,
            message: format!("Komut çalıştırılamadı: {e}"),
        },
    }
}

pub fn service_control(root_str: &str, action: &str) -> ActionResult {
    #[cfg(target_os = "linux")]
    {
        let root = expand_tilde(root_str);
        let kayitlar = root.join("kayitlar");
        if action == "start" {
            let _ = fs::write(kayitlar.join("ofis-aciliyor"), "");
        } else if action == "stop" {
            let _ = fs::write(kayitlar.join("ofis-kapaniyor"), "");
        }
        let valid = matches!(action, "start" | "stop" | "restart");
        if !valid {
            return ActionResult {
                ok: false,
                message: format!("Geçersiz servis komutu: {action}"),
            };
        }
        match Command::new("systemctl")
            .args(["--user", action, "app-fabrikasi.service"])
            .output()
        {
            Ok(out) => {
                if let Ok(mut cache) = local_cache().lock() {
                    cache.service_status = None;
                }
                ActionResult {
                ok: out.status.success(),
                message: if out.status.success() {
                    match action {
                        "start" => "Ofis servisi başlatıldı.".to_string(),
                        "stop" => "Ofis servisi durduruldu.".to_string(),
                        _ => "Ofis servisi yeniden başlatıldı.".to_string(),
                    }
                } else {
                    String::from_utf8_lossy(&out.stderr).trim().to_string()
                },
            }
            },
            Err(e) => ActionResult {
                ok: false,
                message: format!("systemctl hatası: {e}"),
            },
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (root_str, action);
        ActionResult {
            ok: false,
            message: "Yerel servis kontrolü yalnızca Linux'ta desteklenir; VDS modunda sunucu köprüsü kullanılır.".to_string(),
        }
    }
}

fn resolve_safe_project_file(project_dir: &str, rel_path: &str) -> Result<(PathBuf, String), String> {
    let clean = rel_path.trim().trim_start_matches("./").replace('\\', "/");
    if clean.is_empty() || clean.contains("..") {
        return Err("Geçersiz dosya yolu.".to_string());
    }
    let proj = expand_tilde(project_dir)
        .canonicalize()
        .map_err(|_| "Proje klasörü bulunamadı.".to_string())?;
    let candidate = if Path::new(&clean).is_absolute() {
        PathBuf::from(&clean)
    } else {
        proj.join(&clean)
    };
    let canon = candidate
        .canonicalize()
        .map_err(|_| format!("Dosya bulunamadı: {clean}"))?;
    if !canon.starts_with(&proj) {
        return Err("Proje klasörü dışındaki dosyalara erişilemez.".to_string());
    }
    let rel_display = canon
        .strip_prefix(&proj)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or(clean);
    Ok((canon, rel_display))
}

pub fn read_project_file(project_dir: &str, rel_path: &str) -> ProjectFileResult {
    let (target, display_path) = match resolve_safe_project_file(project_dir, rel_path) {
        Ok(v) => v,
        Err(e) => {
            return ProjectFileResult {
                ok: false,
                path: rel_path.to_string(),
                lines: vec![],
                total_lines: 0,
                truncated: false,
                error: Some(e),
            };
        }
    };
    const MAX_BYTES: usize = 256_000;
    const MAX_LINES: usize = 1500;
    let Ok(mut f) = File::open(&target) else {
        return ProjectFileResult {
            ok: false,
            path: display_path,
            lines: vec![],
            total_lines: 0,
            truncated: false,
            error: Some("Dosya açılamadı.".to_string()),
        };
    };
    let meta_len = f.metadata().map(|m| m.len() as usize).unwrap_or(0);
    let mut buf = Vec::with_capacity(meta_len.min(MAX_BYTES));
    let mut limited = f.by_ref().take(MAX_BYTES as u64);
    if limited.read_to_end(&mut buf).is_err() {
        return ProjectFileResult {
            ok: false,
            path: display_path,
            lines: vec![],
            total_lines: 0,
            truncated: false,
            error: Some("Dosya okunamadı.".to_string()),
        };
    }
    let text = String::from_utf8_lossy(&buf);
    let all_lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    let total_lines = all_lines.len();
    let truncated = meta_len > MAX_BYTES || total_lines > MAX_LINES;
    let lines = if total_lines > MAX_LINES {
        all_lines.into_iter().take(MAX_LINES).collect()
    } else {
        all_lines
    };
    ProjectFileResult {
        ok: true,
        path: display_path,
        lines,
        total_lines,
        truncated,
        error: None,
    }
}

pub fn read_project_diff(project_dir: &str, rel_path: Option<&str>) -> ProjectDiffResult {
    let proj = expand_tilde(project_dir);
    if !proj.exists() {
        return ProjectDiffResult {
            ok: false,
            path: rel_path.map(String::from),
            lines: vec![],
            changed_files: vec![],
            error: Some("Proje klasörü bulunamadı.".to_string()),
        };
    }

    let mut changed_files: Vec<String> = Vec::new();
    if let Ok(out) = Command::new("git")
        .arg("-C")
        .arg(&proj)
        .args(["status", "--porcelain", "-uall"])
        .output()
    {
        if out.status.success() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                if line.len() >= 4 {
                    let mut fp = line[3..].trim().trim_matches('"').to_string();
                    if let Some((_, rhs)) = fp.split_once(" -> ") {
                        fp = rhs.trim_matches('"').to_string();
                    }
                    if !fp.is_empty() {
                        changed_files.push(fp);
                    }
                }
            }
        }
    }

    let clean_rel = rel_path
        .map(|s| s.trim().trim_start_matches("./").replace('\\', "/"))
        .filter(|s| !s.is_empty() && !s.contains(".."));

    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(&proj).args(["diff", "--no-color", "-U3"]);
    if let Some(ref rp) = clean_rel {
        cmd.arg("--").arg(rp);
    }

    let mut diff_lines: Vec<String> = match cmd.output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .take(1200)
            .map(|l| l.to_string())
            .collect(),
        _ => vec![],
    };

    // Yeni oluşturulan (untracked) dosya ise tüm satırları yeşil (+) diff olarak göster
    if diff_lines.is_empty() {
        if let Some(ref rp) = clean_rel {
            let file_res = read_project_file(project_dir, rp);
            if file_res.ok && !file_res.lines.is_empty() {
                let count = file_res.lines.len().min(400);
                diff_lines.push("--- /dev/null".to_string());
                diff_lines.push(format!("+++ b/{}", file_res.path));
                diff_lines.push(format!("@@ -0,0 +1,{count} @@"));
                for l in file_res.lines.into_iter().take(400) {
                    diff_lines.push(format!("+{l}"));
                }
            }
        }
    }

    ProjectDiffResult {
        ok: true,
        path: clean_rel,
        lines: diff_lines,
        changed_files,
        error: None,
    }
}
