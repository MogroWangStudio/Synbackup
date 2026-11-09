import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  FontFamilyInfo,
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
  /** 枚举系统已安装字体（按字族聚合，含本地化名称与语言标签） */
  listSystemFonts: () => invoke<FontFamilyInfo[]>("list_system_fonts"),
  /** 读取自定义图片 logo，返回 data URI（格式 / 大小校验在后端） */
  readLogoImage: (path: string) => invoke<string>("read_logo_image", { path }),
  openExternal: (url: string) => invoke<void>("open_external", { url }),
  /** 在系统文件管理器中打开文件夹（本地 / 设备卷内） */
  openPath: (path: string) => invoke<void>("open_path", { path }),
  markUiReady: () => invoke<void>("mark_ui_ready"),
  exitApp: () => invoke<void>("exit_app"),
  destroyMainWindow: () => invoke<void>("destroy_main_window"),
  checkUpdate: () => invoke<UpdateInfo>("check_update"),
  /** 下载本平台更新包（进度经 update-progress 事件回传），返回落地路径 */
  downloadUpdate: (version: string) => invoke<string>("download_update", { version }),
  /** 取消进行中的下载 */
  cancelUpdateDownload: () => invoke<void>("cancel_update_download"),
  /** 安装已下载的更新包并退出，由后台脚本替换文件并拉起新版本 */
  installUpdate: (version: string) => invoke<void>("install_update", { version }),
  /** 切换数据目录（复制现有数据 + 记下指针），重启后生效 */
  setDataDir: (path: string) => invoke<string>("set_data_dir", { path }),
  /** 重启应用（前端与后台一并退出，随后自动拉起） */
  restartApp: () => invoke<void>("restart_app"),
  /** 测试通知：标题 / 内容 / 按钮可自定义，留空用后端默认文案 */
  testNotification: (opts?: { title?: string; body?: string; buttons?: string[] }) =>
    invoke<void>("test_notification", {
      title: opts?.title ?? null,
      body: opts?.body ?? null,
      buttons: opts?.buttons ?? null,
    }),
};
