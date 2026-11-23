mod config;
mod dictation;
mod graph;
mod local;
mod models;
mod remote;

use crate::dictation::{RecorderState, SharedRecorder};
use crate::models::{
    ActionResult, AppSettings, ArchiveOverview, GraphData, MemoryStats, NoteContent,
    OfficeSnapshot, PollSnapshotResult,
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::RwLock;
use tauri::State;

struct AppState {
    settings: RwLock<AppSettings>,
    recorder: SharedRecorder,
}

fn expand_tilde(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(p)
}

#[cfg(target_os = "linux")]
fn read_pss_kb(pid: u32) -> f64 {
    let path = format!("/proc/{pid}/smaps_rollup");
    if let Ok(text) = std::fs::read_to_string(&path) {
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("Pss:") {
                if let Some(kb_str) = rest.split_whitespace().next() {
                    if let Ok(kb) = kb_str.parse::<f64>() {
                        return kb;
                    }
                }
            }
        }
    }
    // Fallback to RSS from statm
    if let Ok(text) = std::fs::read_to_string(format!("/proc/{pid}/statm")) {
        if let Some(pages_str) = text.split_whitespace().nth(1) {
            if let Ok(pages) = pages_str.parse::<f64>() {
                return pages * 4.0;
            }
        }
    }
    0.0
}

#[cfg(target_os = "linux")]
fn measure_memory() -> MemoryStats {
    let my_pid = std::process::id();
    let rust_kb = read_pss_kb(my_pid);
    let mut webkit_kb = 0.0;
    let mut network_kb = 0.0;

    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            if pid == my_pid {
                continue;
            }
            // Check if parent PID == my_pid
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                continue;
            };
            let Some(rparen) = stat.rfind(')') else {
                continue;
            };
            let ppid = stat[rparen + 1..]
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            if ppid != my_pid {
                continue;
            }
            let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
            let kb = read_pss_kb(pid);
            if comm.contains("Network") {
                network_kb += kb;
            } else {
                webkit_kb += kb;
            }
        }
    }

    let rust_mb = (rust_kb / 1024.0 * 10.0).round() / 10.0;
    let webkit_mb = (webkit_kb / 1024.0 * 10.0).round() / 10.0;
    let network_mb = (network_kb / 1024.0 * 10.0).round() / 10.0;
    let total_mb = ((rust_kb + webkit_kb + network_kb) / 1024.0 * 10.0).round() / 10.0;
    MemoryStats {
        total_mb,
        rust_mb,
        webkit_mb,
        network_mb,
    }
}

#[cfg(not(target_os = "linux"))]
fn measure_memory() -> MemoryStats {
    MemoryStats {
        total_mb: 0.0,
        rust_mb: 0.0,
        webkit_mb: 0.0,
        network_mb: 0.0,
    }
}

fn snapshot_hash(snap: &OfficeSnapshot) -> u64 {
    let mut h = DefaultHasher::new();
    snap.connected.hash(&mut h);
    snap.error.hash(&mut h);
    snap.profile_name.hash(&mut h);
    snap.mode.hash(&mut h);
    snap.service_status.hash(&mut h);
    snap.paused.hash(&mut h);
    snap.manager_session.hash(&mut h);
    snap.message_queue_count.hash(&mut h);
    if let Some(mc) = &snap.manager_context {
        true.hash(&mut h);
        mc.token.hash(&mut h);
        mc.pencere.hash(&mut h);
        mc.zaman.to_bits().hash(&mut h);
        mc.oturum.hash(&mut h);
        mc.oturum_baslangic.map(|v| v.to_bits()).hash(&mut h);
    } else {
        false.hash(&mut h);
    }
    for q in &snap.quotas {
        q.id.hash(&mut h);
        q.has_data.hash(&mut h);
        q.p5.to_bits().hash(&mut h);
        q.hafta.to_bits().hash(&mut h);
        q.p5_sifir.to_bits().hash(&mut h);
        q.hafta_sifir.to_bits().hash(&mut h);
    }
    for (k, a) in &snap.active_agents {
        k.hash(&mut h);
        a.etiket.hash(&mut h);
        a.model.hash(&mut h);
        a.cli.hash(&mut h);
        a.baslangic.to_bits().hash(&mut h);
        a.muadil.hash(&mut h);
        a.aciklama.hash(&mut h);
    }
    for (k, v) in &snap.assigned_models {
        k.hash(&mut h);
        v.hash(&mut h);
    }
    for (k, lines) in &snap.agent_logs {
        k.hash(&mut h);
        lines.hash(&mut h);
    }
    snap.events_log.hash(&mut h);
    snap.system_log.hash(&mut h);
    for m in &snap.messages {
        m.id.hash(&mut h);
        m.yon.hash(&mut h);
        m.metin.hash(&mut h);
        m.zaman.to_bits().hash(&mut h);
        m.kanal.hash(&mut h);
        m.kaynak.hash(&mut h);
    }
    for j in &snap.jobs {
        j.id.hash(&mut h);
        j.tur.hash(&mut h);
        j.kuyruk.hash(&mut h);
        j.durum.hash(&mut h);
        j.deneme.hash(&mut h);
        j.hata.hash(&mut h);
    }
    for p in &snap.all_projects {
        p.ad.hash(&mut h);
        p.klasor.hash(&mut h);
        p.asama.hash(&mut h);
        p.asama_adi.hash(&mut h);
        p.su_an.hash(&mut h);
        p.expo_url.hash(&mut h);
        p.figma_url.hash(&mut h);
        for t in &p.gorevler {
            t.id.hash(&mut h);
            t.adim.hash(&mut h);
            t.deneme.hash(&mut h);
            t.incelendi.hash(&mut h);
            t.son_hata.hash(&mut h);
            t.inceleme_duzeltmeleri.hash(&mut h);
        }
    }
    h.finish()
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.settings.read().unwrap().clone()
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    config::save_settings(&settings)?;
    *state.settings.write().unwrap() = settings;
    Ok(())
}

#[tauri::command]
fn get_memory_stats() -> MemoryStats {
    measure_memory()
}

#[tauri::command]
async fn get_snapshot(state: State<'_, AppState>) -> Result<OfficeSnapshot, String> {
    let profile = {
        let s = state.settings.read().unwrap();
        config::active_profile(&s)
    };
    if profile.mode == "remote" {
        Ok(remote::fetch_snapshot(&profile).await)
    } else {
        let root = profile.local_root.clone();
        let name = profile.name.clone();
        tokio::task::spawn_blocking(move || local::read_snapshot(&root, &name))
            .await
            .map_err(|e| e.to_string())
    }
}

#[tauri::command]
async fn poll_snapshot(
    state: State<'_, AppState>,
    last_hash: String,
) -> Result<PollSnapshotResult, String> {
    let profile = {
        let s = state.settings.read().unwrap();
        config::active_profile(&s)
    };
    let snap = if profile.mode == "remote" {
        remote::fetch_snapshot(&profile).await
    } else {
        let root = profile.local_root.clone();
        let name = profile.name.clone();
        tokio::task::spawn_blocking(move || local::read_snapshot(&root, &name))
            .await
            .map_err(|e| e.to_string())?
    };
    let hash = format!("{:016x}", snapshot_hash(&snap));
    if !last_hash.is_empty() && hash == last_hash {
        Ok(PollSnapshotResult {
            hash,
            snapshot: None,
        })
    } else {
        Ok(PollSnapshotResult {
            hash,
            snapshot: Some(snap),
        })
    }
}

#[tauri::command]
async fn get_full_log(
    state: State<'_, AppState>,
    name: String,
    max_lines: Option<usize>,
) -> Result<Vec<String>, String> {
    let profile = {
        let s = state.settings.read().unwrap();
        config::active_profile(&s)
    };
    let lines = max_lines.unwrap_or(1500);
    if profile.mode == "remote" {
        Ok(remote::fetch_full_log(&profile, &name, lines).await)
    } else {
        Ok(local::read_full_log(&profile.local_root, &name, lines))
    }
}

#[tauri::command]
async fn get_graph(state: State<'_, AppState>) -> Result<GraphData, String> {
    let profile = {
        let s = state.settings.read().unwrap();
        config::active_profile(&s)
    };
    if profile.mode == "remote" {
        remote::fetch_graph(&profile).await
    } else {
        graph::scan_vault(&expand_tilde(&profile.local_root))
    }
}

#[tauri::command]
async fn get_note(state: State<'_, AppState>, note_id: String) -> Result<NoteContent, String> {
    let profile = {
        let s = state.settings.read().unwrap();
        config::active_profile(&s)
    };
    if profile.mode == "remote" {
        remote::fetch_note(&profile, &note_id).await
    } else {
        graph::read_note(&expand_tilde(&profile.local_root), &note_id)
    }
}

#[tauri::command]
async fn get_archives(state: State<'_, AppState>) -> Result<ArchiveOverview, String> {
    let profile = {
        let s = state.settings.read().unwrap();
        config::active_profile(&s)
    };
    if profile.mode == "remote" {
        remote::fetch_archives(&profile).await
    } else {
        Ok(local::read_archives(&profile.local_root))
    }
}

#[tauri::command]
async fn get_archive_file(
    state: State<'_, AppState>,
    archive_name: String,
    file_name: String,
) -> Result<Vec<String>, String> {
    let profile = {
        let s = state.settings.read().unwrap();
        config::active_profile(&s)
    };
    if profile.mode == "remote" {
        Ok(remote::fetch_archive_file(&profile, &archive_name, &file_name).await)
    } else {
        Ok(local::read_archive_file(
            &profile.local_root,
            &archive_name,
            &file_name,
        ))
    }
}

#[tauri::command]
async fn send_message(state: State<'_, AppState>, text: String) -> Result<ActionResult, String> {
    let profile = {
        let s = state.settings.read().unwrap();
        config::active_profile(&s)
    };
    if profile.mode == "remote" {
        Ok(remote::post_action(&profile, "/api/message", serde_json::json!({ "text": text })).await)
    } else {
        Ok(local::send_message(&profile.local_root, &text))
    }
}

#[tauri::command]
async fn control_factory(state: State<'_, AppState>, action: String) -> Result<ActionResult, String> {
    let profile = {
        let s = state.settings.read().unwrap();
        config::active_profile(&s)
    };
    if profile.mode == "remote" {
        Ok(remote::post_action(&profile, "/api/control", serde_json::json!({ "action": action })).await)
    } else {
        match action.as_str() {
            "durdur" | "devam" | "arastir" => Ok(local::run_fabrika_cmd(&profile.local_root, &action)),
            "start" | "stop" | "restart" => Ok(local::service_control(&profile.local_root, &action)),
            _ => Ok(ActionResult {
                ok: false,
                message: format!("Bilinmeyen komut: {action}"),
            }),
        }
    }
}

#[tauri::command]
fn start_dictation(state: State<'_, AppState>) -> Result<(), String> {
    dictation::start_recording(&state.recorder)
}

#[tauri::command]
async fn stop_dictation(state: State<'_, AppState>) -> Result<String, String> {
    let cfg = {
        let s = state.settings.read().unwrap();
        s.dictation.clone()
    };
    dictation::stop_and_transcribe(&state.recorder, &cfg).await
}

#[tauri::command]
async fn test_connection(profile: models::ConnectionProfile) -> ActionResult {
    if profile.mode == "remote" {
        let snap = remote::fetch_snapshot(&profile).await;
        ActionResult {
            ok: snap.connected,
            message: if snap.connected {
                format!("VDS bağlantısı başarılı (servis: {})", snap.service_status)
            } else {
                snap.error.unwrap_or_else(|| "Bağlanılamadı".to_string())
            },
        }
    } else {
        let snap = local::read_snapshot(&profile.local_root, &profile.name);
        ActionResult {
            ok: snap.connected,
            message: if snap.connected {
                format!("Yerel klasör doğrulandı (servis: {})", snap.service_status)
            } else {
                snap.error.unwrap_or_else(|| "Klasör okunamadı".to_string())
            },
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let initial_settings = config::load_settings();
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState {
            settings: RwLock::new(initial_settings),
            recorder: SharedRecorder::new(RecorderState::default()),
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            get_memory_stats,
            get_snapshot,
            poll_snapshot,
            get_full_log,
            get_graph,
            get_note,
            get_archives,
            get_archive_file,
            send_message,
            control_factory,
            start_dictation,
            stop_dictation,
            test_connection,
        ])
        .run(tauri::generate_context!())
        .expect("App Fabrikası Ofisi başlatılamadı");
}
