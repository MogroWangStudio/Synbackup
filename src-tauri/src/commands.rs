//! IPC 命令：前端 WebView 的唯一入口，全部是对 backend 核心的薄封装。
//! 前端退出后这些命令不再被调用，后端照常运行。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use sysinfo::{Pid, ProcessesToUpdate, System};
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

/// 本进程内存查询用的 sysinfo 实例（仅刷新自身进程，代价极小）
static MEM_SYS: OnceLock<Mutex<System>> = OnceLock::new();

/// 本应用当前物理内存（RAM）占用，字节
#[tauri::command]
pub fn get_app_memory() -> CmdResult<u64> {
    let pid = Pid::from_u32(std::process::id());
    let sys = MEM_SYS.get_or_init(|| {
        let mut s = System::new();
        s.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        Mutex::new(s)
    });
    let Ok(mut guard) = sys.lock() else {
        return Ok(0);
    };
    guard.refresh_processes(ProcessesToUpdate::Some(&[pid]), false);
    Ok(guard.process(pid).map(|p| p.memory()).unwrap_or(0))
}

/// 系统字体的语言标签（按本地化名称的语言归类）
fn font_language_tag(lang: &fontdb::Language) -> &'static str {
    let name = format!("{lang:?}");
    if name.starts_with("Chinese_PeoplesRepublicOfChina") || name.starts_with("Chinese_Singapore") {
        "zh-Hans"
    } else if name.starts_with("Chinese_Taiwan")
        || name.starts_with("Chinese_HongKong")
        || name.starts_with("Chinese_Macao")
    {
        "zh-Hant"
    } else if name == "Japanese" {
        "ja"
    } else if name == "Korean" || name == "Korean_Johab" {
        "ko"
    } else if name.starts_with("English") {
        "latin"
    } else {
        "other"
    }
}

/// 系统字体信息（按字族聚合）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontFamilyInfo {
    family: String,
    languages: Vec<String>,
    localized_names: Vec<String>,
    faces: usize,
}

/// 枚举系统已安装字体：按字族聚合，返回字族名 / 本地化名称 / 语言标签 / 面数。
/// 字体集进程内缓存（会话内不变）。
#[tauri::command]
pub fn list_system_fonts() -> CmdResult<Vec<FontFamilyInfo>> {
    use fontdb::Database;

    static FONT_DB: OnceLock<Database> = OnceLock::new();
    let db = FONT_DB.get_or_init(|| {
        let mut db = Database::new();
        db.load_system_fonts();
        db
    });

    #[derive(Default)]
    struct Agg {
        languages: HashSet<&'static str>,
        localized: HashSet<String>,
        faces: usize,
    }
    let mut aggs: HashMap<String, Agg> = HashMap::new();
    for face in db.faces() {
        // families 首项恒为英文名（缺失时为首个可用名），其余为各语言的本地化名
        let Some((primary, _)) = face.families.first() else { continue };
        let family = primary.clone();
        let agg = aggs.entry(family.clone()).or_default();
        agg.faces += 1;
        for (name, lang) in &face.families {
            agg.languages.insert(font_language_tag(lang));
            if name != &family {
                agg.localized.insert(name.clone());
            }
        }
    }
    let mut list: Vec<FontFamilyInfo> = aggs
        .into_iter()
        .map(|(family, a)| {
            let mut languages: Vec<String> = a.languages.into_iter().map(String::from).collect();
            languages.sort();
            let mut localized: Vec<String> = a.localized.into_iter().collect();
            localized.sort_by_key(|n| n.to_lowercase());
            FontFamilyInfo { family, languages, localized_names: localized, faces: a.faces }
        })
        .collect();
    list.sort_by(|a, b| a.family.to_lowercase().cmp(&b.family.to_lowercase()));
    Ok(list)
}

/// 读取自定义图片 logo：校验格式与大小（≤4MB），转 data URI 返回。
/// 存路径不存内容（config.json 不膨胀），文件缺失 / 格式不符由前端回退默认 logo。
#[tauri::command]
pub fn read_logo_image(path: String) -> CmdResult<String> {
    use base64::Engine as _;

    const MAX_LEN: u64 = 4 * 1024 * 1024;
    let p = std::path::Path::new(&path);
    let mime = match p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        _ => {
            return Err("不支持的图片格式：请使用 PNG / JPG / WebP / GIF / SVG".into());
        }
    };
    let meta = std::fs::metadata(p).map_err(|_| "图片文件不存在或无法读取".to_string())?;
    if meta.len() > MAX_LEN {
        return Err("图片过大：请使用 4 MB 以内的图片".into());
    }
    let data = std::fs::read(p).map_err(|e| e.to_string())?;
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(data)
    ))
}

#[tauri::command]
pub async fn get_app_info(app: AppHandle, core: State<'_, backend::Core>) -> CmdResult<AppInfo> {
    Ok(AppInfo {
        data_dir: core.dir.to_string_lossy().to_string(),
        version: app.package_info().version.to_string(),
    })
}

/// 切换数据目录：把现有数据复制过去并记下指针，重启后生效。
/// 返回新目录的绝对路径（界面在重启前显示"将切换到"）。
#[tauri::command]
pub async fn set_data_dir(core: State<'_, backend::Core>, path: String) -> CmdResult<String> {
    // 同步进行中不动数据：正在写的清单 / 历史会被复制成半截
    let _sync = core
        .sync_lock
        .try_lock()
        .map_err(|_| "同步任务进行中，请完成后再更改数据目录".to_string())?;
    let current = core.dir.clone();
    let target = PathBuf::from(path);
    let next = sync_flow::run_blocking(move || backend::config::switch_data_dir(&current, &target))
        .await??;
    Ok(next.to_string_lossy().to_string())
}

/// 重启应用：前端与后台一并退出，随后由系统脚本拉起新实例
#[tauri::command]
pub fn restart_app(app: AppHandle, core: State<'_, backend::Core>) -> CmdResult<()> {
    // 与安装更新同一条纪律：同步进行中不退出进程
    let _sync = core
        .sync_lock
        .try_lock()
        .map_err(|_| "同步任务进行中，请完成后再重启".to_string())?;
    crate::relaunch::spawn_relaunch_current()?;
    app.exit(0);
    Ok(())
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

/// 在系统文件管理器中打开文件夹（本地文件夹 / 设备卷内文件夹）
#[tauri::command]
pub fn open_path(app: AppHandle, path: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;

    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("路径为空".into());
    }
    if !std::path::Path::new(trimmed).is_dir() {
        return Err("文件夹不存在或已被移除".into());
    }
    app.opener()
        .open_path(trimmed, None::<&str>)
        .map_err(|e| format!("无法打开文件夹: {e}"))
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

/// 发送一条测试通知（走与自动同步相同的系统通知链路）。
/// 标题 / 内容为空时回退默认文案；macOS 与 Windows 支持通知按钮，
/// 其他平台忽略按钮。授权被拒等可在发送前判定的失败直接回给界面。
#[tauri::command]
pub fn test_notification(
    app: AppHandle,
    core: State<'_, backend::Core>,
    title: Option<String>,
    body: Option<String>,
    buttons: Option<Vec<String>>,
) -> CmdResult<()> {
    let settings = core.settings();
    let title = title
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| crate::text(&settings.language, "app_title").to_string());
    let body = body
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            crate::text_fmt(&settings.language, "test_notification_body", "", 0)
        });
    // 去空、去重、截断到 3 个（macOS 横幅上按钮过多也不可读）
    let mut buttons: Vec<String> = buttons
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    buttons.dedup();
    buttons.truncate(3);
    crate::notify::test_notify(&app, &title, &body, &buttons)
}
