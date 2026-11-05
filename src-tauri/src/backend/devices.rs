//! 移动存储设备枚举与识别。
//!
//! 识别策略：优先比对配对时写入设备的标记文件（卷标变更、盘符变化后依然有效），
//! 其次回退到卷标匹配。

use std::path::{Path, PathBuf};

use sysinfo::Disks;

use super::model::{SyncPair, VolumeInfo, MARKER_FILE};

/// 枚举当前接入的移动存储设备
pub fn list_volumes() -> Vec<VolumeInfo> {
    let disks = Disks::new_with_refreshed_list();
    let mut out = Vec::new();
    for disk in disks.list() {
        let mount = disk.mount_point();
        if !is_candidate_mount(mount) {
            continue;
        }
        let mount_str = normalize_mount(mount);
        let label = volume_label(disk, &mount_str);
        out.push(VolumeInfo {
            mount: mount_str,
            label,
            fs: disk.file_system().to_string_lossy().to_string(),
            total: disk.total_space(),
            available: disk.available_space(),
            matched_pair_ids: Vec::new(),
        });
    }
    out.sort_by(|a, b| a.mount.cmp(&b.mount));
    out
}

#[cfg(target_os = "macos")]
fn is_candidate_mount(mount: &Path) -> bool {
    let s = mount.to_string_lossy();
    // 排除根分区与 /System 下的数据卷，只保留 /Volumes 下的用户可见卷
    s.starts_with("/Volumes/") && s.len() > "/Volumes/".len()
}

#[cfg(target_os = "windows")]
fn is_candidate_mount(mount: &Path) -> bool {
    let s = mount.to_string_lossy();
    let bytes = s.as_bytes();
    if bytes.len() < 2 || bytes.len() > 3 || bytes[1] != b':' {
        return false;
    }
    let sys = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    !s[..2].eq_ignore_ascii_case(&sys)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn is_candidate_mount(_mount: &Path) -> bool {
    false
}

fn normalize_mount(mount: &Path) -> String {
    let s = mount.to_string_lossy().to_string();
    #[cfg(target_os = "macos")]
    {
        s.trim_end_matches('/').to_string()
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Windows 盘符根（C:\）必须保留尾部反斜杠，去掉会变成驱动器相对路径
        s
    }
}

fn volume_label(disk: &sysinfo::Disk, mount: &str) -> String {
    let name = disk.name().to_string_lossy().trim().to_string();
    if !name.is_empty() {
        name
    } else {
        Path::new(mount)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| mount.to_string())
    }
}

/// 清理相对路径：统一 "/" 分隔，去掉空段与 "."（"..",由校验层拒绝）
pub fn normalize_rel(input: &str) -> String {
    input
        .replace('\\', "/")
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect::<Vec<_>>()
        .join("/")
}

/// 卷内配对目录路径。rel 为空表示设备根目录。
pub fn remote_root(mount: &str, rel: &str) -> PathBuf {
    let mut p = PathBuf::from(mount);
    for part in rel.split('/') {
        if !part.is_empty() {
            p.push(part);
        }
    }
    p
}

/// 在已接入的卷中解析任务的设备侧目录：先按标记文件，再按卷标
pub fn resolve_remote_root(volumes: &[VolumeInfo], pair: &SyncPair) -> Option<PathBuf> {
    let rel = normalize_rel(&pair.remote_path);
    if let Some(expected) = pair.device_marker.as_deref() {
        if !expected.is_empty() {
            for v in volumes {
                let root = remote_root(&v.mount, &rel);
                if read_marker(&root).as_deref() == Some(expected) {
                    return Some(root);
                }
            }
        }
    }
    for v in volumes {
        if v.label == pair.device_label {
            return Some(remote_root(&v.mount, &rel));
        }
    }
    None
}

/// 裸卷列表 + 任务补全一步到位。前端命令返回与后台推送共用，
/// 只取裸卷列表会让前端拿到 `matchedPairIds: []`，刚插入的设备
/// 在界面上仍显示"设备未连接"。
pub fn snapshot_with_pairs(pairs: &[SyncPair]) -> Vec<VolumeInfo> {
    let mut vols = list_volumes();
    attach_pairs(&mut vols, pairs);
    vols
}

/// 为卷列表补上匹配到的同步任务 id。
pub fn attach_pairs(volumes: &mut [VolumeInfo], pairs: &[SyncPair]) {
    for v in volumes.iter_mut() {
        // 先复制一份只读快照，避免同时持有可变与不可变借用
        let probe = v.clone();
        let ids: Vec<String> = pairs
            .iter()
            .filter(|p| p.enabled && resolve_remote_root(std::slice::from_ref(&probe), p).is_some())
            .map(|p| p.id.clone())
            .collect();
        v.matched_pair_ids = ids;
    }
}

pub fn read_marker(root: &Path) -> Option<String> {
    std::fs::read_to_string(root.join(MARKER_FILE))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 确保配对目录存在并写入设备标记（已存在则不覆盖）
pub fn ensure_marker(root: &Path, pair: &SyncPair) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    if read_marker(root).is_some() {
        return Ok(());
    }
    let id = pair
        .device_marker
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    std::fs::write(root.join(MARKER_FILE), id)
}
