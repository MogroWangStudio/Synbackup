//! 系统通知层：统一从 Rust 侧发送，与窗口（WebView）是否存活无关，
//! 前端关闭、仅托盘常驻时通知照常弹出。
//!
//! 平台差异（这也是 0.3.0 之前通知不弹的根因）：
//! - macOS：改用现代 `UNUserNotificationCenter`。旧路径（NSUserNotification）
//!   在新版 macOS 上已不可靠，且从不主动请求授权——授权未决时系统静默丢弃
//!   通知。这里在启动时请求一次授权，并设置 delegate 让应用处于前台时
//!   也弹出横幅（默认前台只进通知中心、不显示横幅）。
//! - Windows：WinRT toast 要求 AUMID 能解析到一个已注册的来源。NSIS 安装版
//!   会写 `HKCU\Software\Classes\AppUserModelId\<identifier>`，此时用应用
//!   自身身份发送（显示应用名与图标）；便携版 / 开发环境没有这个注册表键，
//!   tauri-plugin-notification 会把 AUMID 硬设成未注册的 identifier，toast
//!   被 Windows 静默丢弃——这里改为不设置 AUMID，notify-rust 自动落到系统
//!   PowerShell 的 AUMID，保证通知始终能弹出来。
//! - 其他桌面平台：notify-rust（Desktop Notifications 规范）。

use std::sync::OnceLock;

use tauri::{AppHandle, Emitter};

use crate::text;

/// 应用 identifier（init 时从 Tauri 配置读取，供 Windows AUMID 使用）
static IDENTIFIER: OnceLock<String> = OnceLock::new();

/// AppHandle（delegate 点击回调里向前端发事件用；init 在主线程最早调用）
static APP: OnceLock<AppHandle> = OnceLock::new();

pub fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
    let _ = IDENTIFIER.set(app.config().identifier.clone());
    #[cfg(target_os = "windows")]
    register_aumid_windows();
    #[cfg(target_os = "macos")]
    imp::init();
}

/// 发送通知。统一分发到主线程执行（macOS 的 UNUserNotificationCenter 相关
/// 对象只在主线程触碰，Windows / Linux 无此要求但统一走也无害）。
pub fn notify(app: &AppHandle, title: &str, body: &str) {
    let (title, body) = (title.to_owned(), body.to_owned());
    let send = move || {
        if let Err(e) = imp::send(&title, &body) {
            // 发送失败不能静默：系统里关掉了本应用的通知、或授权被拒时，
            // 只有日志能说明"为什么没有弹出通知"。
            eprintln!("[synbackup] 发送系统通知失败: {e}");
        }
    };
    if let Err(e) = app.run_on_main_thread(send) {
        eprintln!("[synbackup] 无法派发通知到主线程: {e}");
    }
}

/// 同步前询问通知：固定两个动作（开始同步 / 取消任务），请求 ID 携带
/// pairId 供点击响应回溯。macOS：注册 sync-review 类别 + 持久显示；
/// 其他平台忽略按钮只发文本（用户进应用处理）。
pub fn notify_ask(app: &AppHandle, lang: &str, title: &str, body: &str, pair_id: &str) {
    let actions = [
        text(lang, "notify_action_sync").to_string(),
        text(lang, "notify_action_cancel").to_string(),
    ];
    let (title, body, pair_id) = (title.to_owned(), body.to_owned(), pair_id.to_owned());
    let send = move || {
        if let Err(e) = imp::send_ask(&title, &body, &actions, &pair_id) {
            eprintln!("[synbackup] 发送询问通知失败: {e}");
        }
    };
    if let Err(e) = app.run_on_main_thread(send) {
        eprintln!("[synbackup] 无法派发通知到主线程: {e}");
    }
}

/// 测试通知：标题 / 内容 / 按钮全部由界面传入。按钮（通知动作）仅 macOS
/// 生效——Win32 toast 的按钮需要 COM 激活链路，notify-rust 不支持，其他
/// 平台忽略按钮只发标题与内容（界面侧已在非 macOS 隐藏按钮编辑）。
pub fn test_notify(app: &AppHandle, title: &str, body: &str, buttons: &[String]) {
    let (title, body, buttons) = (title.to_owned(), body.to_owned(), buttons.to_vec());
    let send = move || {
        if let Err(e) = imp::send_test(&title, &body, &buttons) {
            eprintln!("[synbackup] 发送测试通知失败: {e}");
        }
    };
    if let Err(e) = app.run_on_main_thread(send) {
        eprintln!("[synbackup] 无法派发通知到主线程: {e}");
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::Emitter;
    use tauri::Manager;
    use block2::{DynBlock, RcBlock};
    use objc2::rc::Retained;
    use objc2::runtime::{Bool, ProtocolObject};
    use objc2::{msg_send, MainThreadMarker, MainThreadOnly};
    use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol, NSSet, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
        UNNotificationAction, UNNotificationActionOptionNone, UNNotificationCategory,
        UNNotificationCategoryOptionNone, UNNotificationDefaultActionIdentifier,
        UNNotificationDismissActionIdentifier, UNNotificationPresentationOptions,
        UNNotificationRequest, UNNotificationResponse, UNNotificationSound,
        UNUserNotificationCenter, UNUserNotificationCenterDelegate,
    };

    /// 测试通知类别：按钮按发送时的文字即时重建（类别集整体替换，幂等）
    const TEST_CATEGORY: &str = "test-actions";
    /// 同步前询问类别：固定两个动作（开始同步 / 取消任务）
    const ASK_CATEGORY: &str = "sync-review";

    objc2::define_class!(
        /// 前台横幅 delegate：应用在前台时默认只把通知放进通知中心，
        /// 不弹横幅——自动同步的提醒恰恰是在窗口开着时发生的，必须补上。
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        struct BannerDelegate;

        unsafe impl NSObjectProtocol for BannerDelegate {}

        unsafe impl UNUserNotificationCenterDelegate for BannerDelegate {
            #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
            unsafe fn will_present_notification(
                &self,
                _center: &UNUserNotificationCenter,
                _notification: &UNNotification,
                completion_handler: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
            ) {
                completion_handler.call((
                    UNNotificationPresentationOptions::Banner
                        | UNNotificationPresentationOptions::List,
                ));
            }

            /// 点击通知本体或通知上的按钮。
            /// - 同步前询问类别（sync-review:<pairId>）：点「开始同步」由后端
            ///   直接按建议方向执行同步；点「取消任务」移除待处理差异；点
            ///   通知本体打开主窗口查看。窗口是否存活均可用。
            /// - 测试类别：回传事件给界面（按钮文字即 actionIdentifier）。
            /// - 其他通知（自动同步等）的点击行为交给系统默认，不打扰。
            #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
            unsafe fn did_receive_response(
                &self,
                _center: &UNUserNotificationCenter,
                response: &UNNotificationResponse,
                completion_handler: &DynBlock<dyn Fn()>,
            ) {
                completion_handler.call(());
                let Some(app) = super::APP.get() else { return };
                let action = response.actionIdentifier().to_string();
                if action == UNNotificationDismissActionIdentifier.to_string() {
                    return; // 关闭不打扰
                }
                let identifier = response.notification().request().identifier().to_string();
                // 同步前询问：请求 ID 携带 pairId
                if let Some(pair_id) = identifier.strip_prefix("sync-review:") {
                    let pair_id = pair_id.to_string();
                    if action == "sync" {
                        let app2 = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) =
                                crate::monitor::sync_with_suggestions(&app2, &pair_id).await
                            {
                                eprintln!("[synbackup] 通知发起同步失败: {e}");
                            }
                        });
                    } else if action == "cancel" {
                        let core = app.state::<crate::backend::Core>();
                        core.remove_pending(&pair_id);
                        let _ = app.emit("pending-changed", &pair_id);
                    } else {
                        // 点通知本体：打开主窗口查看差异
                        crate::show_main_window(app);
                    }
                    return;
                }
                // 测试通知类别：回传事件给界面
                let category = response
                    .notification()
                    .request()
                    .content()
                    .categoryIdentifier();
                if category.to_string() != TEST_CATEGORY {
                    return;
                }
                let kind = if action == UNNotificationDefaultActionIdentifier.to_string() {
                    "open"
                } else {
                    "button"
                };
                let _ = app.emit(
                    "notification-action",
                    serde_json::json!({ "kind": kind, "action": action }),
                );
            }
        }
    );

    impl BannerDelegate {
        fn new(mtm: MainThreadMarker) -> Retained<Self> {
            // MainThreadOnly：只在主线程构造（调用方已保证）
            unsafe { msg_send![Self::alloc(mtm), init] }
        }
    }

    /// 启动时调用：请求一次通知授权（系统只弹一次授权对话框）并挂上 delegate。
    pub fn init() {
        let mtm = MainThreadMarker::new().expect("notify::init 必须在主线程调用");
        let delegate = BannerDelegate::new(mtm);
        let center = UNUserNotificationCenter::currentNotificationCenter();
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        // setDelegate 是弱引用，消费 Retained 泄漏保活（引用计数不减，指针有意丢弃）
        let _ = Retained::into_raw(delegate);

        let options = UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound;
        let handler = RcBlock::new(move |granted: Bool, error: *mut NSError| {
            if !granted.as_bool() {
                let detail = unsafe {
                    error.as_ref().map(|e| e.localizedDescription().to_string())
                };
                eprintln!("[synbackup] macOS 通知授权未通过: {detail:?}（请在系统设置 → 通知中允许同备）");
            }
        });
        center.requestAuthorizationWithOptions_completionHandler(options, &handler);
    }

    pub fn send(title: &str, body: &str) -> Result<(), String> {
        let center = UNUserNotificationCenter::currentNotificationCenter();

        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        content.setSound(Some(&UNNotificationSound::defaultSound()));

        // trigger 传 None = 立即投递
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str("synbackup"),
            &content,
            None,
        );
        let done = RcBlock::new(|error: *mut NSError| {
            if let Some(e) = unsafe { error.as_ref() } {
                eprintln!("[synbackup] 通知投递失败: {}", e.localizedDescription());
            }
        });
        center.addNotificationRequest_withCompletionHandler(&request, Some(&done));
        Ok(())
    }

    /// 同步前询问：注册 sync-review 类别（两个动作），请求 ID 携带
    /// pairId；类别每次按当前语言重建（幂等替换）
    pub fn send_ask(title: &str, body: &str, actions: &[String], pair_id: &str) -> Result<(), String> {
        let center = UNUserNotificationCenter::currentNotificationCenter();

        let defs = [
            ("sync", actions.first().map(String::as_str).unwrap_or("同步")),
            ("cancel", actions.get(1).map(String::as_str).unwrap_or("取消")),
        ];
        let action_refs: Vec<Retained<UNNotificationAction>> = defs
            .iter()
            .map(|(id, label)| {
                UNNotificationAction::actionWithIdentifier_title_options(
                    &NSString::from_str(id),
                    &NSString::from_str(label),
                    UNNotificationActionOptionNone,
                )
            })
            .collect();
        let category = UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
            &NSString::from_str(ASK_CATEGORY),
            &NSArray::from_retained_slice(&action_refs),
            &NSArray::from_retained_slice(&[]),
            UNNotificationCategoryOptionNone,
        );
        center.setNotificationCategories(&NSSet::from_retained_slice(&[category]));

        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        content.setSound(Some(&UNNotificationSound::defaultSound()));
        content.setCategoryIdentifier(&NSString::from_str(ASK_CATEGORY));

        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&format!("sync-review:{pair_id}")),
            &content,
            None,
        );
        let done = RcBlock::new(|error: *mut NSError| {
            if let Some(e) = unsafe { error.as_ref() } {
                eprintln!("[synbackup] 通知投递失败: {}", e.localizedDescription());
            }
        });
        center.addNotificationRequest_withCompletionHandler(&request, Some(&done));
        Ok(())
    }

    /// 测试通知：标题 / 内容自由，buttons 非空时注册测试类别挂动作按钮。
    /// 请求 ID 唯一（时间戳），连续发多条测试通知不会被系统去重合并。
    pub fn send_test(title: &str, body: &str, buttons: &[String]) -> Result<(), String> {
        let center = UNUserNotificationCenter::currentNotificationCenter();

        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        content.setSound(Some(&UNNotificationSound::defaultSound()));

        if !buttons.is_empty() {
            // 按钮文字直接作为 actionIdentifier：点击回调里拿到的就是展示文案
            let actions: Vec<Retained<UNNotificationAction>> = buttons
                .iter()
                .map(|b| {
                    UNNotificationAction::actionWithIdentifier_title_options(
                        &NSString::from_str(b),
                        &NSString::from_str(b),
                        UNNotificationActionOptionNone,
                    )
                })
                .collect();
            let category =
                UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
                    &NSString::from_str(TEST_CATEGORY),
                    &NSArray::from_retained_slice(&actions),
                    &NSArray::from_retained_slice(&[]),
                    UNNotificationCategoryOptionNone,
                );
            center.setNotificationCategories(&NSSet::from_retained_slice(&[category]));
            content.setCategoryIdentifier(&NSString::from_str(TEST_CATEGORY));
        }

        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&format!("test-{millis}")),
            &content,
            None,
        );
        let done = RcBlock::new(|error: *mut NSError| {
            if let Some(e) = unsafe { error.as_ref() } {
                eprintln!("[synbackup] 通知投递失败: {}", e.localizedDescription());
            }
        });
        center.addNotificationRequest_withCompletionHandler(&request, Some(&done));
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use notify_rust::Notification;

    pub fn send(title: &str, body: &str) -> Result<(), String> {
        let mut n = Notification::new();
        n.summary(title).body(body);

        #[cfg(target_os = "windows")]
        {
            // 启动时已把自身 AUMID 注册到 HKCU（含 DisplayName 与 IconUri），
            // 通知直接以应用身份发出：显示"同备 Synbackup"与软件图标，
            // 不再落到 PowerShell 的 AUMID 上。
            if let Some(id) = super::IDENTIFIER.get() {
                n.app_id(id);
            }
        }

        // notify-rust 的 show() 返回通知句柄，这里只关心是否投递成功
        n.show().map(|_| ()).map_err(|e| e.to_string())
    }

    /// 测试通知：按钮（通知动作）在 Windows 的 toast 上需要 COM 激活链路，
    /// notify-rust 不支持——忽略按钮只发标题与内容（界面侧已隐藏按钮编辑）。
    pub fn send_test(title: &str, body: &str, buttons: &[String]) -> Result<(), String> {
        let _ = buttons;
        send(title, body)
    }
}

/// Windows：便携版 AUMID 自注册。
///
/// WinRT ToastNotifier 解析 AUMID 的标准路径是
/// `HKCU\Software\Classes\AppUserModelId\<AUMID>`；键下的 `DisplayName`
/// 与 `IconUri` 值决定 toast 归属区显示的名称与图标。安装版的键由
/// NSIS 写入；便携版没有安装器，这里在启动时自行注册——幂等，重复
/// 启动只是覆盖同名值，卸载即随目录删除（注册表残留一个空键无碍）。
#[cfg(target_os = "windows")]
pub fn register_aumid_windows() {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let Some(id) = IDENTIFIER.get() else { return };
    if id.is_empty() || id.chars().any(|c| c.is_control() || c == '"') {
        return;
    }

    let key = format!(r"HKCU\Software\Classes\AppUserModelId\{id}");
    let mut entries: Vec<(&str, &str, String)> =
        vec![("DisplayName", "REG_SZ", "同备 Synbackup".to_string())];

    // 图标：优先用 exe 旁的 icons/ 目录；便携版与开发环境没有该目录，
    // 退回把内置图标落到 %LOCALAPPDATA% 下，保证 IconUri 指向真实文件。
    let near_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("icons").join("32x32.png")));
    match near_exe.as_deref().and_then(icon_file_for_aumid) {
        Some(path) => entries.push(("IconUri", "REG_SZ", path.display().to_string())),
        None => eprintln!("[synbackup] AUMID 图标不可用，通知将无图标"),
    }

    // reg add 幂等覆盖；逐条执行，任一失败仅记录（不影响应用运行）
    for (name, ty, data) in entries {
        let mut cmd = Command::new("reg");
        cmd.arg("add")
            .arg(&key)
            .arg("/v")
            .arg(name)
            .arg("/t")
            .arg(ty)
            .arg("/d")
            .arg(&data)
            .arg("/f");
        cmd.creation_flags(CREATE_NO_WINDOW);
        match cmd.output() {
            Ok(out) if out.status.success() => {}
            Ok(out) => {
                eprintln!(
                    "[synbackup] AUMID 值注册失败（{name}）: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            Err(e) => eprintln!("[synbackup] AUMID 注册命令执行失败（{name}）: {e}"),
        }
    }
}

/// 取一个真实存在的图标文件：exe 旁已有就直接用，否则把内置的 32×32
/// 图标写到 `%LOCALAPPDATA%\Synbackup\` 下（只写一次）。
#[cfg(target_os = "windows")]
fn icon_file_for_aumid(near_exe: &std::path::Path) -> Option<std::path::PathBuf> {
    if near_exe.is_file() {
        return Some(near_exe.to_path_buf());
    }
    let base = std::env::var_os("LOCALAPPDATA")?;
    let dir = std::path::PathBuf::from(base).join("Synbackup");
    let file = dir.join("icon-32.png");
    if !file.is_file() {
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::write(&file, include_bytes!("../icons/32x32.png")).ok()?;
    }
    Some(file)
}
