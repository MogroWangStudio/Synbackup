//! Tauri 集成层：应用组装、托盘、窗口生命周期与后端文案。
//! 设备监控（事件驱动 + 兜底轮询）见 `monitor`，系统通知见 `notify`。

mod backend;
mod commands;
mod monitor;
mod notify;
mod update;

use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewWindowBuilder};

/// 后端文案（托盘菜单与系统通知；随设置语言，重启托盘后生效）
pub(crate) fn text(lang: &str, key: &str) -> &'static str {
    let zh = lang.starts_with("zh");
    match key {
        "app_title" => if zh { "同备" } else { "Synbackup" },
        "tray_open" => if zh { "打开同备" } else { "Open Synbackup" },
        "tray_scan" => if zh { "立即扫描" } else { "Scan Now" },
        "tray_quit" => if zh { "退出" } else { "Quit" },
        _ => "",
    }
}

pub(crate) fn text_fmt(lang: &str, key: &str, name: &str, n: usize) -> String {
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
pub(crate) fn autosync_body(lang: &str, name: &str, copied: usize, failed: usize) -> String {
    let zh = lang.starts_with("zh");
    match (zh, failed) {
        (true, 0) => format!("任务「{name}」已自动同步 {copied} 个文件。"),
        (true, _) => format!("任务「{name}」已自动同步 {copied} 个文件，{failed} 个失败。"),
        (false, 0) => format!("\"{name}\" synced {copied} file(s) automatically."),
        (false, _) => format!("\"{name}\" synced {copied} file(s) automatically — {failed} failed."),
    }
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
        .plugin(tauri_plugin_opener::init())
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
            commands::get_sync_count,
            commands::clear_history,
            commands::check_all_devices,
            commands::get_app_info,
            commands::open_external,
            commands::mark_ui_ready,
            commands::exit_app,
            update::check_update,
        ])
        .setup(|app| {
            app.manage(backend::Core::new());
            let handle = app.handle().clone();
            // 通知层先行：macOS 上要尽早请求授权（系统只弹一次授权对话框）
            notify::init(&handle);
            build_tray(&handle)?;
            // 设备监控：系统事件驱动，主循环里做去抖与快照对比
            monitor::spawn(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // 关闭窗口：前端就绪时交给前端询问退出方式（仅退窗 / 完全退出）；
            // 前端尚未就绪（如启动瞬间）则直接销毁窗口，后端保留在托盘
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let core = window.app_handle().state::<backend::Core>();
                if !core.ui_ready() {
                    let _ = window.destroy();
                    return;
                }
                let _ = window.app_handle().emit("close-requested", ());
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
                        monitor::check_all_devices(&handle).await;
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
