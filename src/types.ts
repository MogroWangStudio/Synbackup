// 与 Rust 后端 serde 模型一一对应（camelCase）

export interface AppSettings {
  language: string;
  launchAtLogin: boolean;
  autoScanOnInsert: boolean;
  autoSyncNoConflict: boolean;
  notifyOnInsert: boolean;
  /** 外观：follow = 跟随系统，light / dark 为强制指定 */
  appearance: string;
  /** 强调色预设 key（brand / blue / green / orange / pink / custom） */
  accent: string;
  /** 自定义强调色（#RRGGBB，accent = custom 时生效） */
  accentCustom: string;
  /** 界面字体预设 key（system / serif / rounded） */
  fontFamily: string;
  /** 字体缩放（0.9–1.15） */
  fontScale: number;
  /** 界面缩放（0.85–1.25） */
  uiScale: number;
  /** 启动入场动画 */
  launchAnimation: boolean;
  /** 启动动画预设（fade / rise / zoom / slide） */
  launchAnimPreset: string;
  /** 启动动画时长（毫秒，200–1200） */
  launchAnimMs: number;
  /** 启动动画曲线（smooth / spring / linear / swift） */
  launchAnimCurve: string;
  /** 更新渠道：stable = 正式版（Release），beta = 预发布（Pre-Release） */
  updateChannel: string;
  /** 侧栏状态区：显示设备状态 */
  statusDevices: boolean;
  /** 侧栏状态区：显示已开启任务数 */
  statusPairs: boolean;
  /** 侧栏状态区：累计同步次数 */
  statusSyncCount: boolean;
  /** 侧栏状态区：版本号 */
  statusVersion: boolean;
}

/** 任务的主传输方向:push = 本机发送(本地 → 设备),pull = 本机接收(设备 → 本地) */
export type SyncDirection = "push" | "pull";

export interface SyncPair {
  id: string;
  name: string;
  localPath: string;
  deviceLabel: string;
  deviceMarker: string | null;
  remotePath: string;
  direction: SyncDirection;
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

/** 本次同步由谁触发：manual = 差异对话框确认，auto = 插入设备后自动执行 */
export type Trigger = "manual" | "auto";

/** 单个文件的执行结果 */
export interface ApplyOutcome {
  path: string;
  action: SyncAction;
  /** 是否按计划完成（跳过算完成，失败与"文件已消失"为 false） */
  ok: boolean;
  error: string | null;
  /** 文件大小（复制 = 源文件，删除 = 被删文件；目录或旧记录为 null） */
  size: number | null;
}

export interface ApplyResult {
  copiedToRemote: number;
  copiedToLocal: number;
  deletedRemote: number;
  deletedLocal: number;
  skipped: number;
  /** 本次成功动作涉及的字节总量（旧记录为 0） */
  totalBytes: number;
  errors: ApplyError[];
  items: ApplyOutcome[];
}

export interface VolumeInfo {
  mount: string;
  label: string;
  fs: string;
  total: number;
  available: number;
  matchedPairIds: string[];
}

/** 检查到差异的单文件记录 */
export interface ScanDiff {
  path: string;
  status: DiffStatus;
  /** 涉及文件大小（取两侧较大者；目录为 null） */
  size: number | null;
}

export interface HistoryEntry {
  ts: number;
  /** 记录类型：sync = 同步执行（旧记录缺省），scan = 检查到差异 */
  kind: "sync" | "scan";
  trigger: Trigger;
  pairId: string;
  pairName: string;
  deviceLabel: string;
  copiedToRemote: number;
  copiedToLocal: number;
  deletedRemote: number;
  deletedLocal: number;
  skipped: number;
  /** 本次同步涉及的字节总量（旧记录为 0） */
  totalBytes: number;
  errors: ApplyError[];
  /** 逐项明细（最多 200 条，见后端 HISTORY_ITEM_LIMIT） */
  items: ApplyOutcome[];
  /** 超出上限、未记录进历史的明细条数 */
  itemsTruncated: number;
  /** kind = scan 时的差异清单 */
  scanDiffs: ScanDiff[] | null;
  /** kind = scan 时的差异总数 */
  scanTotal: number;
}

export interface AppInfo {
  dataDir: string;
  version: string;
}

/** 更新检查结果（后端查询 GitHub Releases 并按渠道匹配） */
export interface UpdateInfo {
  available: boolean;
  version: string;
  /** 发布说明（markdown 原文，截断到 2000 字符） */
  notes: string;
  /** 下载页地址（浏览器打开） */
  url: string;
  current: string;
}
