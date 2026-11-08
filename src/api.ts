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
  UpdateInfo,
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
  getSyncCount: () => invoke<number>("get_sync_count"),
  getAppInfo: () => invoke<AppInfo>("get_app_info"),
  /** 本应用当前物理内存（RAM）占用，字节 */
  getAppMemory: () => invoke<number>("get_app_memory"),
  openExternal: (url: string) => invoke<void>("open_external", { url }),
  markUiReady: () => invoke<void>("mark_ui_ready"),
  exitApp: () => invoke<void>("exit_app"),
  destroyMainWindow: () => invoke<void>("destroy_main_window"),
  checkUpdate: () => invoke<UpdateInfo>("check_update"),
  /** 测试通知：标题 / 内容 / 按钮可自定义，留空用后端默认文案 */
  testNotification: (opts?: { title?: string; body?: string; buttons?: string[] }) =>
    invoke<void>("test_notification", {
      title: opts?.title ?? null,
      body: opts?.body ?? null,
      buttons: opts?.buttons ?? null,
    }),
};
