import { invoke } from "@tauri-apps/api/core";
import type {
  ActionResult,
  AppSettings,
  ArchiveOverview,
  ConnectionProfile,
  GraphData,
  MemoryStats,
  NoteContent,
  OfficeSnapshot,
  PollSnapshotResult,
} from "./types";

export interface InstallInfo {
  os: string;
  bundle_type: string;
  safe_self_update: boolean;
  reason?: string | null;
}

export const api = {
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => invoke<void>("save_settings", { settings }),
  getMemoryStats: () => invoke<MemoryStats>("get_memory_stats"),
  getInstallInfo: () => invoke<InstallInfo>("get_install_info"),
  getSnapshot: () => invoke<OfficeSnapshot>("get_snapshot"),
  pollSnapshot: (lastHash: string) =>
    invoke<PollSnapshotResult>("poll_snapshot", { lastHash }),
  getFullLog: (name: string, maxLines = 1500) =>
    invoke<string[]>("get_full_log", { name, maxLines }),
  getGraph: () => invoke<GraphData>("get_graph"),
  getNote: (noteId: string) => invoke<NoteContent>("get_note", { noteId }),
  getArchives: () => invoke<ArchiveOverview>("get_archives"),
  getArchiveFile: (archiveName: string, fileName: string) =>
    invoke<string[]>("get_archive_file", { archiveName, fileName }),
  sendMessage: (text: string) => invoke<ActionResult>("send_message", { text }),
  controlFactory: (action: "durdur" | "devam" | "arastir" | "start" | "stop" | "restart") =>
    invoke<ActionResult>("control_factory", { action }),
  startDictation: () => invoke<void>("start_dictation"),
  stopDictation: () => invoke<string>("stop_dictation"),
  testConnection: (profile: ConnectionProfile) =>
    invoke<ActionResult>("test_connection", { profile }),
};
