//! 同步执行：按用户裁决逐文件执行复制/删除，并更新同步清单。

use std::path::{Path, PathBuf};

use filetime::{FileTime, set_file_mtime};

use super::devices;
use super::manifest::Manifest;
use super::model::{ApplyError, ApplyOutcome, ApplyResult, Decision, DiffItem, FileStat, SyncAction};

pub fn apply(
    local_root: &Path,
    remote_root: &Path,
    items: &[DiffItem],
    decisions: &[Decision],
    manifest: &mut Manifest,
    on_progress: &dyn Fn(u32, u32, &str),
) -> ApplyResult {
    let mut result = ApplyResult::default();
    let total = decisions.len() as u32;
    for (idx, d) in decisions.iter().enumerate() {
        on_progress(idx as u32, total, &d.path);
        let Some(_item) = items.iter().find(|i| i.path == d.path) else {
            // 该文件在重新扫描后已消失，按跳过处理
            result.skipped += 1;
            result.items.push(ApplyOutcome::failed(
                &d.path,
                SyncAction::Skip,
                "重新扫描后该文件已消失",
            ));
            continue;
        };
        let lp = devices::remote_root(&local_root.to_string_lossy(), &d.path);
        let rp = devices::remote_root(&remote_root.to_string_lossy(), &d.path);

        match d.action {
            SyncAction::LocalToRemote => match copy_over(&lp, &rp) {
                Ok(()) => {
                    result.copied_to_remote += 1;
                    refresh_entry(manifest, &d.path, &lp, &rp);
                    result.items.push(ApplyOutcome::done(&d.path, d.action));
                }
                Err(e) => {
                    result.errors.push(ApplyError::new(&d.path, &e));
                    result.items.push(ApplyOutcome::failed(&d.path, d.action, &e));
                }
            },
            SyncAction::RemoteToLocal => match copy_over(&rp, &lp) {
                Ok(()) => {
                    result.copied_to_local += 1;
                    refresh_entry(manifest, &d.path, &lp, &rp);
                    result.items.push(ApplyOutcome::done(&d.path, d.action));
                }
                Err(e) => {
                    result.errors.push(ApplyError::new(&d.path, &e));
                    result.items.push(ApplyOutcome::failed(&d.path, d.action, &e));
                }
            },
            SyncAction::DeleteRemote => match remove_path(remote_root, &d.path) {
                Ok(()) => {
                    result.deleted_remote += 1;
                    refresh_entry(manifest, &d.path, &lp, &rp);
                    result.items.push(ApplyOutcome::done(&d.path, d.action));
                }
                Err(e) => {
                    result.errors.push(ApplyError::new(&d.path, &e));
                    result.items.push(ApplyOutcome::failed(&d.path, d.action, &e));
                }
            },
            SyncAction::DeleteLocal => match remove_path(local_root, &d.path) {
                Ok(()) => {
                    result.deleted_local += 1;
                    refresh_entry(manifest, &d.path, &lp, &rp);
                    result.items.push(ApplyOutcome::done(&d.path, d.action));
                }
                Err(e) => {
                    result.errors.push(ApplyError::new(&d.path, &e));
                    result.items.push(ApplyOutcome::failed(&d.path, d.action, &e));
                }
            },
            SyncAction::Skip => {
                result.skipped += 1;
                // 跳过：保留当前实际状态，下次扫描仍会提示
                refresh_entry(manifest, &d.path, &lp, &rp);
                result.items.push(ApplyOutcome::done(&d.path, d.action));
            }
        }
    }
    on_progress(total, total, "");
    result
}

impl ApplyError {
    fn new(path: &str, message: &str) -> Self {
        Self { path: path.to_string(), message: message.to_string() }
    }
}

impl ApplyOutcome {
    fn done(path: &str, action: SyncAction) -> Self {
        Self { path: path.to_string(), action, ok: true, error: None }
    }

    fn failed(path: &str, action: SyncAction, error: &str) -> Self {
        Self { path: path.to_string(), action, ok: false, error: Some(error.to_string()) }
    }
}

fn stat_file(p: &Path) -> Option<FileStat> {
    let meta = std::fs::metadata(p).ok()?;
    if !meta.is_file() {
        return None;
    }
    Some(FileStat {
        size: meta.len(),
        mtime_ms: meta.modified().ok().map(super::model::time_ms).unwrap_or(0),
    })
}

fn refresh_entry(manifest: &mut Manifest, path: &str, lp: &Path, rp: &Path) {
    let entry = super::manifest::ManifestEntry { local: stat_file(lp), remote: stat_file(rp) };
    manifest.entries.insert(path.to_string(), entry);
}

/// 复制单个文件（目标父目录自动创建；覆盖前移除同名目录；保留修改时间）
fn copy_over(from: &Path, to: &Path) -> Result<(), String> {
    if !from.is_file() {
        return Err("源文件不存在".into());
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    if to.is_dir() {
        std::fs::remove_dir_all(to).map_err(|e| format!("移除同名目录失败: {e}"))?;
    } else if to.is_file() {
        let _ = std::fs::remove_file(to);
    }
    std::fs::copy(from, to).map_err(|e| format!("复制失败: {e}"))?;
    if let Ok(meta) = std::fs::metadata(from) {
        if let Ok(mtime) = meta.modified() {
            let _ = set_file_mtime(to, FileTime::from_system_time(mtime));
        }
    }
    Ok(())
}

/// 删除同步根目录下的相对路径（文件或目录），并清理随之变空的父目录。
/// 路径始终由 rel 拼接而来且拒绝 ".."，此处再校验一次前缀以确保安全。
fn remove_path(root: &Path, rel: &str) -> Result<(), String> {
    let p = devices::remote_root(&root.to_string_lossy(), rel);
    if !p.starts_with(root) {
        return Err("路径越界，已拒绝执行".into());
    }
    if !p.exists() && !p.is_symlink() {
        return Ok(());
    }
    if p.is_dir() {
        std::fs::remove_dir_all(&p).map_err(|e| format!("删除目录失败: {e}"))?;
    } else {
        std::fs::remove_file(&p).map_err(|e| format!("删除文件失败: {e}"))?;
    }
    cleanup_empty_parents(root, p);
    Ok(())
}

fn cleanup_empty_parents(root: &Path, mut p: PathBuf) {
    while let Some(parent) = p.parent() {
        if !parent.starts_with(root) || parent == root {
            break;
        }
        if std::fs::remove_dir(parent).is_err() {
            break; // 非空或无权限，停止向上清理
        }
        p = parent.to_path_buf();
    }
}
