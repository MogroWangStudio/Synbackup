//! Tauri 集成层：应用组装、托盘、窗口生命周期与后端文案。
//! 设备监控（事件驱动 + 兜底轮询）见 `monitor`，系统通知见 `notify`。

mod backend;
mod commands;
mod monitor;
mod notify;
mod sync_flow;
mod update;

use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewWindowBuilder};

/// 后端支持的语言集合（与前端 src/i18n/ 保持一致）。
#[derive(Clone, Copy)]
enum Lang {
    ZhCn,
    ZhTw,
    En,
    Ja,
    Ko,
    Fr,
    De,
    Es,
    Pt,
    It,
    Ru,
}

/// 设置里的语言字符串 → Lang；未知值回退源语言（简体中文）。
fn lang_of(lang: &str) -> Lang {
    match lang {
        "zh-TW" => Lang::ZhTw,
        "en-US" => Lang::En,
        "ja" => Lang::Ja,
        "ko" => Lang::Ko,
        "fr" => Lang::Fr,
        "de" => Lang::De,
        "es" => Lang::Es,
        "pt" => Lang::Pt,
        "it" => Lang::It,
        "ru" => Lang::Ru,
        _ => Lang::ZhCn, // 含 zh-CN 与历史遗留的未知值
    }
}

/// 后端文案（托盘菜单与系统通知；随设置语言，重启托盘后生效）
pub(crate) fn text(lang: &str, key: &str) -> &'static str {
    let l = lang_of(lang);
    match key {
        "app_title" => match l {
            Lang::ZhCn => "同备",
            Lang::ZhTw => "同備",
            _ => "Synbackup",
        },
        "tray_open" => match l {
            Lang::ZhCn => "打开同备",
            Lang::ZhTw => "開啟同備",
            Lang::En => "Open Synbackup",
            Lang::Ja => "Synbackup を開く",
            Lang::Ko => "Synbackup 열기",
            Lang::Fr => "Ouvrir Synbackup",
            Lang::De => "Synbackup öffnen",
            Lang::Es => "Abrir Synbackup",
            Lang::Pt => "Abrir o Synbackup",
            Lang::It => "Apri Synbackup",
            Lang::Ru => "Открыть Synbackup",
        },
        "tray_scan" => match l {
            Lang::ZhCn => "立即扫描",
            Lang::ZhTw => "立即掃描",
            Lang::En => "Scan Now",
            Lang::Ja => "今すぐスキャン",
            Lang::Ko => "지금 스캔",
            Lang::Fr => "Analyser maintenant",
            Lang::De => "Jetzt scannen",
            Lang::Es => "Escanear ahora",
            Lang::Pt => "Escanear agora",
            Lang::It => "Scansiona ora",
            Lang::Ru => "Сканировать сейчас",
        },
        "tray_quit" => match l {
            Lang::ZhCn => "退出",
            Lang::ZhTw => "結束",
            Lang::En => "Quit",
            Lang::Ja => "終了",
            Lang::Ko => "종료",
            Lang::Fr => "Quitter",
            Lang::De => "Beenden",
            Lang::Es => "Salir",
            Lang::Pt => "Sair",
            Lang::It => "Esci",
            Lang::Ru => "Выход",
        },
        _ => "",
    }
}

pub(crate) fn text_fmt(lang: &str, key: &str, name: &str, n: usize) -> String {
    let l = lang_of(lang);
    match key {
        "diff_body" => match l {
            Lang::ZhCn => format!("任务「{name}」发现 {n} 处差异，请打开同备处理。"),
            Lang::ZhTw => format!("任務「{name}」發現 {n} 處差異，請開啟同備處理。"),
            Lang::En => format!("{n} difference(s) found in \"{name}\". Open Synbackup to review."),
            Lang::Ja => format!("「{name}」で {n} 件の差分を検出しました。Synbackup を開いて確認してください。"),
            Lang::Ko => format!("“{name}” 작업에서 차이 {n}곳을 발견했습니다. Synbackup에서 확인하세요."),
            Lang::Fr => format!("{n} différence(s) détectée(s) dans « {name} ». Ouvrez Synbackup pour les examiner."),
            Lang::De => format!("{n} Unterschied(e) in „{name}“ gefunden. Öffnen Sie Synbackup zur Überprüfung."),
            Lang::Es => format!("{n} diferencia(s) detectada(s) en «{name}». Abra Synbackup para revisarlas."),
            Lang::Pt => format!("{n} diferença(s) encontrada(s) em “{name}”. Abra o Synbackup para revisar."),
            Lang::It => format!("{n} differenza/e rilevata/e in «{name}». Apri Synbackup per verificarle."),
            Lang::Ru => format!("В задаче «{name}» обнаружено различий: {n}. Откройте Synbackup для разбора."),
        },
        "local_missing" => match l {
            Lang::ZhCn => format!("任务「{name}」的本地文件夹不存在。"),
            Lang::ZhTw => format!("任務「{name}」的本機資料夾不存在。"),
            Lang::En => format!("Local folder for \"{name}\" does not exist."),
            Lang::Ja => format!("「{name}」のローカルフォルダが見つかりません。"),
            Lang::Ko => format!("“{name}” 작업의 로컬 폴더가 없습니다."),
            Lang::Fr => format!("Le dossier local de « {name} » n'existe pas."),
            Lang::De => format!("Der lokale Ordner von „{name}“ existiert nicht."),
            Lang::Es => format!("La carpeta local de «{name}» no existe."),
            Lang::Pt => format!("A pasta local de “{name}” não existe."),
            Lang::It => format!("La cartella locale di «{name}» non esiste."),
            Lang::Ru => format!("Локальная папка задачи «{name}» не существует."),
        },
        "sync_started_body" => match l {
            Lang::ZhCn => format!("任务「{name}」开始自动同步，共 {n} 个文件。"),
            Lang::ZhTw => format!("任務「{name}」開始自動同步，共 {n} 個檔案。"),
            Lang::En => format!("Auto-sync started for \"{name}\" — {n} file(s)."),
            Lang::Ja => format!("「{name}」の自動同期を開始しました（{n} 個のファイル）。"),
            Lang::Ko => format!("“{name}” 작업의 자동 동기화를 시작합니다(파일 {n}개)."),
            Lang::Fr => format!("Synchronisation auto de « {name} » démarrée — {n} fichier(s)."),
            Lang::De => format!("Auto-Sync für „{name}“ gestartet — {n} Datei(en)."),
            Lang::Es => format!("Sync automática de «{name}» iniciada — {n} archivo(s)."),
            Lang::Pt => format!("Sync automática de “{name}” iniciada — {n} arquivo(s)."),
            Lang::It => format!("Sync automatica di «{name}» avviata — {n} file."),
            Lang::Ru => format!("Автосинхронизация «{name}» началась — файлов: {n}."),
        },
        "test_notification_body" => match l {
            Lang::ZhCn => "这是一条测试通知，通知链路工作正常。".to_string(),
            Lang::ZhTw => "這是一條測試通知，通知鏈路運作正常。".to_string(),
            Lang::En => "This is a test notification — the pipeline is working.".to_string(),
            Lang::Ja => "これはテスト通知です。通知の仕組みは正常に動作しています。".to_string(),
            Lang::Ko => "테스트 알림입니다. 알림 경로가 정상적으로 작동합니다.".to_string(),
            Lang::Fr => "Ceci est une notification de test — la chaîne de notification fonctionne.".to_string(),
            Lang::De => "Dies ist eine Testmitteilung — die Meldungskette funktioniert.".to_string(),
            Lang::Es => "Esta es una notificación de prueba: el sistema de avisos funciona.".to_string(),
            Lang::Pt => "Esta é uma notificação de teste — o sistema de avisos está funcionando.".to_string(),
            Lang::It => "Questa è una notifica di prova: il sistema di notifiche funziona.".to_string(),
            Lang::Ru => "Это тестовое уведомление — канал уведомлений работает.".to_string(),
        },
        _ => String::new(),
    }
}

/// 自动同步完成通知：报告同步了多少、失败多少，而不只是"已同步"。
pub(crate) fn autosync_body(lang: &str, name: &str, copied: usize, failed: usize) -> String {
    let l = lang_of(lang);
    if failed == 0 {
        match l {
            Lang::ZhCn => format!("任务「{name}」已自动同步 {copied} 个文件。"),
            Lang::ZhTw => format!("任務「{name}」已自動同步 {copied} 個檔案。"),
            Lang::En => format!("\"{name}\" synced {copied} file(s) automatically."),
            Lang::Ja => format!("「{name}」を自動同期しました（{copied} 個のファイル）。"),
            Lang::Ko => format!("“{name}” 작업이 파일 {copied}개를 자동 동기화했습니다."),
            Lang::Fr => format!("« {name} » a synchronisé {copied} fichier(s) automatiquement."),
            Lang::De => format!("„{name}“ hat {copied} Datei(en) automatisch synchronisiert."),
            Lang::Es => format!("«{name}» sincronizó {copied} archivo(s) automáticamente."),
            Lang::Pt => format!("“{name}” sincronizou {copied} arquivo(s) automaticamente."),
            Lang::It => format!("«{name}» ha sincronizzato {copied} file automaticamente."),
            Lang::Ru => format!("Задача «{name}» автоматически синхронизировала файлов: {copied}."),
        }
    } else {
        match l {
            Lang::ZhCn => format!("任务「{name}」已自动同步 {copied} 个文件，{failed} 个失败。"),
            Lang::ZhTw => format!("任務「{name}」已自動同步 {copied} 個檔案，{failed} 個失敗。"),
            Lang::En => format!("\"{name}\" synced {copied} file(s) automatically — {failed} failed."),
            Lang::Ja => format!("「{name}」を自動同期しました（{copied} 個のファイル、{failed} 件失敗）。"),
            Lang::Ko => format!("“{name}” 작업이 파일 {copied}개를 자동 동기화했습니다({failed}개 실패)."),
            Lang::Fr => format!("« {name} » a synchronisé {copied} fichier(s) automatiquement — {failed} échec(s)."),
            Lang::De => format!("„{name}“ hat {copied} Datei(en) automatisch synchronisiert — {failed} fehlgeschlagen."),
            Lang::Es => format!("«{name}» sincronizó {copied} archivo(s) automáticamente — {failed} con error."),
            Lang::Pt => format!("“{name}” sincronizou {copied} arquivo(s) automaticamente — {failed} com falha."),
            Lang::It => format!("«{name}» ha sincronizzato {copied} file automaticamente — {failed} falliti."),
            Lang::Ru => format!("Задача «{name}» автоматически синхронизировала файлов: {copied}, с ошибками: {failed}."),
        }
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
    // 窗口可见时机：前端以此事件重播启动动画。挂载即播的动画在真实窗口
    // 首次绘制前就已放完；托盘重开的已挂载窗口更不会自己重播
    let _ = app.emit("window-shown", ());
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
            commands::get_app_info,
            commands::open_external,
            commands::mark_ui_ready,
            commands::exit_app,
            commands::test_notification,
            commands::destroy_main_window,
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
