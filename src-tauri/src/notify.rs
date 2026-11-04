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

    // 图标：exe 同级的 icons/ 目录。IconUri 用 PNG 即可；
    // 缺失时仍注册 DisplayName（通知有名字、无图标），不阻断启动。
    let icon_path = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("icons").join("32x32.png")));
    let Some(icon_path) = icon_path else { return };
    if !icon_path.is_file() {
        eprintln!("[synbackup] AUMID 图标缺失，通知将无图标：{}", icon_path.display());
    }

    let key = format!(r"HKCU\Software\Classes\AppUserModelId\{id}");
    // reg add 幂等覆盖；逐条执行，任一失败仅记录（不影响应用运行）
    let entries: [(&str, &str, &str); 2] = [
        ("DisplayName", "REG_SZ", "同备 Synbackup"),
        ("IconUri", "REG_EXPAND_SZ", &icon_uri.to_string_lossy()),
    ];
    for (name, ty, data) in entries {
        let mut cmd = Command::new("reg");
        cmd.args(["add", &key, "/v", name, "/t", ty, "/d", data, "/f"]);
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
