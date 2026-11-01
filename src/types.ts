// 与 Rust 后端 serde 模型一一对应（camelCase）

export interface AppSettings {
  language: string;
  launchAtLogin: boolean;
  autoScanOnInsert: boolean;
  autoSyncNoConflict: boolean;
  notifyOnInsert: boolean;
  pollIntervalSecs: number;
}

export interface SyncPair {
  id: string;
  name: string;
  localPath: string;
  deviceLabel: string;
  deviceMarker: string | null;
  remotePath: string;
  enabled: boolean;
  createdAt: number;
  lastSyncAt: number | null;
}

export interface Config {
  settings: AppSettings;
  pairs: SyncPair[];
}

export type DiffStatus =
  | "new_local"
  | "new_remote"
  | "changed_local"
  | "changed_remote"
  | "both_changed"
  | "deleted_remote"
  | "deleted_local"
  | "type_conflict";

export type SyncAction =
  | "local_to_remote"
  | "remote_to_local"
  | "delete_remote"
  | "delete_local"
  | "skip";

export interface FileStat {
  size: number;
  mtimeMs: number;
}

export interface DiffItem {
  path: string;
  status: DiffStatus;
  local: FileStat | null;
  remote: FileStat | null;
  suggested: SyncAction;
}

export interface PendingReview {
  pairId: string;
  pairName: string;
  deviceLabel: string;
  scannedAt: number;
  items: DiffItem[];
}

export interface Decision {
  path: string;
  action: SyncAction;
}

export interface ApplyError {
  path: string;
  message: string;
}

export interface ApplyResult {
  copiedToRemote: number;
  copiedToLocal: number;
  deletedRemote: number;
  deletedLocal: number;
  skipped: number;
  errors: ApplyError[];
}

export interface VolumeInfo {
  mount: string;
  label: string;
  fs: string;
  total: number;
  available: number;
  matchedPairIds: string[];
}

export interface HistoryEntry {
  ts: number;
  pairId: string;
  pairName: string;
  deviceLabel: string;
  copiedToRemote: number;
  copiedToLocal: number;
  deletedRemote: number;
  deletedLocal: number;
  skipped: number;
  errors: ApplyError[];
}

export interface AppInfo {
  dataDir: string;
  version: string;
}
