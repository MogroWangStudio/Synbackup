//! 共享数据模型：配置、同步任务、差异报告、执行结果。
//! 序列化统一使用 camelCase，与前端 TypeScript 类型一一对应。

use serde::{Deserialize, Serialize};

/// 设备配对标记文件：写入配对目录内，用于在卷标变更后仍能识别同一设备。
pub const MARKER_FILE: &str = ".synbackup-device";

pub fn now_ms() -> i64 {
    time_ms(std::time::SystemTime::now())
}

pub fn time_ms(t: std::time::SystemTime) -> i64 {
    t.duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub language: String,
    pub launch_at_login: bool,
    /// 插入设备时自动检查差异
    pub auto_scan_on_insert: bool,
    /// 无冲突时按建议方向自动同步（冲突与删除永远需要确认）
    pub auto_sync_no_conflict: bool,
    pub notify_on_insert: bool,
    /// 外观：follow = 跟随系统，light / dark 为强制指定
    pub appearance: String,
    /// 强调色预设 key（brand / blue / green / orange / pink）
    pub accent: String,
    /// 自定义强调色（#RRGGBB，accent = custom 时生效）
    pub accent_custom: String,
    /// 界面字体预设 key（system / serif / rounded）
    pub font_family: String,
    /// 字体缩放（0.9–1.15），作用于全部文本
    pub font_scale: f32,
    /// 界面缩放（0.85–1.25），作用于侧栏与内容区
    pub ui_scale: f32,
    /// 启动入场动画
    pub launch_animation: bool,
    /// 启动动画预设（fade / rise / zoom / slide）
    pub launch_anim_preset: String,
    /// 启动动画时长（毫秒，200–1200）
    pub launch_anim_ms: u32,
    /// 启动动画曲线（smooth / spring / linear / swift）
    pub launch_anim_curve: String,
    /// 更新渠道：stable = 正式版（Release），beta = 预发布（Pre-Release）
    pub update_channel: String,
    /// 侧栏状态区：显示设备状态
    pub status_devices: bool,
    /// 侧栏状态区：显示已开启任务数
    pub status_pairs: bool,
    /// 侧栏状态区：显示累计同步次数
    pub status_sync_count: bool,
    /// 侧栏状态区：显示版本号
    pub status_version: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "zh-CN".into(),
            launch_at_login: false,
            auto_scan_on_insert: true,
            auto_sync_no_conflict: false,
            notify_on_insert: true,
            appearance: "follow".into(),
            accent: "brand".into(),
            accent_custom: "#9b62cf".into(),
            font_family: "system".into(),
            font_scale: 1.0,
            ui_scale: 1.0,
            launch_animation: true,
            launch_anim_preset: "rise".into(),
            launch_anim_ms: 340,
            launch_anim_curve: "smooth".into(),
            update_channel: "stable".into(),
            status_devices: true,
            status_pairs: true,
            status_sync_count: true,
            status_version: false,
        }
    }
}

/// SyncPair.direction 的默认值：本机为发送端（本地 → 设备）
fn default_direction() -> String {
    "push".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncPair {
    pub id: String,
    pub name: String,
    /// 本地文件夹绝对路径
    pub local_path: String,
    /// 目标设备卷标（首选 marker 匹配，卷标作为回退）
    pub device_label: String,
    /// 设备标记 UUID（首次配对时生成并写入设备）
    pub device_marker: Option<String>,
    /// 设备上的相对文件夹路径（空串表示设备根目录）
    pub remote_path: String,
    /// 主传输方向：push = 本机发送（本地 → 设备），pull = 本机接收（设备 → 本地）。
    /// 仅标识任务语义与界面展示，两侧内容仍按差异双向同步。
    #[serde(default = "default_direction")]
    pub direction: String,
    pub enabled: bool,
    pub created_at: i64,
    pub last_sync_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub settings: AppSettings,
    pub pairs: Vec<SyncPair>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    /// 仅本地存在
    NewLocal,
    /// 仅设备存在
    NewRemote,
    /// 相对上次同步，本地有修改
    ChangedLocal,
    /// 相对上次同步，设备有修改
    ChangedRemote,
    /// 两侧都有修改，需要用户裁决
    BothChanged,
    /// 上次同步后设备侧文件被删除
    DeletedRemote,
    /// 上次同步后本地侧文件被删除
    DeletedLocal,
    /// 两侧类型不一致（文件 vs 目录）
    TypeConflict,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SyncAction {
    /// 复制本地 → 设备
    LocalToRemote,
    /// 复制设备 → 本地
    RemoteToLocal,
    /// 删除设备侧文件
    DeleteRemote,
    /// 删除本地侧文件
    DeleteLocal,
    /// 本次不处理
    Skip,
}

/// 本次同步由谁触发：历史记录据此区分自动与手动。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    /// 用户在差异对话框里确认后执行
    #[default]
    Manual,
    /// 插入设备后按"无冲突自动同步"规则自动执行
    Auto,
}

/// 单个文件的执行结果：历史明细与结果展示都从这里来。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOutcome {
    pub path: String,
    pub action: SyncAction,
    /// 是否按计划完成（跳过视为完成，失败与"文件已消失"为 false）
    pub ok: bool,
    /// 未完成的原因
    pub error: Option<String>,
    /// 本次动作涉及的文件大小（复制 = 源文件，删除 = 被删文件；目录或未知为 null，
    /// 旧记录无此字段时也为 null）
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileStat {
    pub size: u64,
    pub mtime_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffItem {
    /// 相对同步根目录的路径，统一使用 "/" 分隔
    pub path: String,
    pub status: DiffStatus,
    pub local: Option<FileStat>,
    pub remote: Option<FileStat>,
    pub suggested: SyncAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingReview {
    pub pair_id: String,
    pub pair_name: String,
    pub device_label: String,
    pub scanned_at: i64,
    pub items: Vec<DiffItem>,
}

impl PendingReview {
    pub fn new(pair: &SyncPair, items: Vec<DiffItem>) -> Self {
        Self {
            pair_id: pair.id.clone(),
            pair_name: pair.name.clone(),
            device_label: pair.device_label.clone(),
            scanned_at: now_ms(),
            items,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub path: String,
    pub action: SyncAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyError {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ApplyResult {
    pub copied_to_remote: u32,
    pub copied_to_local: u32,
    pub deleted_remote: u32,
    pub deleted_local: u32,
    pub skipped: u32,
    /// 本次成功动作涉及的字节总量（复制 + 删除；旧记录无此字段时为 0）
    pub total_bytes: u64,
    pub errors: Vec<ApplyError>,
    /// 逐项执行明细，顺序与决策顺序一致
    pub items: Vec<ApplyOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VolumeInfo {
    /// 挂载点（macOS: /Volumes/xxx，Windows: X:\）
    pub mount: String,
    pub label: String,
    pub fs: String,
    pub total: u64,
    pub available: u64,
    /// 当前匹配到的同步任务 id 列表
    pub matched_pair_ids: Vec<String>,
}

/// 同步历史条目。
///
/// 加了 `default`：0.1.0 写入的旧记录（无 trigger / items 字段）依然能读出，
/// 缺失字段回落到默认值而不是整行被丢弃。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HistoryEntry {
    /// 同步完成时间
    pub ts: i64,
    /// 记录类型：sync = 同步执行（旧记录缺省），scan = 检查到差异
    #[serde(default = "default_history_kind")]
    pub kind: String,
    pub trigger: Trigger,
    pub pair_id: String,
    pub pair_name: String,
    pub device_label: String,
    pub copied_to_remote: u32,
    pub copied_to_local: u32,
    pub deleted_remote: u32,
    pub deleted_local: u32,
    pub skipped: u32,
    /// 本次成功动作涉及的字节总量（旧记录无此字段时为 0）
    pub total_bytes: u64,
    pub errors: Vec<ApplyError>,
    /// 逐项执行明细（条数上限见 backend::HISTORY_ITEM_LIMIT）
    pub items: Vec<ApplyOutcome>,
    /// 超出上限未记录的明细条数
    pub items_truncated: u32,
    /// kind = scan 时的差异清单（与 items 同受上限约束）
    pub scan_diffs: Option<Vec<ScanDiff>>,
    /// kind = scan 时的差异总数（明细截断也不失真）
    pub scan_total: u32,
}

/// 检查到差异的单文件记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanDiff {
    pub path: String,
    pub status: DiffStatus,
    /// 涉及文件大小（取两侧较大者；目录为 null）
    pub size: Option<u64>,
}

fn default_history_kind() -> String {
    "sync".into()
}

impl Default for HistoryEntry {
    fn default() -> Self {
        Self {
            ts: 0,
            trigger: Trigger::Manual,
            pair_id: String::new(),
            pair_name: String::new(),
            device_label: String::new(),
            copied_to_remote: 0,
            copied_to_local: 0,
            deleted_remote: 0,
            deleted_local: 0,
            skipped: 0,
            total_bytes: 0,
            errors: Vec::new(),
            items: Vec::new(),
            items_truncated: 0,
            kind: "sync".into(),
            scan_diffs: None,
            scan_total: 0,
        }
    }
}
