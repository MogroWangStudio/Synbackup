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

/// 测试通知：标题 / 内容 / 按钮全部由界面传入。
/// 按钮（通知动作）在 macOS（UNNotificationAction）与 Windows（toast 的
/// `<action>`）上生效；Linux 的 Desktop Notifications 规范在 notify-rust
/// 下没有动作能力，忽略按钮只发标题与内容（界面侧仅在 Linux 隐藏按钮编辑）。
pub fn test_notify(app: &AppHandle, title: &str, body: &str, buttons: &[String]) -> Result<(), String> {
    // 发送前先做可同步判定的检查（macOS：授权是否被拒）。授权被拒时把
    // 错误码回给界面，而不是让用户面对"点了没反应"。
    imp::precheck()?;
    let (title, body, buttons) = (title.to_owned(), body.to_owned(), buttons.to_vec());
    let app2 = app.clone();
    let send = move || {
        if let Err(e) = imp::send_test(&title, &body, &buttons) {
            eprintln!("[synbackup] 发送测试通知失败: {e}");
            // 投递阶段才暴露的失败（系统未注册 AUMID、权限在发送瞬间被撤销等）
            let _ = app2.emit("notification-error", e);
        }
    };
    if let Err(e) = app.run_on_main_thread(send) {
        let msg = e.to_string();
        eprintln!("[synbackup] 无法派发通知到主线程: {msg}");
        return Err(msg);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
mod imp {
    use std::sync::atomic::{AtomicI8, Ordering};

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

    /// 通知授权状态：0 = 未决（尚未拿到回调）/ 1 = 已授权 / -1 = 被拒绝。
    /// 启动时请求一次；发送前据此判断——未决就再请求一次（系统只在未决时
    /// 弹窗，已决会立即回调），被拒绝则把原因回传界面，而不是"点了没反应"。
    static AUTH: AtomicI8 = AtomicI8::new(0);

    /// 授权被拒时回传给前端的稳定错误码（界面据此本地化提示文案）
    pub const ERR_DENIED: &str = "notification-permission-denied";

    /// 同步可判定的发送前检查：授权已被拒绝就不必再派发（否则系统静默丢弃，
    /// 用户看到的只是"点了没反应"）
    pub fn precheck() -> Result<(), String> {
        if AUTH.load(Ordering::Relaxed) < 0 {
            Err(ERR_DENIED.to_string())
        } else {
            Ok(())
        }
    }

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
                        | UNNotificationPresentationOptions::List
                        | UNNotificationPresentationOptions::Sound,
                ));
            }

            /// 点击任何来自 Synbackup 的通知（本体或按钮）都打开前端窗口；
            /// 仅从通知中心清除（dismiss）不弹窗。
            /// - 同步前询问类别（sync-review:<pairId>）：点「开始同步」由后端
            ///   直接按建议方向执行同步；点「取消任务」移除待处理差异。
            /// - 测试类别：回传事件给界面（按钮文字即 actionIdentifier）。
            /// - 其余通知（自动同步等）：点击仅打开窗口，无额外动作。
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
                    return; // 从通知中心清除：不弹窗
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
                    }
                    crate::show_main_window(app);
                    return;
                }
                // 其余通知：点击一律打开前端窗口
                crate::show_main_window(app);
                // 测试通知类别：回传事件给界面（预览按钮 / 点击反馈）
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

    /// 请求通知授权：系统只在"未决"时弹一次对话框，已决状态会立即回调。
    /// 回调可能落在任意队列——需要触碰主线程对象的调用方自行切回主线程。
    fn request_auth(done: impl Fn(bool) + 'static) {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let options = UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound;
        let handler = RcBlock::new(move |granted: Bool, error: *mut NSError| {
            let ok = granted.as_bool();
            AUTH.store(if ok { 1 } else { -1 }, Ordering::Relaxed);
            if !ok {
                let detail = unsafe { error.as_ref().map(|e| e.localizedDescription().to_string()) };
                eprintln!("[synbackup] macOS 通知授权未通过: {detail:?}（请在系统设置 → 通知中允许 Synbackup）");
            }
            done(ok);
        });
        center.requestAuthorizationWithOptions_completionHandler(options, &handler);
    }

    /// 启动时调用：请求一次通知授权（系统只弹一次授权对话框）并挂上 delegate。
    pub fn init() {
        let mtm = MainThreadMarker::new().expect("notify::init 必须在主线程调用");
        let delegate = BannerDelegate::new(mtm);
        let center = UNUserNotificationCenter::currentNotificationCenter();
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        // setDelegate 是弱引用，消费 Retained 泄漏保活（引用计数不减，指针有意丢弃）
        let _ = Retained::into_raw(delegate);

        request_auth(|_| {});
    }

    /// 投递一条普通通知。标识符带时间戳：同一标识符的请求会互相替换，
    /// 固定 id 会让连续提醒在通知中心里只剩最后一条。
    pub fn send(title: &str, body: &str) -> Result<(), String> {
        let center = UNUserNotificationCenter::currentNotificationCenter();

        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        content.setSound(Some(&UNNotificationSound::defaultSound()));

        // trigger 传 None = 立即投递
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&format!("synbackup-{}", millis())),
            &content,
            None,
        );
        let done = RcBlock::new(|error: *mut NSError| {
            if let Some(e) = unsafe { error.as_ref() } {
                report_delivery_error(e.localizedDescription().to_string());
            }
        });
        center.addNotificationRequest_withCompletionHandler(&request, Some(&done));
        Ok(())
    }

    /// 当前毫秒时间戳（通知标识符用）
    fn millis() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    }

    /// 投递失败：既写日志，也把原因转给界面——点了"发送"却毫无反应，
    /// 用户无从判断是权限、系统设置还是程序问题。
    fn report_delivery_error(msg: String) {
        eprintln!("[synbackup] 通知投递失败: {msg}");
        if let Some(app) = super::APP.get() {
            let _ = app.emit("notification-error", msg);
        }
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
                report_delivery_error(e.localizedDescription().to_string());
            }
        });
        center.addNotificationRequest_withCompletionHandler(&request, Some(&done));
        Ok(())
    }

    /// 测试通知：标题 / 内容自由，buttons 非空时注册测试类别挂动作按钮。
    /// 请求 ID 唯一（时间戳），连续发多条测试通知不会被系统去重合并。
    ///
    /// 发送前先看授权：被拒绝直接回错误码（界面据此提示去系统设置开启）；
    /// 仍未决（启动时的请求可能早于系统就绪而静默失败）就再请求一次，
    /// 授权通过后再补发——这是"点测试通知没反应"的根治点。
    pub fn send_test(title: &str, body: &str, buttons: &[String]) -> Result<(), String> {
        match AUTH.load(Ordering::Relaxed) {
            -1 => Err(ERR_DENIED.to_string()),
            1 => {
                deliver_test(title, body, buttons);
                Ok(())
            }
            _ => {
                let (t, b, btns) = (title.to_owned(), body.to_owned(), buttons.to_vec());
                request_auth(move |ok| {
                    if !ok {
                        if let Some(app) = super::APP.get() {
                            let _ = app.emit("notification-error", ERR_DENIED);
                        }
                        return;
                    }
                    if let Some(app) = super::APP.get() {
                        let app = app.clone();
                        // 授权回调是 Fn（可能被重复调用），闭包内只克隆、不搬走捕获值
                        let (t, b, btns) = (t.clone(), b.clone(), btns.clone());
                        let _ = app.run_on_main_thread(move || deliver_test(&t, &b, &btns));
                    }
                });
                Ok(())
            }
        }
    }

    /// 组装并投递测试通知（须在主线程调用）
    fn deliver_test(title: &str, body: &str, buttons: &[String]) {
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

        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&format!("test-{}", millis())),
            &content,
            None,
        );
        let done = RcBlock::new(|error: *mut NSError| {
            if let Some(e) = unsafe { error.as_ref() } {
                report_delivery_error(e.localizedDescription().to_string());
            }
        });
        center.addNotificationRequest_withCompletionHandler(&request, Some(&done));
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use notify_rust::Notification;

    /// 非 macOS 平台没有需要提前拦截的授权状态
    pub fn precheck() -> Result<(), String> {
        Ok(())
    }

    pub fn send(title: &str, body: &str) -> Result<(), String> {
        let mut n = Notification::new();
        n.summary(title).body(body);

        #[cfg(target_os = "windows")]
        {
            // 启动时已把自身 AUMID 注册到 HKCU（含 DisplayName 与 IconUri），
            // 通知直接以应用身份发出：显示"Synbackup"与软件图标，
            // 不再落到 PowerShell 的 AUMID 上。
            if let Some(id) = super::IDENTIFIER.get() {
                n.app_id(id);
            }
        }

        // notify-rust 的 show() 返回通知句柄，这里只关心是否投递成功
        n.show().map(|_| ()).map_err(|e| e.to_string())
    }

    /// 测试通知（Windows）：toast 原生支持动作按钮，直接用 WinRT 构造带
    /// `<action>` 的 toast——notify-rust 的 Windows 后端不暴露按钮。
    /// 按钮的 action 参数写序号，点击回调里再映射回展示文案；点通知本体
    /// 视为"打开"，与 macOS 的事件口径一致。
    #[cfg(target_os = "windows")]
    pub fn send_test(title: &str, body: &str, buttons: &[String]) -> Result<(), String> {
        use tauri::Emitter;
        use tauri_winrt_notification::{Duration, Toast};

        // AUMID 已在启动时注册到 HKCU；缺失时退回 PowerShell 的 AUMID，
        // 宁可通知归属显示不对，也不能让 toast 被系统静默丢弃
        let app_id = super::IDENTIFIER
            .get()
            .map(String::as_str)
            .unwrap_or(Toast::POWERSHELL_APP_ID);

        let mut toast = Toast::new(app_id)
            .title(title)
            .text1(body)
            .duration(Duration::Short);

        for (i, label) in buttons.iter().enumerate() {
            toast = toast.add_button(label, &format!("test-button-{i}"));
        }

        if !buttons.is_empty() {
            let labels = buttons.to_vec();
            toast = toast.on_activated(move |action| {
                let Some(app) = super::APP.get() else { return Ok(()) };
                crate::show_main_window(app);
                let (kind, label) = match action.as_deref() {
                    Some(arg) => {
                        let label = arg
                            .strip_prefix("test-button-")
                            .and_then(|i| i.parse::<usize>().ok())
                            .and_then(|i| labels.get(i).cloned())
                            .unwrap_or_else(|| arg.to_string());
                        ("button", label)
                    }
                    // 点通知本体：无 action 参数
                    None => ("open", String::new()),
                };
                let _ = app.emit(
                    "notification-action",
                    serde_json::json!({ "kind": kind, "action": label }),
                );
                Ok(())
            });
        }

        toast.show().map_err(|e| e.to_string())
    }

    /// 测试通知（其他非 macOS 平台）：Desktop Notifications 规范在
    /// notify-rust 下没有动作能力——忽略按钮只发标题与内容。
    #[cfg(not(target_os = "windows"))]
    pub fn send_test(title: &str, body: &str, buttons: &[String]) -> Result<(), String> {
        let _ = buttons;
        send(title, body)
    }

    /// 同步前询问通知：同样受限于 notify-rust 不支持 toast 动作按钮，
    /// 这里忽略两个动作与 pairId，只发标题与内容——用户点开应用自行处理。
    /// 与 macOS 的 `send_ask` 保持同一签名，跨平台调用点无需条件编译。
    pub fn send_ask(
        title: &str,
        body: &str,
        actions: &[String],
        pair_id: &str,
    ) -> Result<(), String> {
        let _ = (actions, pair_id);
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
        vec![("DisplayName", "REG_SZ", "Synbackup".to_string())];

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
