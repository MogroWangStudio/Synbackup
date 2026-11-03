//! Tauri 集成层：应用组装、托盘、窗口生命周期、设备监控与通知。

mod backend;
mod commands;
mod notify;

use std::collections::HashSet;
use std::path::PathBuf;

use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewWindowBuilder};

use backend::model::{Decision, DiffStatus, PendingReview, SyncPair, Trigger, VolumeInfo, now_ms};

/// 后端文案（托盘菜单与系统通知；随设置语言，重启托盘后生效）
fn text(lang: &str, key: &str) -> &'static str {
    let zh = lang.starts_with("zh");
    match key {
        "app_title" => if zh { "同备" } else { "Synbackup" },
        "tray_open" => if zh { "打开同备" } else { "Open Synbackup" },
        "tray_scan" => if zh { "立即扫描" } else { "Scan Now" },
        "tray_quit" => if zh { "退出" } else { "Quit" },
        _ => "",
    }
}

fn text_fmt(lang: &str, key: &str, name: &str, n: usize) -> String {
    let zh = lang.starts_with("zh");
    match key {
        "diff_body" => if zh {
            format!("任务「{name}」发现 {n} 处差异，请打开同备处理。")
        } else {
            format!("{n} difference(s) found in \"{name}\". Open Synbackup to review.")
        },
        "uptodate_body" => if zh {
            format!("任务「{name}」两侧一致，无需同步。")
        } else {
            format!("\"{name}\" is already in sync.")
        },
        "local_missing" => if zh {
            format!("任务「{name}」的本地文件夹不存在。")
        } else {
            format!("Local folder for \"{name}\" does not exist.")
        },
        "sync_started_body" => if zh {
            format!("任务「{name}」开始自动同步，共 {n} 个文件。")
        } else {
            format!("Auto-sync started for \"{name}\" — {n} file(s).")
        },
        _ => String::new(),
    }
}

/// 自动同步完成通知：报告同步了多少、失败多少，而不只是"已同步"。
fn autosync_body(lang: &str, name: &str, copied: usize, failed: usize) -> String {
    let zh = lang.starts_with("zh");
    match (zh, failed) {
        (true, 0) => format!("任务「{name}」已自动同步 {copied} 个文件。"),
        (true, _) => format!("任务「{name}」已自动同步 {copied} 个文件，{failed} 个失败。"),
        (false, 0) => format!("\"{name}\" synced {copied} file(s) automatically."),
        (false, _) => format!("\"{name}\" synced {copied} file(s) automatically — {failed} failed."),
    }
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    notify::notify(app, title, body);
}

async fn run_blocking<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    } else if let Some(cfg) = app.config().app.windows.iter().find(|w| w.label == "main").cloned() {
        if let Ok(builder) = WebviewWindowBuilder::from_config(app, &cfg) {
            let _ = builder.build();
        }
    }
}

/// 当前设备快照（含"匹配到哪些任务"）。
///
/// 命令与后台推送共用同一套补全逻辑：只取裸卷列表会让前端拿到
/// `matchedPairIds: []`，于是刚插入的设备在界面上仍显示"设备未连接"。
async fn volumes_snapshot(app: &AppHandle) -> Option<Vec<VolumeInfo>> {
    let pairs = app.state::<backend::Core>().pairs();
    run_blocking(move || {
        let mut vols = backend::devices::list_volumes();
        backend::devices::attach_pairs(&mut vols, &pairs);
        vols
    })
    .await
    .ok()
}

/// 推送设备变化；内容未变则不推，避免每轮轮询都打一次 IPC。
fn emit_devices(app: &AppHandle, volumes: &[VolumeInfo], last: &mut String) {
    let payload = serde_json::to_string(volumes).unwrap_or_default();
    if payload != *last {
        let _ = app.emit("devices-changed", volumes);
        *last = payload;
    }
}

/// 扫描所有已连接且匹配的任务（托盘「立即扫描」与手动命令共用）
pub async fn check_all_devices(app: &AppHandle) {
    let Some(volumes) = volumes_snapshot(app).await else {
        eprintln!("[synbackup] 枚举设备失败");
        return;
    };
    let _ = app.emit("devices-changed", &volumes);

    for vol in &volumes {
        if let Err(e) = handle_volume_inserted(app, vol, true).await {
            eprintln!("[synbackup] 处理设备 {} 失败: {e}", vol.label);
        }
    }

    // 处理过程可能写入配对标记、改变关联关系
    if let Some(after) = volumes_snapshot(app).await {
        let _ = app.emit("devices-changed", &after);
    }
}

/// 设备插入（或应用启动时已插入）后的处理：确保标记 → 差异扫描 → 通知/自动同步
async fn handle_volume_inserted(
    app: &AppHandle,
    vol: &VolumeInfo,
    force_scan: bool,
) -> Result<(), String> {
    let (dir, settings, pairs) = {
        let core = app.state::<backend::Core>();
        let pairs: Vec<SyncPair> = core.pairs().into_iter().filter(|p| p.enabled).collect();
        (core.dir.clone(), core.settings(), pairs)
    };

    for pair in pairs {
        let Some(remote_root) = backend::devices::resolve_remote_root(std::slice::from_ref(vol), &pair)
        else {
            continue;
        };
        // 自动同步也得先扫描：只开了"无冲突自动同步"而关了"插入时自动检查"时，
        // 以前会直接 continue，自动同步（以及它的通知）永远不会发生。
        if !settings.auto_scan_on_insert && !settings.auto_sync_no_conflict && !force_scan {
            continue;
        }
        let local_root = PathBuf::from(&pair.local_path);
        if !local_root.is_dir() {
            if settings.notify_on_insert {
                notify(
                    app,
                    text(&settings.language, "app_title"),
                    &text_fmt(&settings.language, "local_missing", &pair.name, 0),
                );
            }
            continue;
        }

        // 确保设备侧目录与标记文件存在
        let root = remote_root.clone();
        let marker_pair = pair.clone();
        run_blocking(move || backend::devices::ensure_marker(&root, &marker_pair))
            .await
            .and_then(|r| r.map_err(|e| e.to_string()))?;

        let manifest = backend::manifest::load(&dir, &pair.id);
        let items = {
            let lr = local_root.clone();
            let rr = remote_root.clone();
            let mf = manifest.clone();
            run_blocking(move || backend::scan::compute_diff(&lr, &rr, &mf)).await?
        };

        let core = app.state::<backend::Core>();
        if items.is_empty() {
            core.touch_last_sync(&pair.id)?;
            continue;
        }

        let safe = items.iter().all(|i| {
            matches!(
                i.status,
                DiffStatus::NewLocal
                    | DiffStatus::NewRemote
                    | DiffStatus::ChangedLocal
                    | DiffStatus::ChangedRemote
            )
        });

        if safe && settings.auto_sync_no_conflict {
            // 无冲突：按建议方向自动执行
            if settings.notify_on_insert {
                notify(
                    app,
                    text(&settings.language, "app_title"),
                    &text_fmt(
                        &settings.language,
                        "sync_started_body",
                        &pair.name,
                        items.len(),
                    ),
                );
            }
            let decisions: Vec<Decision> =
                items.iter().map(|i| Decision { path: i.path.clone(), action: i.suggested }).collect();
            let (result, updated_manifest) = {
                let lr = local_root.clone();
                let rr = remote_root.clone();
                let items = items.clone();
                let mut manifest = manifest;
                run_blocking(move || {
                    let res = backend::execute::apply(
                        &lr, &rr, &items, &decisions, &mut manifest, &|_, _, _| {},
                    );
                    (res, manifest)
                })
                .await?
            };
            backend::manifest::save(&dir, &pair.id, &updated_manifest).map_err(|e| e.to_string())?;
            core.record_history(&pair, &result, Trigger::Auto);
            core.touch_last_sync(&pair.id)?;
            // 自动同步已经处理完这批差异，之前若留下待审条目就是过期状态
            core.remove_pending(&pair.id);
            let _ = app.emit(
                "sync-finished",
                serde_json::json!({ "pairId": pair.id, "result": result, "trigger": Trigger::Auto }),
            );
            if settings.notify_on_insert {
                let copied = (result.copied_to_remote + result.copied_to_local) as usize;
                notify(
                    app,
                    text(&settings.language, "app_title"),
                    &autosync_body(&settings.language, &pair.name, copied, result.errors.len()),
                );
            }
        } else {
            let review = PendingReview {
                pair_id: pair.id.clone(),
                pair_name: pair.name.clone(),
                device_label: pair.device_label.clone(),
                scanned_at: now_ms(),
                items,
            };
            let n = review.items.len();
            core.put_pending(review.clone());
            let _ = app.emit("review-available", &review);
            if settings.notify_on_insert {
                notify(
                    app,
                    text(&settings.language, "app_title"),
                    &text_fmt(&settings.language, "diff_body", &pair.name, n),
                );
            }
        }
    }
    Ok(())
}

/// 后台设备监控循环：轮询挂载点，检测新插入的设备。
/// 前端 WebView 是否存活与本循环完全无关。
async fn monitor_loop(app: AppHandle) {
    let mut prev_mounts: HashSet<String> = HashSet::new();
    let mut last_payload = String::new();
    loop {
        let interval = {
            let core = app.state::<backend::Core>();
            core.settings().poll_interval_secs.max(1)
        };
        tokio::time::sleep(std::time::Duration::from_secs(interval)).await;

        let Some(volumes) = volumes_snapshot(&app).await else {
            continue;
        };
        // 先推一次：设备一插上界面就能变，不必等扫描与同步跑完
        emit_devices(&app, &volumes, &mut last_payload);

        let mounts: HashSet<String> = volumes.iter().map(|v| v.mount.clone()).collect();
        let fresh: Vec<VolumeInfo> =
            volumes.iter().filter(|v| !prev_mounts.contains(&v.mount)).cloned().collect();
        prev_mounts = mounts;

        if fresh.is_empty() {
            continue;
        }
        for vol in fresh {
            if let Err(e) = handle_volume_inserted(&app, &vol, false).await {
                eprintln!("[synbackup] 处理设备 {} 失败: {e}", vol.label);
            }
        }
        // 处理过程会写入配对标记、可能改变关联关系 → 再补一次快照
        if let Some(after) = volumes_snapshot(&app).await {
            emit_devices(&app, &after, &mut last_payload);
        }
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    use tauri::image::Image;
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::TrayIconBuilder;

    let lang = app.state::<backend::Core>().settings().language;

    let open = MenuItem::with_id(app, "open", text(&lang, "tray_open"), true, None::<&str>)?;
    let scan = MenuItem::with_id(app, "scan_now", text(&lang, "tray_scan"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", text(&lang, "tray_quit"), true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &scan, &PredefinedMenuItem::separator(app)?, &quit])?;

    // macOS 菜单栏用模板图：系统只取 alpha 轮廓，并随明暗外观自动反色；
    // 其他平台用彩色应用图标。
    #[cfg(target_os = "macos")]
    let icon = Image::from_bytes(include_bytes!("../icons/tray-template.png"))?;
    #[cfg(not(target_os = "macos"))]
    let icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))?;

    let builder = TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("同备 Synbackup")
        .menu(&menu)
        .on_tray_icon_event(|tray, event| {
            // Windows 惯例：左键唤起主窗口，右键出菜单
            #[cfg(target_os = "windows")]
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = (tray, event);
            }
        });

    #[cfg(target_os = "macos")]
    let builder = builder.icon_as_template(true);

    builder.build(app)?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_settings,
            commands::upsert_pair,
            commands::delete_pair,
            commands::set_pair_enabled,
            commands::list_volumes,
            commands::scan_pair,
            commands::apply_decisions,
            commands::get_pending_reviews,
            commands::dismiss_pending_review,
            commands::get_history,
            commands::clear_history,
            commands::check_all_devices,
            commands::get_app_info,
        ])
        .setup(|app| {
            app.manage(backend::Core::new());
            let handle = app.handle().clone();
            // 通知层先行：macOS 上要尽早请求授权（系统只弹一次授权对话框）
            notify::init(&handle);
            build_tray(&handle)?;
            tauri::async_runtime::spawn(monitor_loop(handle));
            Ok(())
        })
        .on_window_event(|window, event| {
            // 关闭窗口 = 彻底销毁 WebView；后端进程保留在托盘继续运行
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.destroy();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            // 最后一个窗口销毁触发的隐式退出请求 → 拒绝退出，保持托盘常驻
            RunEvent::ExitRequested { code, api, .. } => {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
            RunEvent::MenuEvent(ev) => match ev.id.0.as_str() {
                "open" => show_main_window(app),
                "scan_now" => {
                    let handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        check_all_devices(&handle).await;
                    });
                }
                "quit" => app.exit(0),
                _ => {}
            },
            // Reopen 仅 macOS 存在（Tauri 中该变体带 cfg(target_os = "macos")），
            // 不加条件编译会导致 Windows 构建 E0599。
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => {
                // macOS 点击 Dock 图标
                show_main_window(app);
            }
            _ => {}
        });
}
