use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConnectionProfile {
    pub id: String,
    pub name: String,
    pub mode: String, // "local" | "remote"
    pub local_root: String,
    pub remote_url: String,
    pub remote_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DictationConfig {
    pub provider: String, // "groq" | "openai" | "custom"
    pub api_key: String,
    pub custom_url: String,
    pub model: String,
    pub language: String,
    pub auto_send: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub theme: String,        // "obsidian" | "terminal" | "midnight" | "slate"
    pub accent: String,       // "emerald" | "cyan" | "amber" | "violet"
    pub font_size: u32,       // 11..16
    pub grid_columns: String, // "auto" | "2" | "3"
    pub show_events: bool,
    pub show_system: bool,
    pub show_chat: bool,
    pub visible_agents: Vec<String>,
    pub poll_interval_ms: u64,
    pub notifications_enabled: bool,
    pub compact_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub profiles: Vec<ConnectionProfile>,
    pub active_profile_id: String,
    pub dictation: DictationConfig,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaPool {
    pub id: String,
    pub name: String,
    pub has_data: bool,
    pub p5: f64,
    pub hafta: f64,
    pub p5_sifir: f64,
    pub hafta_sifir: f64,
    pub guncelleme: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveAgentInfo {
    pub etiket: String,
    pub model: String,
    pub cli: String,
    pub baslangic: f64,
    pub muadil: bool,
    pub aciklama: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagerContext {
    pub token: u64,
    pub pencere: u64,
    pub zaman: f64,
    pub oturum: Option<String>,
    pub oturum_baslangic: Option<f64>,
    pub son_ozet: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: i64,
    pub yon: String,
    pub metin: String,
    pub zaman: f64,
    pub kanal: String,
    pub kaynak: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobItem {
    pub id: i64,
    pub tur: String,
    pub kuyruk: String,
    pub durum: String,
    pub deneme: i64,
    pub sonraki_deneme: f64,
    pub hata: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectTask {
    pub id: String,
    pub adim: String,
    pub deneme: i64,
    pub incelendi: bool,
    pub son_hata: Option<String>,
    pub inceleme_duzeltmeleri: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub ad: String,
    pub klasor: String,
    pub asama: String,
    pub asama_adi: String,
    pub su_an: Option<String>,
    pub expo_url: Option<String>,
    pub figma_url: Option<String>,
    pub gorevler: Vec<ProjectTask>,
    pub gorseller: Vec<String>,
    pub olusturma: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficeSnapshot {
    pub connected: bool,
    pub error: Option<String>,
    pub profile_name: String,
    pub mode: String,
    pub service_status: String, // "active" | "inactive" | "activating" | "unknown"
    pub paused: bool,
    pub manager_session: Option<String>,
    pub message_queue_count: usize,
    pub manager_context: Option<ManagerContext>,
    pub quotas: Vec<QuotaPool>,
    pub active_agents: BTreeMap<String, ActiveAgentInfo>,
    pub assigned_models: BTreeMap<String, String>,
    pub agent_logs: BTreeMap<String, Vec<String>>,
    pub events_log: Vec<String>,
    pub system_log: Vec<String>,
    pub messages: Vec<ChatMessage>,
    pub jobs: Vec<JobItem>,
    #[allow(dead_code)]
    #[serde(default, skip_serializing)]
    pub active_project: Option<ProjectInfo>,
    pub all_projects: Vec<ProjectInfo>,
    pub timestamp: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub path: String,
    pub folder: String,
    pub group: String,
    pub color: String,
    pub degree: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphColorGroup {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub groups: Vec<GraphColorGroup>,
    pub show_arrow: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteContent {
    pub id: String,
    pub path: String,
    pub title: String,
    pub folder: String,
    pub content: String,
    pub links: Vec<String>,
    pub backlinks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveProject {
    pub name: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveOverview {
    pub archives: Vec<ArchiveProject>,
    pub journal: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_mb: f64,
    pub rust_mb: f64,
    pub webkit_mb: f64,
    pub network_mb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDelta {
    pub connected: bool,
    pub error: Option<String>,
    pub profile_name: String,
    pub mode: String,
    pub service_status: String,
    pub paused: bool,
    pub manager_session: Option<String>,
    pub message_queue_count: usize,
    pub manager_context: Option<ManagerContext>,
    pub quotas: Vec<QuotaPool>,
    pub active_agents: BTreeMap<String, ActiveAgentInfo>,
    pub assigned_models: BTreeMap<String, String>,
    pub jobs: Vec<JobItem>,
    pub timestamp: f64,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub agent_logs: BTreeMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events_log: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_log: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<ChatMessage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_projects: Option<Vec<ProjectInfo>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollSnapshotResult {
    pub hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<OfficeSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<SnapshotDelta>,
}
