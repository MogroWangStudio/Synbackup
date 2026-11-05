//! 手动同步（commands）与自动同步（monitor）共用的流程片段：
//! 阻塞执行封装、扫描前导、同步收尾。两条路径只有触发方式与
//! 失败后的呈现不同，核心步骤保持同一实现，避免日后悄然漂移。

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter};

use crate::backend::{self, manifest::Manifest, model::{ApplyResult, DiffItem, SyncPair, Trigger, VolumeInfo}};

/// spawn_blocking 统一封装：join 失败（任务 panic 等）转为 Err(String)
pub(crate) async fn run_blocking<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

/// 扫描前导各步的失败原因，调用方各自决定文案与后续动作
pub(crate) enum ScanFailure {
    /// 设备未接入（或配对目录无法定位）
    DeviceNotConnected,
    /// 本地文件夹不存在
    LocalMissing,
    /// 其余失败（带可展示的信息）
    Failed(String),
}

/// 解析任务两侧根目录：设备侧按标记 / 卷标匹配，本地侧校验存在。
/// 不写设备标记、不算差异，供执行前重扫（apply_decisions）等只定位的场景。
pub(crate) fn prepare_roots(
    volumes: &[VolumeInfo],
    pair: &SyncPair,
) -> Result<(PathBuf, PathBuf), ScanFailure> {
    let remote_root = backend::devices::resolve_remote_root(volumes, pair)
        .ok_or(ScanFailure::DeviceNotConnected)?;
    let local_root = PathBuf::from(&pair.local_path);
    if !local_root.is_dir() {
        return Err(ScanFailure::LocalMissing);
    }
    Ok((local_root, remote_root))
}

/// 扫描前导 + 差异计算：定位两侧目录 → 确保设备标记 → 载入清单 → compute_diff。
/// 设备标记失败等其余错误统一归入 [`ScanFailure::Failed`]。
pub(crate) async fn prepare_and_scan(
    dir: &Path,
    pair: &SyncPair,
    volumes: &[VolumeInfo],
) -> Result<PreparedScan, ScanFailure> {
    let (local_root, remote_root) = prepare_roots(volumes, pair)?;

    let marker_root = remote_root.clone();
    let marker_pair = pair.clone();
    run_blocking(move || backend::devices::ensure_marker(&marker_root, &marker_pair))
        .await
        .and_then(|r| r.map_err(|e| e.to_string()))
        .map_err(ScanFailure::Failed)?;

    let manifest = backend::manifest::load(dir, &pair.id);
    let items = {
        let lr = local_root.clone();
        let rr = remote_root.clone();
        let mf = manifest.clone();
        run_blocking(move || backend::scan::compute_diff(&lr, &rr, &mf))
            .await
            .map_err(ScanFailure::Failed)?
    };

    Ok(PreparedScan { local_root, remote_root, manifest, items })
}

/// 一次扫描的产出：两侧根目录、清单与差异项。
pub(crate) struct PreparedScan {
    pub local_root: PathBuf,
    pub remote_root: PathBuf,
    pub manifest: Manifest,
    pub items: Vec<DiffItem>,
}

/// 同步执行完成后的收尾：落盘清单 → 记历史 → 更新同步时间 →
/// 清掉待审条目 → 广播完成事件。手动与自动两条路径共用，仅 Trigger 不同。
pub(crate) fn finish_sync(
    app: &AppHandle,
    core: &backend::Core,
    dir: &Path,
    pair: &SyncPair,
    result: &ApplyResult,
    manifest: &Manifest,
    trigger: Trigger,
) -> Result<(), String> {
    backend::manifest::save(dir, &pair.id, manifest).map_err(|e| e.to_string())?;
    core.record_history(pair, result, trigger);
    core.touch_last_sync(&pair.id)?;
    core.remove_pending(&pair.id);
    let _ = app.emit(
        "sync-finished",
        serde_json::json!({ "pairId": pair.id, "result": result, "trigger": trigger }),
    );
    Ok(())
}
