import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  AppSettings,
  ApplyResult,
  Config,
  Decision,
  HistoryEntry,
  PendingReview,
  SyncPair,
  VolumeInfo,
} from "./types";

export const api = {
  getConfig: () => invoke<Config>("get_config"),
  saveSettings: (settings: AppSettings) => invoke<void>("save_settings", { settings }),
  upsertPair: (pair: SyncPair) => invoke<void>("upsert_pair", { pair }),
  deletePair: (id: string) => invoke<void>("delete_pair", { id }),
  setPairEnabled: (id: string, enabled: boolean) =>
    invoke<void>("set_pair_enabled", { id, enabled }),
  listVolumes: () => invoke<VolumeInfo[]>("list_volumes"),
  scanPair: (pairId: string) => invoke<PendingReview>("scan_pair", { pairId }),
  applyDecisions: (pairId: string, decisions: Decision[]) =>
    invoke<ApplyResult>("apply_decisions", { pairId, decisions }),
  getPendingReviews: () => invoke<PendingReview[]>("get_pending_reviews"),
  dismissPendingReview: (pairId: string) => invoke<void>("dismiss_pending_review", { pairId }),
  getHistory: (limit = 200) => invoke<HistoryEntry[]>("get_history", { limit }),
  clearHistory: () => invoke<void>("clear_history"),
  checkAllDevices: () => invoke<number>("check_all_devices"),
  getAppInfo: () => invoke<AppInfo>("get_app_info"),
};
