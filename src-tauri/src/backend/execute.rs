//! 同步执行：按用户裁决逐文件执行复制/删除，并更新同步清单。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use filetime::{FileTime, set_file_mtime};

use super::devices;
use super::manifest::Manifest;
use super::model::{ApplyError, ApplyOutcome, ApplyResult, Decision, DiffItem, FileStat, SyncAction};

/// 进度回调的最小间隔：万级文件逐个 emit 会把 IPC 通道打满，
/// 界面上的进度条反而卡住（前端每次都要重排整条提示）
const PROGRESS_INTERVAL: Duration = Duration::from_millis(60);

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
    // 每次裁决都要回查"该文件是否还在重扫结果里"，线性查找在千级文件上
    // 是 O(n²) 的字符串比较，换成集合后增删都只看一次
    let present: HashSet<&str> = items.iter().map(|i| i.path.as_str()).collect();
    let mut last_emit: Option<Instant> = None;
    for (idx, d) in decisions.iter().enumerate() {
        if last_emit.map_or(true, |t| t.elapsed() >= PROGRESS_INTERVAL) {
            on_progress(idx as u32, total, &d.path);
            last_emit = Some(Instant::now());
        }
        if !present.contains(d.path.as_str()) {
            // 该文件在重新扫描后已消失，按跳过处理
            result.skipped += 1;
            result.items.push(ApplyOutcome::failed(
                &d.path,
                SyncAction::Skip,
                "重新扫描后该文件已消失",
            ));
            continue;
        }
        let lp = devices::remote_root(&local_root.to_string_lossy(), &d.path);
        let rp = devices::remote_root(&remote_root.to_string_lossy(), &d.path);

        match run_step(d.action, local_root, remote_root, &d.path, &lp, &rp) {
            Ok((size, synced)) => {
                match d.action {
                    SyncAction::LocalToRemote => result.copied_to_remote += 1,
                    SyncAction::RemoteToLocal => result.copied_to_local += 1,
                    SyncAction::DeleteRemote => result.deleted_remote += 1,
                    SyncAction::DeleteLocal => result.deleted_local += 1,
                    SyncAction::Skip => result.skipped += 1,
                }
                result.total_bytes += size.unwrap_or(0);
                // 跳过不代表两侧状态变了：刷新清单会把"设备侧已删除"记成
                // "设备侧从来没有过"，下次扫描就退化成新文件，自动同步会把
                // 用户特意留着不删的文件又拷回去
                if synced {
                    refresh_entry(manifest, &d.path, &lp, &rp);
                }
                result.items.push(ApplyOutcome::done(&d.path, d.action, size));
            }
            Err(e) => {
                result.errors.push(ApplyError::new(&d.path, &e));
                result.items.push(ApplyOutcome::failed(&d.path, d.action, &e));
            }
        }
    }
    on_progress(total, total, "");
    result
}

/// 执行单条决策对应的操作。返回 `(涉及文件的字节数, 是否需要刷新清单)`：
/// 字节数供本次统计，删除类动作在删除前计量（删完再 stat 只剩 0）；
/// 清单只在两侧状态真的被改写时才刷新。
fn run_step(
    action: SyncAction,
    local_root: &Path,
    remote_root: &Path,
    rel: &str,
    lp: &Path,
    rp: &Path,
) -> Result<(Option<u64>, bool), String> {
    match action {
        SyncAction::LocalToRemote => {
            copy_over(lp, rp)?;
            Ok((file_size(lp), true))
        }
        SyncAction::RemoteToLocal => {
            copy_over(rp, lp)?;
            Ok((file_size(rp), true))
        }
        SyncAction::DeleteRemote => {
            let size = file_size(rp);
            remove_path(remote_root, rel)?;
            Ok((size, true))
        }
        SyncAction::DeleteLocal => {
            let size = file_size(lp);
            remove_path(local_root, rel)?;
            Ok((size, true))
        }
        // 跳过：保留当前实际状态，清单不动，下次扫描仍会提示
        SyncAction::Skip => Ok((None, false)),
    }
}

impl ApplyError {
    fn new(path: &str, message: &str) -> Self {
        Self { path: path.to_string(), message: message.to_string() }
    }
}

impl ApplyOutcome {
    fn done(path: &str, action: SyncAction, size: Option<u64>) -> Self {
        Self { path: path.to_string(), action, ok: true, error: None, size }
    }

    fn failed(path: &str, action: SyncAction, error: &str) -> Self {
        Self { path: path.to_string(), action, ok: false, error: Some(error.to_string()), size: None }
    }
}

/// 文件大小（目录或不存在返回 None；目录的递归体量不在此统计）
fn file_size(p: &Path) -> Option<u64> {
    std::fs::metadata(p).ok().filter(|m| m.is_file()).map(|m| m.len())
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
