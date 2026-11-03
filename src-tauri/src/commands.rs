//! IPC 命令：前端 WebView 的唯一入口，全部是对 backend 核心的薄封装。
//! 前端退出后这些命令不再被调用，后端照常运行。

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::backend::{self, model::*};

type CmdResult<T> = Result<T, String>;

async fn blocking<T, F>(f: F) -> CmdResult<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub data_dir: String,
    pub version: String,
}

#[tauri::command]
pub async fn get_config(core: State<'_, backend::Core>) -> CmdResult<Config> {
    Ok(core.config_snapshot())
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    core: State<'_, backend::Core>,
    settings: AppSettings,
) -> CmdResult<()> {
    let old = core.settings();
    core.update_settings(settings.clone())?;
    if old.launch_at_login != settings.launch_at_login {
        use tauri_plugin_autostart::ManagerExt;
        let mgr = app.autolaunch();
        if settings.launch_at_login {
            mgr.enable().map_err(|e| e.to_string())?;
        } else {
            mgr.disable().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn upsert_pair(core: State<'_, backend::Core>, pair: SyncPair) -> CmdResult<()> {
    let mut pair = pair;
    // 目标设备当前已连接时，读取或生成设备标记
    if pair.device_marker.is_none() {
        let vols = blocking(backend::devices::list_volumes).await?;
        if let Some(vol) = vols.iter().find(|v| v.label == pair.device_label) {
            let root = backend::devices::remote_root(&vol.mount, &backend::devices::normalize_rel(&pair.remote_path));
            let marker_pair = pair.clone();
            let marker_root = root.clone();
            let _ = blocking(move || backend::devices::ensure_marker(&marker_root, &marker_pair)).await;
            pair.device_marker = backend::devices::read_marker(&root);
        }
    }
    core.upsert_pair(pair)
}

#[tauri::command]
pub async fn delete_pair(core: State<'_, backend::Core>, id: String) -> CmdResult<()> {
    core.delete_pair(&id)
}

#[tauri::command]
pub async fn set_pair_enabled(
    core: State<'_, backend::Core>,
    id: String,
    enabled: bool,
) -> CmdResult<()> {
    core.set_pair_enabled(&id, enabled)
}

#[tauri::command]
pub async fn list_volumes(core: State<'_, backend::Core>) -> CmdResult<Vec<VolumeInfo>> {
    let pairs = core.pairs();
    Ok(blocking(move || {
        let mut vols = backend::devices::list_volumes();
        backend::devices::attach_pairs(&mut vols, &pairs);
        vols
    })
    .await?)
}

/// 手动扫描单个任务，返回差异报告（同时存入待处理列表）
#[tauri::command]
pub async fn scan_pair(
    core: State<'_, backend::Core>,
    pair_id: String,
) -> CmdResult<PendingReview> {
    let pair = core.pair_by_id(&pair_id).ok_or("任务不存在")?;
    let dir = core.dir.clone();

    let vols = blocking(backend::devices::list_volumes).await?;
    let remote_root =
        backend::devices::resolve_remote_root(&vols, &pair).ok_or("设备未连接，请插入后重试")?;
    let local_root = PathBuf::from(&pair.local_path);
    if !local_root.is_dir() {
        return Err("本地文件夹不存在".into());
    }

    let marker_pair = pair.clone();
    let marker_root = remote_root.clone();
    blocking(move || backend::devices::ensure_marker(&marker_root, &marker_pair))
        .await?
        .map_err(|e| e.to_string())?;

    let manifest = backend::manifest::load(&dir, &pair.id);
    let items = {
        let lr = local_root;
        let rr = remote_root;
        blocking(move || backend::scan::compute_diff(&lr, &rr, &manifest)).await?
    };

    let review = PendingReview {
        pair_id: pair.id.clone(),
        pair_name: pair.name.clone(),
        device_label: pair.device_label.clone(),
        scanned_at: now_ms(),
        items,
    };
    core.put_pending(review.clone());
    Ok(review)
}

/// 按用户裁决执行同步。执行前重新扫描，确保清单与当前状态一致。
#[tauri::command]
pub async fn apply_decisions(
    app: AppHandle,
    core: State<'_, backend::Core>,
    pair_id: String,
    decisions: Vec<Decision>,
) -> CmdResult<ApplyResult> {
    let _guard = core.sync_lock.lock().await;
    let pair = core.pair_by_id(&pair_id).ok_or("任务不存在")?;
    let dir = core.dir.clone();

    let vols = blocking(backend::devices::list_volumes).await?;
    let remote_root =
        backend::devices::resolve_remote_root(&vols, &pair).ok_or("设备未连接，同步已中止")?;
    let local_root = PathBuf::from(&pair.local_path);
    if !local_root.is_dir() {
        return Err("本地文件夹不存在，同步已中止".into());
    }

    let manifest = backend::manifest::load(&dir, &pair.id);
    let (result, updated_manifest) = {
        let lr = local_root;
        let rr = remote_root;
        let handle = app.clone();
        let pid = pair.id.clone();
        blocking(move || {
            let items = backend::scan::compute_diff(&lr, &rr, &manifest);
            let mut manifest = manifest;
            let on_progress = |done: u32, total: u32, current: &str| {
                let _ = handle.emit(
                    "sync-progress",
                    serde_json::json!({ "pairId": pid, "done": done, "total": total, "current": current }),
                );
            };
            let res = backend::execute::apply(&lr, &rr, &items, &decisions, &mut manifest, &on_progress);
            (res, manifest)
        })
        .await?
    };

    backend::manifest::save(&dir, &pair.id, &updated_manifest).map_err(|e| e.to_string())?;
    core.record_history(&pair, &result, Trigger::Manual);
    core.touch_last_sync(&pair_id)?;
    core.remove_pending(&pair_id);
    let _ = app.emit(
        "sync-finished",
        serde_json::json!({ "pairId": pair_id, "result": result, "trigger": Trigger::Manual }),
    );
    Ok(result)
}

#[tauri::command]
pub async fn get_pending_reviews(core: State<'_, backend::Core>) -> CmdResult<Vec<PendingReview>> {
    Ok(core.pending_snapshot())
}

#[tauri::command]
pub async fn dismiss_pending_review(core: State<'_, backend::Core>, pair_id: String) -> CmdResult<()> {
    core.remove_pending(&pair_id);
    Ok(())
}

#[tauri::command]
pub async fn get_history(core: State<'_, backend::Core>, limit: Option<u32>) -> CmdResult<Vec<HistoryEntry>> {
    Ok(backend::history::read(&core.dir, limit.unwrap_or(100) as usize))
}

/// 累计同步次数（历史总条数，供侧栏状态区展示）
#[tauri::command]
pub async fn get_sync_count(core: State<'_, backend::Core>) -> CmdResult<u64> {
    Ok(backend::history::count(&core.dir))
}

#[tauri::command]
pub async fn clear_history(core: State<'_, backend::Core>) -> CmdResult<()> {
    backend::history::clear(&core.dir);
    Ok(())
}

#[tauri::command]
pub async fn check_all_devices(app: AppHandle) -> CmdResult<usize> {
    crate::monitor::check_all_devices(&app).await;
    let core = app.state::<backend::Core>();
    Ok(core.pending_snapshot().len())
}

#[tauri::command]
pub async fn get_app_info(app: AppHandle, core: State<'_, backend::Core>) -> CmdResult<AppInfo> {
    Ok(AppInfo {
        data_dir: core.dir.to_string_lossy().to_string(),
        version: app.package_info().version.to_string(),
    })
}
