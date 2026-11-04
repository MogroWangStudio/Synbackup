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

use tauri::AppHandle;

/// 应用 identifier（init 时从 Tauri 配置读取，供 Windows AUMID 使用）
static IDENTIFIER: OnceLock<String> = OnceLock::new();

pub fn init(app: &AppHandle) {
    let _ = IDENTIFIER.set(app.config().identifier.clone());
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

#[cfg(target_os = "macos")]
mod imp {
    use block2::{DynBlock, RcBlock};
    use objc2::rc::Retained;
    use objc2::runtime::{Bool, ProtocolObject};
    use objc2::{msg_send, MainThreadMarker, MainThreadOnly};
    use objc2_foundation::{NSError, NSObject, NSObjectProtocol, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
        UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationSound,
        UNUserNotificationCenter, UNUserNotificationCenterDelegate,
    };

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
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use notify_rust::Notification;

    pub fn send(title: &str, body: &str) -> Result<(), String> {
        let mut n = Notification::new();
        n.summary(title).body(body);

        #[cfg(target_os = "windows")]
        {
            // 安装版（NSIS 注册过 AUMID）→ 用应用身份，通知显示应用名与图标；
            // 便携版 / 开发环境 → 不设置 AUMID，notify-rust 自动落到 PowerShell
            // AUMID，toast 仍会弹出（来源显示为 Windows PowerShell）。
            if aumid_registered() {
                if let Some(id) = super::IDENTIFIER.get() {
                    n.app_id(id);
                }
            }
        }

        // notify-rust 的 show() 返回通知句柄，这里只关心是否投递成功
        n.show().map(|_| ()).map_err(|e| e.to_string())
    }

    /// 检测 NSIS 安装器写入的 AUMID 注册表键是否存在。
    #[cfg(target_os = "windows")]
    fn aumid_registered() -> bool {
        use std::os::windows::process::CommandExt;
        use std::process::Command;

        let Some(id) = super::IDENTIFIER.get() else {
            return false;
        };
        if id.is_empty() || id.chars().any(|c| c.is_control() || c == '"') {
            return false;
        }
        let key = format!(r"HKCU\Software\Classes\AppUserModelId\{id}");
        let mut cmd = Command::new("reg");
        cmd.args(["query", &key]);
        // CREATE_NO_WINDOW：后台查询不能闪出一个控制台窗口
        cmd.creation_flags(0x0800_0000);
        cmd.output().map(|o| o.status.success()).unwrap_or(false)
    }
}
