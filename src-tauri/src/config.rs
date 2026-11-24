use crate::models::{AppSettings, ConnectionProfile, DictationConfig, UiConfig};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn default_agents() -> Vec<String> {
    vec![
        "yonetici",
        "arastirmaci",
        "tasarim_lideri",
        "tasarimci",
        "gorsel_ajani",
        "yazilim_lideri",
        "test_lideri",
        "kod_ajani",
        "test_ajani",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

pub fn default_local_root() -> String {
    if let Some(home) = dirs::home_dir() {
        let desktop_candidate = home.join("Desktop").join("AppFabrikasi");
        if desktop_candidate.exists() {
            return desktop_candidate.to_string_lossy().to_string();
        }
        return desktop_candidate.to_string_lossy().to_string();
    }
    "/home/tibet/Desktop/AppFabrikasi".to_string()
}

impl Default for ConnectionProfile {
    fn default() -> Self {
        Self {
            id: "local".to_string(),
            name: "Yerel Bilgisayar".to_string(),
            mode: "local".to_string(),
            local_root: default_local_root(),
            remote_url: String::new(),
            remote_token: String::new(),
        }
    }
}

impl Default for DictationConfig {
    fn default() -> Self {
        Self {
            provider: "groq".to_string(),
            api_key: String::new(),
            custom_url: String::new(),
            model: "whisper-large-v3-turbo".to_string(),
            language: "tr".to_string(),
            auto_send: false,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "obsidian".to_string(),
            accent: "emerald".to_string(),
            font_size: 12,
            grid_columns: "auto".to_string(),
            show_events: true,
            show_system: true,
            show_chat: true,
            visible_agents: default_agents(),
            poll_interval_ms: 1000,
            notifications_enabled: true,
            compact_mode: false,
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        #[cfg(target_os = "windows")]
        let default_active = "vds".to_string();
        #[cfg(not(target_os = "windows"))]
        let default_active = "local".to_string();

        Self {
            profiles: vec![
                ConnectionProfile::default(),
                ConnectionProfile {
                    id: "vds".to_string(),
                    name: "VDS Sunucu".to_string(),
                    mode: "remote".to_string(),
                    local_root: String::new(),
                    remote_url: "http://127.0.0.1:8787".to_string(),
                    remote_token: String::new(),
                },
            ],
            active_profile_id: default_active,
            dictation: DictationConfig::default(),
            ui: UiConfig::default(),
        }
    }
}

pub fn settings_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("app-fabrikasi-ofis").join("settings.json")
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

fn normalize_settings(mut settings: AppSettings) -> AppSettings {
    if settings.profiles.is_empty() {
        settings.profiles = AppSettings::default().profiles;
    }
    if !settings
        .profiles
        .iter()
        .any(|p| p.id == settings.active_profile_id)
    {
        settings.active_profile_id = settings.profiles[0].id.clone();
    }
    if settings.ui.visible_agents.is_empty() {
        settings.ui.visible_agents = default_agents();
    }
    if settings.ui.poll_interval_ms < 400 {
        settings.ui.poll_interval_ms = 1000;
    }
    settings
}

fn load_settings_from_path(path: &Path) -> AppSettings {
    let bak = backup_path(path);

    if path.exists() {
        if let Ok(raw) = fs::read_to_string(path) {
            if let Ok(parsed) = serde_json::from_str::<AppSettings>(&raw) {
                return normalize_settings(parsed);
            }
        }
        // Mevcut dosya okunamadı/bozuksa asla üstüne varsayılan yazıp silme; yedekle
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let corrupt = path.with_extension(format!("json.corrupt.{ts}"));
        let _ = fs::copy(path, &corrupt);

        if let Ok(raw_bak) = fs::read_to_string(&bak) {
            if let Ok(parsed) = serde_json::from_str::<AppSettings>(&raw_bak) {
                let normalized = normalize_settings(parsed);
                let _ = save_settings_to_path(&normalized, path);
                return normalized;
            }
        }
        return AppSettings::default();
    }

    // Ana dosya yoksa ama yedek varsa yedekten kurtar
    if let Ok(raw_bak) = fs::read_to_string(&bak) {
        if let Ok(parsed) = serde_json::from_str::<AppSettings>(&raw_bak) {
            let normalized = normalize_settings(parsed);
            let _ = save_settings_to_path(&normalized, path);
            return normalized;
        }
    }

    let defaults = AppSettings::default();
    let _ = save_settings_to_path(&defaults, path);
    defaults
}

fn save_settings_to_path(settings: &AppSettings, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    let bak = backup_path(path);

    // Önce geçici dosyaya yaz, sonra atomik olarak taşı
    fs::write(&tmp, &json).map_err(|e| e.to_string())?;

    // Mevcut geçerli ayarı .bak olarak sakla
    if path.exists() {
        if let Ok(existing) = fs::read_to_string(path) {
            if serde_json::from_str::<AppSettings>(&existing).is_ok() {
                let _ = fs::write(&bak, &existing);
            }
        }
    }

    if let Err(_) = fs::rename(&tmp, path) {
        fs::write(path, &json).map_err(|e| e.to_string())?;
        let _ = fs::remove_file(&tmp);
    }

    // Güncel geçerli ayarın yedeğini de güncel tut
    let _ = fs::write(&bak, &json);
    Ok(())
}

fn try_apply_bootstrap_file(settings: &mut AppSettings, candidate: &Path) -> bool {
    let Ok(raw) = fs::read_to_string(candidate) else {
        return false;
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    let url = val
        .get("remote_url")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("");
    let token = val
        .get("remote_token")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("");
    if url.is_empty() || token.is_empty() {
        return false;
    }
    let name = val
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("VDS Sunucu");

    if let Some(p) = settings.profiles.iter_mut().find(|p| p.id == "vds") {
        p.mode = "remote".to_string();
        p.name = name.to_string();
        p.remote_url = url.to_string();
        p.remote_token = token.to_string();
    } else {
        settings.profiles.push(ConnectionProfile {
            id: "vds".to_string(),
            name: name.to_string(),
            mode: "remote".to_string(),
            local_root: String::new(),
            remote_url: url.to_string(),
            remote_token: token.to_string(),
        });
    }
    settings.active_profile_id = "vds".to_string();
    true
}

fn maybe_import_bootstrap(settings: &mut AppSettings, settings_file: &Path) {
    let has_remote_token = settings
        .profiles
        .iter()
        .any(|p| p.mode == "remote" && !p.remote_token.trim().is_empty());
    if has_remote_token {
        return;
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("app-fabrikasi-baglanti.json"));
        }
    }
    if let Some(d) = dirs::desktop_dir() {
        candidates.push(d.join("app-fabrikasi-baglanti.json"));
    }
    if let Some(d) = dirs::download_dir() {
        candidates.push(d.join("app-fabrikasi-baglanti.json"));
    }

    for cand in candidates {
        if cand.exists() && try_apply_bootstrap_file(settings, &cand) {
            let _ = save_settings_to_path(settings, settings_file);
            break;
        }
    }
}

pub fn load_settings() -> AppSettings {
    let path = settings_path();
    let mut settings = load_settings_from_path(&path);
    maybe_import_bootstrap(&mut settings, &path);
    settings
}

pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let path = settings_path();
    save_settings_to_path(settings, &path)
}

pub fn active_profile(settings: &AppSettings) -> ConnectionProfile {
    settings
        .profiles
        .iter()
        .find(|p| p.id == settings.active_profile_id)
        .or_else(|| settings.profiles.first())
        .cloned()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_legacy_settings_preserve_vds_token() {
        let dir = std::env::temp_dir().join(format!(
            "af_ofis_test_partial_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");

        // Eski/eksik alanlı JSON: ui ve dictation eksik olsa bile VDS profili korunmalı
        let partial = r#"{
            "profiles": [
                {
                    "id": "vds",
                    "name": "Benim VDS",
                    "mode": "remote",
                    "remote_url": "http://10.0.0.1:8787",
                    "remote_token": "gizli-test-token"
                }
            ],
            "active_profile_id": "vds"
        }"#;
        fs::write(&path, partial).unwrap();

        let loaded = load_settings_from_path(&path);
        assert_eq!(loaded.active_profile_id, "vds");
        assert_eq!(loaded.profiles.len(), 1);
        assert_eq!(loaded.profiles[0].remote_url, "http://10.0.0.1:8787");
        assert_eq!(loaded.profiles[0].remote_token, "gizli-test-token");
        assert!(!loaded.ui.visible_agents.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_settings_recovers_from_backup() {
        let dir = std::env::temp_dir().join(format!(
            "af_ofis_test_corrupt_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");

        let mut s = AppSettings::default();
        s.profiles[1].remote_url = "http://10.0.0.2:8787".to_string();
        s.profiles[1].remote_token = "yedek-token".to_string();
        s.active_profile_id = "vds".to_string();
        save_settings_to_path(&s, &path).unwrap();

        // Ana dosyayı boz
        fs::write(&path, "{ bozuk json").unwrap();

        let recovered = load_settings_from_path(&path);
        assert_eq!(recovered.active_profile_id, "vds");
        assert_eq!(recovered.profiles[1].remote_token, "yedek-token");

        let _ = fs::remove_dir_all(&dir);
    }
}
