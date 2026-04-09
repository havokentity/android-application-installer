// ─── Shared Types ─────────────────────────────────────────────────────────────

export interface DeviceInfo {
  serial: string;
  state: string;
  model: string;
  product: string;
  transport_id: string;
}

export interface LogEntry {
  id: number;
  time: string;
  level: "info" | "success" | "error" | "warning";
  message: string;
}

export interface ToolsStatus {
  adb_installed: boolean;
  adb_path: string;
  bundletool_installed: boolean;
  bundletool_path: string;
  java_installed: boolean;
  java_path: string;
  data_dir: string;
}

export interface DownloadProgress {
  tool: string;
  downloaded: number;
  total: number;
  percent: number;
  status: string; // "downloading" | "extracting" | "done" | "error"
}

export interface StaleTool {
  tool: string;
  label: string;
  last_updated_secs: number;
  age_days: number;
}

export type DetectionStatus = "unknown" | "found" | "not-found";

export interface RecentFile {
  path: string;
  name: string;
  last_used: number;
}

export interface RecentFilesConfig {
  packages: RecentFile[];
  keystores: RecentFile[];
}

