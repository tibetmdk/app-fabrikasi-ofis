mod config;
mod dictation;
mod graph;
mod local;
mod models;
mod remote;

use crate::dictation::{RecorderState, SharedRecorder};
use crate::models::{
    ActionResult, AppSettings, ArchiveOverview, GraphData, MemoryStats, NoteContent,
    OfficeSnapshot, PollSnapshotResult, SnapshotDelta,
};
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};
use tauri::State;

struct AppState {
    settings: RwLock<AppSettings>,
    recorder: SharedRecorder,
    last_snapshot: Mutex<Option<(String, OfficeSnapshot)>>,
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

#[cfg(any(target_os = "windows", test))]
fn collect_descendant_pids(root_pid: u32, pairs: &[(u32, u32)]) -> Vec<u32> {
    let mut children_by_ppid: std::collections::HashMap<u32, Vec<u32>> =
        std::collections::HashMap::new();
    for &(pid, ppid) in pairs {
        if pid != 0 && pid != ppid {
            children_by_ppid.entry(ppid).or_default().push(pid);
        }
    }
    let mut visited = std::collections::HashSet::new();
    visited.insert(root_pid);
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(root_pid);
    let mut descendants = Vec::new();
    while let Some(curr) = queue.pop_front() {
        if let Some(kids) = children_by_ppid.get(&curr) {
            for &kid in kids {
                if visited.insert(kid) {
                    descendants.push(kid);
                    queue.push_back(kid);
                }
            }
        }
    }
    descendants
}

#[cfg(target_os = "windows")]
mod win_mem {
    use super::{collect_descendant_pids, MemoryStats};
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};

    const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const INVALID_HANDLE_VALUE: *mut c_void = -1isize as *mut c_void;

    #[repr(C)]
    struct ProcessEntry32W {
        dw_size: u32,
        cnt_usage: u32,
        th32_process_id: u32,
        th32_default_heap_id: usize,
        th32_module_id: u32,
        cnt_threads: u32,
        th32_parent_process_id: u32,
        pc_pri_class_base: i32,
        dw_flags: u32,
        sz_exe_file: [u16; 260],
    }

    #[repr(C)]
    struct ProcessMemoryCountersEx {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
        private_usage: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateToolhelp32Snapshot(dw_flags: u32, th32_process_id: u32) -> *mut c_void;
        fn Process32FirstW(h_snapshot: *mut c_void, lppe: *mut ProcessEntry32W) -> i32;
        fn Process32NextW(h_snapshot: *mut c_void, lppe: *mut ProcessEntry32W) -> i32;
        fn OpenProcess(dw_desired_access: u32, b_inherit_handle: i32, dw_process_id: u32)
            -> *mut c_void;
        fn CloseHandle(h_object: *mut c_void) -> i32;
        fn K32GetProcessMemoryInfo(
            process: *mut c_void,
            ppsmem_counters: *mut ProcessMemoryCountersEx,
            cb: u32,
        ) -> i32;
    }

    fn read_process_mem_kb(pid: u32) -> f64 {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return 0.0;
            }
            let mut counters: ProcessMemoryCountersEx = zeroed();
            counters.cb = size_of::<ProcessMemoryCountersEx>() as u32;
            let ok = K32GetProcessMemoryInfo(handle, &mut counters, counters.cb);
            CloseHandle(handle);
            if ok == 0 {
                return 0.0;
            }
            // PrivateUsage avoids double-counting shared DLLs across WebView2 processes
            let bytes = if counters.private_usage > 0 {
                counters.private_usage
            } else {
                counters.working_set_size
            };
            bytes as f64 / 1024.0
        }
    }

    pub fn measure_memory() -> MemoryStats {
        let my_pid = std::process::id();
        let rust_kb = read_process_mem_kb(my_pid);
        let mut pairs: Vec<(u32, u32)> = Vec::new();

        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if !snap.is_null() && snap != INVALID_HANDLE_VALUE {
                let mut entry: ProcessEntry32W = zeroed();
                entry.dw_size = size_of::<ProcessEntry32W>() as u32;
                if Process32FirstW(snap, &mut entry) != 0 {
                    loop {
                        pairs.push((entry.th32_process_id, entry.th32_parent_process_id));
                        if Process32NextW(snap, &mut entry) == 0 {
                            break;
                        }
                    }
                }
                CloseHandle(snap);
            }
        }

        let mut webview_kb = 0.0;
        for pid in collect_descendant_pids(my_pid, &pairs) {
            webview_kb += read_process_mem_kb(pid);
        }

        let rust_mb = (rust_kb / 1024.0 * 10.0).round() / 10.0;
        let webkit_mb = (webview_kb / 1024.0 * 10.0).round() / 10.0;
        let total_mb = ((rust_kb + webview_kb) / 1024.0 * 10.0).round() / 10.0;
        MemoryStats {
            total_mb,
            rust_mb,
            webkit_mb,
            network_mb: 0.0,
        }
    }
}

#[cfg(target_os = "windows")]
fn measure_memory() -> MemoryStats {
    win_mem::measure_memory()
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
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

fn build_delta(prev: &OfficeSnapshot, next: &OfficeSnapshot) -> SnapshotDelta {
    let mut agent_logs = BTreeMap::new();
    for (rol, lines) in &next.agent_logs {
        if prev.agent_logs.get(rol) != Some(lines) {
            agent_logs.insert(rol.clone(), lines.clone());
        }
    }
    for rol in prev.agent_logs.keys() {
        if !next.agent_logs.contains_key(rol) {
            agent_logs.insert(rol.clone(), Vec::new());
        }
    }

    SnapshotDelta {
        connected: next.connected,
        error: next.error.clone(),
        profile_name: next.profile_name.clone(),
        mode: next.mode.clone(),
        service_status: next.service_status.clone(),
        paused: next.paused,
        manager_session: next.manager_session.clone(),
        message_queue_count: next.message_queue_count,
        manager_context: next.manager_context.clone(),
        quotas: next.quotas.clone(),
        active_agents: next.active_agents.clone(),
        assigned_models: next.assigned_models.clone(),
        jobs: next.jobs.clone(),
        timestamp: next.timestamp,
        agent_logs,
        events_log: if next.events_log != prev.events_log {
            Some(next.events_log.clone())
        } else {
            None
        },
        system_log: if next.system_log != prev.system_log {
            Some(next.system_log.clone())
        } else {
            None
        },
        messages: if next.messages != prev.messages {
            Some(next.messages.clone())
        } else {
            None
        },
        all_projects: if next.all_projects != prev.all_projects {
            Some(next.all_projects.clone())
        } else {
            None
        },
    }
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.settings.read().unwrap().clone()
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    config::save_settings(&settings)?;
    *state.settings.write().unwrap() = settings;
    *state.last_snapshot.lock().unwrap_or_else(|e| e.into_inner()) = None;
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
        return Ok(PollSnapshotResult {
            hash,
            snapshot: None,
            delta: None,
        });
    }

    let mut cache = state.last_snapshot.lock().unwrap_or_else(|e| e.into_inner());
    if !last_hash.is_empty() {
        if let Some((prev_hash, prev_snap)) = cache.as_ref() {
            if prev_hash == &last_hash {
                let delta = build_delta(prev_snap, &snap);
                *cache = Some((hash.clone(), snap));
                return Ok(PollSnapshotResult {
                    hash,
                    snapshot: None,
                    delta: Some(delta),
                });
            }
        }
    }

    *cache = Some((hash.clone(), snap.clone()));
    Ok(PollSnapshotResult {
        hash,
        snapshot: Some(snap),
        delta: None,
    })
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

#[derive(serde::Serialize)]
struct InstallInfo {
    os: String,
    bundle_type: String,
    safe_self_update: bool,
    reason: Option<String>,
}

fn resolve_bundle_type_label(
    bundle: Option<tauri::utils::config::BundleType>,
    is_appimage_env: bool,
) -> &'static str {
    use tauri::utils::config::BundleType;
    match bundle {
        Some(BundleType::Deb) => "deb",
        Some(BundleType::Rpm) => "rpm",
        Some(BundleType::AppImage) => "appimage",
        Some(BundleType::Msi) => "msi",
        Some(BundleType::Nsis) => "nsis",
        Some(BundleType::App) => "app",
        Some(BundleType::Dmg) => "dmg",
        None if is_appimage_env => "appimage",
        None => "standalone",
    }
}

fn evaluate_install_info(
    os: &str,
    bundle: Option<tauri::utils::config::BundleType>,
    is_appimage_env: bool,
) -> InstallInfo {
    let bundle_label = resolve_bundle_type_label(bundle, is_appimage_env);
    // On Linux, tauri-plugin-updater falls back to downloading an AppImage and overwriting the
    // running binary in-place whenever bundle_type() is None (standalone / --no-bundle build).
    // On Ubuntu 24.04+/26.04 without libfuse2, overwriting a native ELF binary with an AppImage
    // breaks launching completely. Block in-place self-update for standalone Linux binaries.
    let (safe_self_update, reason) = if os == "linux" && bundle_label == "standalone" {
        (
            false,
            Some(
                "Yerel tek-dosya sürümünde otomatik güncelleme Linux'ta AppImage üzerine yazıp açılmamaya yol açabildiği için engellendi. Tek tıkla güncelleme için .deb paketini kurun."
                    .to_string(),
            ),
        )
    } else {
        (true, None)
    };

    InstallInfo {
        os: os.to_string(),
        bundle_type: bundle_label.to_string(),
        safe_self_update,
        reason,
    }
}

#[tauri::command]
fn get_install_info() -> InstallInfo {
    evaluate_install_info(
        std::env::consts::OS,
        tauri::utils::platform::bundle_type(),
        std::env::var_os("APPIMAGE").is_some(),
    )
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
            last_snapshot: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            get_memory_stats,
            get_install_info,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ChatMessage, ProjectInfo};

    fn sample_snap() -> OfficeSnapshot {
        let mut agent_logs = BTreeMap::new();
        agent_logs.insert("kod_ajani".to_string(), vec!["l1".to_string()]);
        agent_logs.insert("test_ajani".to_string(), vec!["t1".to_string()]);
        OfficeSnapshot {
            connected: true,
            error: None,
            profile_name: "VDS".to_string(),
            mode: "remote".to_string(),
            service_status: "active".to_string(),
            paused: false,
            manager_session: None,
            message_queue_count: 0,
            manager_context: None,
            quotas: vec![],
            active_agents: BTreeMap::new(),
            assigned_models: BTreeMap::new(),
            agent_logs,
            events_log: vec!["e1".to_string()],
            system_log: vec!["s1".to_string()],
            messages: vec![ChatMessage {
                id: 1,
                yon: "gelen".to_string(),
                metin: "merhaba".to_string(),
                zaman: 1.0,
                kanal: "ofis".to_string(),
                kaynak: "sahip".to_string(),
            }],
            jobs: vec![],
            active_project: None,
            all_projects: vec![ProjectInfo {
                ad: "TestApp".to_string(),
                klasor: "/tmp/test".to_string(),
                asama: "gelistirme".to_string(),
                asama_adi: "Geliştirme".to_string(),
                su_an: None,
                expo_url: None,
                figma_url: None,
                gorevler: vec![],
                gorseller: vec![],
                olusturma: None,
            }],
            timestamp: 100.0,
        }
    }

    #[test]
    fn delta_omits_unchanged_heavy_fields() {
        let prev = sample_snap();
        let mut next = sample_snap();
        next.agent_logs
            .get_mut("kod_ajani")
            .unwrap()
            .push("l2".to_string());

        let delta = build_delta(&prev, &next);
        assert_eq!(delta.agent_logs.len(), 1);
        assert!(delta.agent_logs.contains_key("kod_ajani"));
        assert!(!delta.agent_logs.contains_key("test_ajani"));
        assert!(delta.events_log.is_none());
        assert!(delta.system_log.is_none());
        assert!(delta.messages.is_none());
        assert!(delta.all_projects.is_none());

        let json = serde_json::to_string(&delta).unwrap();
        assert!(!json.contains("all_projects"));
        assert!(!json.contains("messages"));
        assert!(!json.contains("test_ajani"));
    }

    #[test]
    fn snapshot_skips_duplicate_active_project_on_serialize() {
        let mut snap = sample_snap();
        snap.active_project = snap.all_projects.first().cloned();
        let json = serde_json::to_string(&snap).unwrap();
        assert!(!json.contains("\"active_project\""));
        assert!(json.contains("\"all_projects\""));
    }

    #[test]
    fn collect_descendant_pids_walks_multi_level_tree() {
        // 100 (app) -> 200 (webview2 browser) -> 201, 202 (renderer/gpu)
        // 999 (unrelated) -> 1000
        let pairs = vec![
            (100, 1),
            (200, 100),
            (201, 200),
            (202, 200),
            (300, 100),
            (999, 1),
            (1000, 999),
            (200, 200), // self-parent edge case
        ];
        let mut desc = collect_descendant_pids(100, &pairs);
        desc.sort();
        assert_eq!(desc, vec![200, 201, 202, 300]);
    }

    #[test]
    fn linux_standalone_blocks_unsafe_appimage_overwrite() {
        let standalone = evaluate_install_info("linux", None, false);
        assert_eq!(standalone.bundle_type, "standalone");
        assert!(!standalone.safe_self_update);
        assert!(standalone.reason.is_some());

        let deb = evaluate_install_info("linux", Some(tauri::utils::config::BundleType::Deb), false);
        assert_eq!(deb.bundle_type, "deb");
        assert!(deb.safe_self_update);

        let appimage = evaluate_install_info("linux", None, true);
        assert_eq!(appimage.bundle_type, "appimage");
        assert!(appimage.safe_self_update);

        let win = evaluate_install_info("windows", None, false);
        assert!(win.safe_self_update);
    }
}
