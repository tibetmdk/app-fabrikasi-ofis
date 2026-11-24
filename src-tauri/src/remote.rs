use crate::models::{
    ActionResult, ArchiveOverview, ConnectionProfile, GraphData, NoteContent, OfficeSnapshot,
    ProjectDiffResult, ProjectFileResult,
};
use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn client() -> Result<&'static reqwest::Client, String> {
    if let Some(c) = HTTP_CLIENT.get() {
        return Ok(c);
    }
    let built = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(4))
        .timeout(std::time::Duration::from_secs(8))
        .pool_max_idle_per_host(2)
        .build()
        .map_err(|e| e.to_string())?;
    let _ = HTTP_CLIENT.set(built);
    Ok(HTTP_CLIENT.get().unwrap())
}

fn build_url(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

fn now_ts() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn empty_error_snapshot(profile: &ConnectionProfile, err: String) -> OfficeSnapshot {
    OfficeSnapshot {
        connected: false,
        error: Some(err),
        profile_name: profile.name.clone(),
        mode: "remote".to_string(),
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
        timestamp: now_ts(),
    }
}

pub async fn fetch_snapshot(profile: &ConnectionProfile) -> OfficeSnapshot {
    let Ok(c) = client() else {
        return empty_error_snapshot(profile, "HTTP istemcisi oluşturulamadı".to_string());
    };
    let mut req = c.get(build_url(&profile.remote_url, "/api/snapshot"));
    if !profile.remote_token.trim().is_empty() {
        req = req.bearer_auth(profile.remote_token.trim());
    }
    match req.send().await {
        Ok(resp) if resp.status().is_success() => match resp.json::<OfficeSnapshot>().await {
            Ok(mut snap) => {
                snap.profile_name = profile.name.clone();
                snap.mode = "remote".to_string();
                snap
            }
            Err(e) => empty_error_snapshot(profile, format!("Sunucu yanıtı okunamadı: {e}")),
        },
        Ok(resp) => empty_error_snapshot(profile, format!("Sunucu hatası: HTTP {}", resp.status())),
        Err(e) => {
            let msg = if e.is_timeout() {
                "VDS yanıt vermedi (zaman aşımı)".to_string()
            } else if e.is_connect() {
                "VDS sunucusuna ulaşılamadı".to_string()
            } else {
                format!("VDS bağlantısı kurulamadı: {e}")
            };
            empty_error_snapshot(profile, msg)
        }
    }
}

pub async fn fetch_full_log(
    profile: &ConnectionProfile,
    name: &str,
    max_lines: usize,
) -> Vec<String> {
    let Ok(c) = client() else { return vec![] };
    let mut req = c.get(build_url(
        &profile.remote_url,
        &format!("/api/logs/{name}?lines={max_lines}"),
    ));
    if !profile.remote_token.trim().is_empty() {
        req = req.bearer_auth(profile.remote_token.trim());
    }
    match req.send().await {
        Ok(resp) if resp.status().is_success() => resp.json::<Vec<String>>().await.unwrap_or_default(),
        _ => vec![],
    }
}

pub async fn fetch_graph(profile: &ConnectionProfile) -> Result<GraphData, String> {
    let c = client()?;
    let mut req = c.get(build_url(&profile.remote_url, "/api/graph"));
    if !profile.remote_token.trim().is_empty() {
        req = req.bearer_auth(profile.remote_token.trim());
    }
    let resp = req.send().await.map_err(|e| format!("Bağlantı hatası: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<GraphData>().await.map_err(|e| e.to_string())
}

pub async fn fetch_note(profile: &ConnectionProfile, note_id: &str) -> Result<NoteContent, String> {
    let c = client()?;
    let mut req = c
        .get(build_url(&profile.remote_url, "/api/note"))
        .query(&[("id", note_id)]);
    if !profile.remote_token.trim().is_empty() {
        req = req.bearer_auth(profile.remote_token.trim());
    }
    let resp = req.send().await.map_err(|e| format!("Bağlantı hatası: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<NoteContent>().await.map_err(|e| e.to_string())
}

pub async fn fetch_archives(profile: &ConnectionProfile) -> Result<ArchiveOverview, String> {
    let c = client()?;
    let mut req = c.get(build_url(&profile.remote_url, "/api/archives"));
    if !profile.remote_token.trim().is_empty() {
        req = req.bearer_auth(profile.remote_token.trim());
    }
    let resp = req.send().await.map_err(|e| format!("Bağlantı hatası: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<ArchiveOverview>().await.map_err(|e| e.to_string())
}

pub async fn fetch_archive_file(
    profile: &ConnectionProfile,
    archive_name: &str,
    file_name: &str,
) -> Vec<String> {
    let Ok(c) = client() else { return vec![] };
    let mut req = c
        .get(build_url(&profile.remote_url, "/api/archive-file"))
        .query(&[("archive", archive_name), ("file", file_name)]);
    if !profile.remote_token.trim().is_empty() {
        req = req.bearer_auth(profile.remote_token.trim());
    }
    match req.send().await {
        Ok(resp) if resp.status().is_success() => resp.json::<Vec<String>>().await.unwrap_or_default(),
        _ => vec![],
    }
}

pub async fn post_action(
    profile: &ConnectionProfile,
    path: &str,
    body: serde_json::Value,
) -> ActionResult {
    let Ok(c) = client() else {
        return ActionResult {
            ok: false,
            message: "HTTP istemcisi hatası".to_string(),
        };
    };
    let mut req = c.post(build_url(&profile.remote_url, path)).json(&body);
    if !profile.remote_token.trim().is_empty() {
        req = req.bearer_auth(profile.remote_token.trim());
    }
    match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            resp.json::<ActionResult>().await.unwrap_or(ActionResult {
                ok: true,
                message: "İşlem tamamlandı.".to_string(),
            })
        }
        Ok(resp) => ActionResult {
            ok: false,
            message: format!("Sunucu hatası: HTTP {}", resp.status()),
        },
        Err(e) => ActionResult {
            ok: false,
            message: format!("Bağlantı hatası: {e}"),
        },
    }
}

pub async fn fetch_project_file(
    profile: &ConnectionProfile,
    project_dir: &str,
    rel_path: &str,
) -> ProjectFileResult {
    let Ok(c) = client() else {
        return ProjectFileResult {
            ok: false,
            path: rel_path.to_string(),
            lines: vec![],
            total_lines: 0,
            truncated: false,
            error: Some("HTTP istemcisi hatası".to_string()),
        };
    };
    let mut req = c
        .get(build_url(&profile.remote_url, "/api/project-file"))
        .query(&[("dir", project_dir), ("path", rel_path)]);
    if !profile.remote_token.trim().is_empty() {
        req = req.bearer_auth(profile.remote_token.trim());
    }
    match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            resp.json::<ProjectFileResult>()
                .await
                .unwrap_or_else(|e| ProjectFileResult {
                    ok: false,
                    path: rel_path.to_string(),
                    lines: vec![],
                    total_lines: 0,
                    truncated: false,
                    error: Some(format!("Yanıt okunamadı: {e}")),
                })
        }
        Ok(resp) if resp.status().as_u16() == 404 => ProjectFileResult {
            ok: false,
            path: rel_path.to_string(),
            lines: vec![],
            total_lines: 0,
            truncated: false,
            error: Some(
                "VDS köprüsü (kopru.py) henüz güncellenmemiş — canlı dosya izleyici için VDS'deki kopru.py'yi güncelleyin."
                    .to_string(),
            ),
        },
        Ok(resp) => ProjectFileResult {
            ok: false,
            path: rel_path.to_string(),
            lines: vec![],
            total_lines: 0,
            truncated: false,
            error: Some(format!("Sunucu hatası: HTTP {}", resp.status())),
        },
        Err(e) => ProjectFileResult {
            ok: false,
            path: rel_path.to_string(),
            lines: vec![],
            total_lines: 0,
            truncated: false,
            error: Some(format!("Bağlantı hatası: {e}")),
        },
    }
}

pub async fn fetch_project_diff(
    profile: &ConnectionProfile,
    project_dir: &str,
    rel_path: Option<&str>,
) -> ProjectDiffResult {
    let Ok(c) = client() else {
        return ProjectDiffResult {
            ok: false,
            path: rel_path.map(String::from),
            lines: vec![],
            changed_files: vec![],
            error: Some("HTTP istemcisi hatası".to_string()),
        };
    };
    let mut query: Vec<(&str, &str)> = vec![("dir", project_dir)];
    if let Some(rp) = rel_path {
        if !rp.trim().is_empty() {
            query.push(("path", rp));
        }
    }
    let mut req = c
        .get(build_url(&profile.remote_url, "/api/project-diff"))
        .query(&query);
    if !profile.remote_token.trim().is_empty() {
        req = req.bearer_auth(profile.remote_token.trim());
    }
    match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            resp.json::<ProjectDiffResult>()
                .await
                .unwrap_or_else(|e| ProjectDiffResult {
                    ok: false,
                    path: rel_path.map(String::from),
                    lines: vec![],
                    changed_files: vec![],
                    error: Some(format!("Yanıt okunamadı: {e}")),
                })
        }
        Ok(resp) if resp.status().as_u16() == 404 => ProjectDiffResult {
            ok: false,
            path: rel_path.map(String::from),
            lines: vec![],
            changed_files: vec![],
            error: Some(
                "VDS köprüsü (kopru.py) henüz güncellenmemiş — canlı diff izleyici için VDS'deki kopru.py'yi güncelleyin."
                    .to_string(),
            ),
        },
        Ok(resp) => ProjectDiffResult {
            ok: false,
            path: rel_path.map(String::from),
            lines: vec![],
            changed_files: vec![],
            error: Some(format!("Sunucu hatası: HTTP {}", resp.status())),
        },
        Err(e) => ProjectDiffResult {
            ok: false,
            path: rel_path.map(String::from),
            lines: vec![],
            changed_files: vec![],
            error: Some(format!("Bağlantı hatası: {e}")),
        },
    }
}
