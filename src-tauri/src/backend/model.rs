//! 共享数据模型：配置、同步任务、差异报告、执行结果。
//! 序列化统一使用 camelCase，与前端 TypeScript 类型一一对应。

use serde::{Deserialize, Serialize};

/// 设备配对标记文件：写入配对目录内，用于在卷标变更后仍能识别同一设备。
pub const MARKER_FILE: &str = ".synbackup-device";

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
    pub poll_interval_secs: u64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "zh-CN".into(),
            launch_at_login: false,
            auto_scan_on_insert: true,
            auto_sync_no_conflict: false,
            notify_on_insert: true,
            poll_interval_secs: 3,
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub copied_to_remote: u32,
    pub copied_to_local: u32,
    pub deleted_remote: u32,
    pub deleted_local: u32,
    pub skipped: u32,
    pub errors: Vec<ApplyError>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub ts: i64,
    pub pair_id: String,
    pub pair_name: String,
    pub device_label: String,
    pub copied_to_remote: u32,
    pub copied_to_local: u32,
    pub deleted_remote: u32,
    pub deleted_local: u32,
    pub skipped: u32,
    pub errors: Vec<ApplyError>,
}
