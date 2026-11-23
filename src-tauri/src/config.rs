use crate::models::{AppSettings, ConnectionProfile, DictationConfig, UiConfig};
use std::fs;
use std::path::PathBuf;

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

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            profiles: vec![
                ConnectionProfile {
                    id: "local".to_string(),
                    name: "Yerel Bilgisayar".to_string(),
                    mode: "local".to_string(),
                    local_root: default_local_root(),
                    remote_url: String::new(),
                    remote_token: String::new(),
                },
                ConnectionProfile {
                    id: "vds".to_string(),
                    name: "VDS Sunucu".to_string(),
                    mode: "remote".to_string(),
                    local_root: String::new(),
                    remote_url: "http://127.0.0.1:8787".to_string(),
                    remote_token: String::new(),
                },
            ],
            active_profile_id: "local".to_string(),
            dictation: DictationConfig {
                provider: "groq".to_string(),
                api_key: String::new(),
                custom_url: String::new(),
                model: "whisper-large-v3-turbo".to_string(),
                language: "tr".to_string(),
                auto_send: false,
            },
            ui: UiConfig {
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
            },
        }
    }
}

pub fn settings_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("app-fabrikasi-ofis").join("settings.json")
}

pub fn load_settings() -> AppSettings {
    let path = settings_path();
    if let Ok(raw) = fs::read_to_string(&path) {
        if let Ok(parsed) = serde_json::from_str::<AppSettings>(&raw) {
            return parsed;
        }
    }
    let defaults = AppSettings::default();
    let _ = save_settings(&defaults);
    defaults
}

pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

pub fn active_profile(settings: &AppSettings) -> ConnectionProfile {
    settings
        .profiles
        .iter()
        .find(|p| p.id == settings.active_profile_id)
        .or_else(|| settings.profiles.first())
        .cloned()
        .unwrap_or(ConnectionProfile {
            id: "local".to_string(),
            name: "Yerel".to_string(),
            mode: "local".to_string(),
            local_root: default_local_root(),
            remote_url: String::new(),
            remote_token: String::new(),
        })
}
