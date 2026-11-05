//! IPC 命令：前端 WebView 的唯一入口，全部是对 backend 核心的薄封装。
//! 前端退出后这些命令不再被调用，后端照常运行。

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::backend::{self, model::*};
use crate::sync_flow::{self, ScanFailure};

type CmdResult<T> = Result<T, String>;

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
        let vols = sync_flow::run_blocking(backend::devices::list_volumes).await?;
        if let Some(vol) = vols.iter().find(|v| v.label == pair.device_label) {
            let root = backend::devices::remote_root(&vol.mount, &backend::devices::normalize_rel(&pair.remote_path));
            let marker_pair = pair.clone();
            let marker_root = root.clone();
            let _ = sync_flow::run_blocking(move || backend::devices::ensure_marker(&marker_root, &marker_pair)).await;
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
    sync_flow::run_blocking(move || backend::devices::snapshot_with_pairs(&pairs)).await
}

/// 手动扫描单个任务，返回差异报告（同时存入待处理列表）
#[tauri::command]
pub async fn scan_pair(
    core: State<'_, backend::Core>,
    pair_id: String,
) -> CmdResult<PendingReview> {
    let pair = core.pair_by_id(&pair_id).ok_or("任务不存在")?;

    let vols = sync_flow::run_blocking(backend::devices::list_volumes).await?;
    let scan = sync_flow::prepare_and_scan(&core.dir, &pair, &vols)
        .await
        .map_err(|f| match f {
            ScanFailure::DeviceNotConnected => "设备未连接，请插入后重试".into(),
            ScanFailure::LocalMissing => "本地文件夹不存在".into(),
            ScanFailure::Failed(e) => e,
        })?;

    let review = PendingReview::new(&pair, scan.items);
    core.put_pending(review.clone());
    // 手动扫描到差异：写入"检查到差异"历史记录
    core.record_scan_history(&pair, &review.items, Trigger::Manual);
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

    let vols = sync_flow::run_blocking(backend::devices::list_volumes).await?;
    let (local_root, remote_root) = sync_flow::prepare_roots(&vols, &pair).map_err(|f| match f {
        ScanFailure::DeviceNotConnected => "设备未连接，同步已中止".into(),
        ScanFailure::LocalMissing => "本地文件夹不存在，同步已中止".into(),
        ScanFailure::Failed(e) => e,
    })?;

    let manifest = backend::manifest::load(&core.dir, &pair.id);
    let (result, updated_manifest) = {
        let lr = local_root;
        let rr = remote_root;
        let handle = app.clone();
        let pid = pair.id.clone();
        sync_flow::run_blocking(move || {
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

    sync_flow::finish_sync(&app, &core, &core.dir, &pair, &result, &updated_manifest, Trigger::Manual)?;
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
pub async fn get_app_info(app: AppHandle, core: State<'_, backend::Core>) -> CmdResult<AppInfo> {
    Ok(AppInfo {
        data_dir: core.dir.to_string_lossy().to_string(),
        version: app.package_info().version.to_string(),
    })
}

/// 用系统默认浏览器打开外部链接（仅 http/https，防任意协议唤起）
#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("仅支持 http/https 链接".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// 前端初始化完成：此后关闭窗口改为询问退出方式
#[tauri::command]
pub fn mark_ui_ready(core: State<'_, backend::Core>) -> CmdResult<()> {
    core.set_ui_ready();
    Ok(())
}

/// 完全退出（含后台托盘进程）
#[tauri::command]
pub fn exit_app(app: AppHandle) -> CmdResult<()> {
    app.exit(0);
    Ok(())
}

/// 销毁主窗口（后端保留在托盘运行）。"仅退出窗口"的兜底通道：
/// 前端 destroy IPC 受 capability 管控，权限缺失时由此命令完成同一动作。
#[tauri::command]
pub fn destroy_main_window(app: AppHandle) -> CmdResult<()> {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.destroy();
    }
    Ok(())
}

/// 发送一条测试通知（走与自动同步相同的系统通知链路）
#[tauri::command]
pub fn test_notification(app: AppHandle, core: State<'_, backend::Core>) -> CmdResult<()> {
    let settings = core.settings();
    crate::notify::notify(
        &app,
        crate::text(&settings.language, "app_title"),
        crate::text(&settings.language, "test_notification_body"),
    );
    Ok(())
}
