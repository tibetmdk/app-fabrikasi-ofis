export interface ConnectionProfile {
  id: string;
  name: string;
  mode: "local" | "remote";
  local_root: string;
  remote_url: string;
  remote_token: string;
}

export interface DictationConfig {
  provider: "groq" | "openai" | "custom";
  api_key: string;
  custom_url: string;
  model: string;
  language: string;
  auto_send: boolean;
}

export interface UiConfig {
  theme: "obsidian" | "terminal" | "midnight" | "slate";
  accent: "emerald" | "cyan" | "amber" | "violet";
  font_size: number;
  grid_columns: "auto" | "2" | "3";
  show_events: boolean;
  show_system: boolean;
  show_chat: boolean;
  visible_agents: string[];
  poll_interval_ms: number;
  notifications_enabled: boolean;
  compact_mode: boolean;
}

export interface AppSettings {
  profiles: ConnectionProfile[];
  active_profile_id: string;
  dictation: DictationConfig;
  ui: UiConfig;
}

export interface QuotaPool {
  id: string;
  name: string;
  has_data: boolean;
  p5: number;
  hafta: number;
  p5_sifir: number;
  hafta_sifir: number;
  guncelleme: number;
}

export interface ActiveAgentInfo {
  etiket: string;
  model: string;
  cli: string;
  baslangic: number;
  muadil: boolean;
  aciklama: string;
}

export interface ManagerContext {
  token: number;
  pencere: number;
  zaman: number;
  oturum: string | null;
  oturum_baslangic: number | null;
  son_ozet: { zaman: number; once: number; sonra: number } | null;
}

export interface ChatMessage {
  id: number;
  yon: string;
  metin: string;
  zaman: number;
  kanal: string;
  kaynak: string;
}

export interface JobItem {
  id: number;
  tur: string;
  kuyruk: string;
  durum: string;
  deneme: number;
  sonraki_deneme: number;
  hata: string | null;
}

export interface ProjectTask {
  id: string;
  adim: string;
  deneme: number;
  incelendi: boolean;
  son_hata: string | null;
  inceleme_duzeltmeleri: string[];
}

export interface ProjectInfo {
  ad: string;
  klasor: string;
  asama: string;
  asama_adi: string;
  su_an: string | null;
  expo_url: string | null;
  figma_url: string | null;
  gorevler: ProjectTask[];
  gorseller: string[];
  olusturma: number | null;
}

export interface OfficeSnapshot {
  connected: boolean;
  error: string | null;
  profile_name: string;
  mode: "local" | "remote";
  service_status: string;
  paused: boolean;
  manager_session: string | null;
  message_queue_count: number;
  manager_context: ManagerContext | null;
  quotas: QuotaPool[];
  active_agents: Record<string, ActiveAgentInfo>;
  assigned_models: Record<string, string>;
  agent_logs: Record<string, string[]>;
  events_log: string[];
  system_log: string[];
  messages: ChatMessage[];
  jobs: JobItem[];
  active_project: ProjectInfo | null;
  all_projects: ProjectInfo[];
  timestamp: number;
}

export interface GraphNode {
  id: string;
  label: string;
  path: string;
  folder: string;
  group: string;
  color: string;
  degree: number;
}

export interface GraphEdge {
  source: string;
  target: string;
}

export interface GraphColorGroup {
  name: string;
  color: string;
}

export interface GraphData {
  nodes: GraphNode[];
  edges: GraphEdge[];
  groups: GraphColorGroup[];
  show_arrow: boolean;
}

export interface NoteContent {
  id: string;
  path: string;
  title: string;
  folder: string;
  content: string;
  links: string[];
  backlinks: string[];
}

export interface ArchiveProject {
  name: string;
  files: string[];
}

export interface ArchiveOverview {
  archives: ArchiveProject[];
  journal: string[];
}

export interface MemoryStats {
  total_mb: number;
  rust_mb: number;
  webkit_mb: number;
  network_mb: number;
}

export interface ActionResult {
  ok: boolean;
  message: string;
}

export interface PollSnapshotResult {
  hash: string;
  snapshot: OfficeSnapshot | null;
}

export type TabId = "office" | "graph" | "project" | "archive" | "settings";

export const AGENT_DEFS: { id: string; key: string; name: string; short: string }[] = [
  { id: "yonetici", key: "1", name: "Yönetici", short: "Yönetici" },
  { id: "arastirmaci", key: "2", name: "Pazar Araştırmacısı", short: "Araştırmacı" },
  { id: "tasarim_lideri", key: "3", name: "Tasarım Ekibi Lideri", short: "Tasarım Lideri" },
  { id: "tasarimci", key: "4", name: "Figma Tasarımcısı", short: "Figma" },
  { id: "gorsel_ajani", key: "5", name: "Görsel Tasarımcı", short: "Görsel" },
  { id: "yazilim_lideri", key: "6", name: "Yazılım Ekibi Lideri", short: "Yazılım Lideri" },
  { id: "test_lideri", key: "7", name: "Test/Güvenlik Lideri", short: "Test Lideri" },
  { id: "kod_ajani", key: "8", name: "Kod Ajanı", short: "Kod Ajanı" },
  { id: "test_ajani", key: "9", name: "Test Ajanı", short: "Test Ajanı" },
];
