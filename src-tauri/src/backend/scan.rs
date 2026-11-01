//! 差异扫描：遍历本地与设备两侧目录树，结合上次同步清单，
//! 为每个差异文件给出状态与建议操作。

use std::collections::BTreeMap;
use std::path::Path;

use super::manifest::Manifest;
use super::model::{DiffItem, DiffStatus, FileStat, SyncAction};

/// 跨文件系统（如 FAT32）修改时间可能有 1-2 秒偏移，视为相同
const MTIME_TOLERANCE_MS: i64 = 2000;

/// 同步时忽略的文件/目录名（任意层级生效）
pub const IGNORE_NAMES: &[&str] = &[
    ".DS_Store",
    "Thumbs.db",
    "desktop.ini",
    "Desktop.ini",
    super::model::MARKER_FILE,
    ".Spotlight-V100",
    ".Trashes",
    ".Trashes",
    ".fseventsd",
    "$RECYCLE.BIN",
    "System Volume Information",
];

pub fn is_ignored(name: &str) -> bool {
    IGNORE_NAMES.contains(&name)
}

pub fn index_tree(root: &Path) -> BTreeMap<String, FileStat> {
    let mut map = BTreeMap::new();
    walk(root, root, &mut map);
    map
}

fn walk(root: &Path, dir: &Path, map: &mut BTreeMap<String, FileStat>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if is_ignored(&name) {
            continue;
        }
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            walk(root, &path, map);
        } else {
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let Ok(rel) = path.strip_prefix(root) else { continue };
            let rel = rel.to_string_lossy().replace('\\', "/");
            let mtime_ms = meta.modified().ok().map(super::model::time_ms).unwrap_or(0);
            map.insert(rel, FileStat { size: meta.len(), mtime_ms });
        }
    }
}

fn same(a: FileStat, b: FileStat) -> bool {
    a.size == b.size && (a.mtime_ms - b.mtime_ms).abs() <= MTIME_TOLERANCE_MS
}

fn suggested_for(status: DiffStatus) -> SyncAction {
    match status {
        DiffStatus::NewLocal | DiffStatus::ChangedLocal => SyncAction::LocalToRemote,
        DiffStatus::NewRemote | DiffStatus::ChangedRemote => SyncAction::RemoteToLocal,
        // 冲突与删除默认不自动处理，由用户决定
        _ => SyncAction::Skip,
    }
}

/// 计算两侧差异。清单缺失或与任一侧不符的文件按冲突处理（保守策略）。
pub fn compute_diff(local_root: &Path, remote_root: &Path, manifest: &Manifest) -> Vec<DiffItem> {
    let local = index_tree(local_root);
    let remote = index_tree(remote_root);

    let mut keys: Vec<String> = local.keys().chain(remote.keys()).cloned().collect();
    keys.sort_unstable();
    keys.dedup();

    let mut items = Vec::new();
    for key in keys {
        let l = local.get(&key).copied();
        let r = remote.get(&key).copied();
        let m = manifest.entries.get(&key);

        match (l, r) {
            (Some(l), Some(r)) => {
                if same(l, r) {
                    continue; // 两侧内容一致
                }
                let local_changed = match m.and_then(|e| e.local) {
                    Some(ml) => !same(ml, l),
                    None => true,
                };
                let remote_changed = match m.and_then(|e| e.remote) {
                    Some(mr) => !same(mr, r),
                    None => true,
                };
                let status = match (local_changed, remote_changed) {
                    (true, true) => DiffStatus::BothChanged,
                    (true, false) => DiffStatus::ChangedLocal,
                    (false, true) => DiffStatus::ChangedRemote,
                    // 清单与两侧都一致但内容不同：清单状态异常，按冲突处理
                    (false, false) => DiffStatus::BothChanged,
                };
                items.push(DiffItem {
                    path: key,
                    status,
                    local: Some(l),
                    remote: Some(r),
                    suggested: suggested_for(status),
                });
            }
            (Some(l), None) => {
                // 设备侧缺失：若清单记录过设备侧，说明设备侧被删除
                let status = if m.map_or(false, |e| e.remote.is_some()) {
                    DiffStatus::DeletedRemote
                } else {
                    DiffStatus::NewLocal
                };
                items.push(DiffItem {
                    path: key,
                    status,
                    local: Some(l),
                    remote: None,
                    suggested: suggested_for(status),
                });
            }
            (None, Some(r)) => {
                let status = if m.map_or(false, |e| e.local.is_some()) {
                    DiffStatus::DeletedLocal
                } else {
                    DiffStatus::NewRemote
                };
                items.push(DiffItem {
                    path: key,
                    status,
                    local: None,
                    remote: Some(r),
                    suggested: suggested_for(status),
                });
            }
            (None, None) => unreachable!(),
        }
    }
    items
}
