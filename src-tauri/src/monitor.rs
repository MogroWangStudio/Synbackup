//! 设备监控：系统事件驱动为主、低频兜底轮询为辅。
//!
//! 0.3.0 之前按设置间隔（默认 3 秒）全量枚举挂载点，空转耗电；
//! 现在改为：
//! - macOS：NSWorkspace 的卷挂载 / 卸载 / 重命名通知；
//! - Windows：隐藏消息窗口接收 WM_DEVICECHANGE（设备到达 / 移除）；
//! - 其他平台与兜底：60 秒一次的全量快照对比。
//!
//! 平台监听器只负责"可能有变化"这一个信号，信号统一进入
//! [`DEVICE_EVENT`]，消费循环合并去抖后走与原来相同的快照对比逻辑。

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::backend;
use crate::backend::model::{
    Decision, DiffStatus, PendingReview, SyncPair, Trigger, VolumeInfo, now_ms,
};
use crate::notify;
use crate::{autosync_body, text, text_fmt};

/// 平台监听器发出的"设备可能变化"信号（无载荷，处理一律以快照为准）。
pub static DEVICE_EVENT: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// 兜底全量复查间隔：事件监听覆盖 macOS / Windows 的主体场景，
/// 该节奏用于事件遗漏的保险，也是无事件源平台的唯一来源。
const FALLBACK_INTERVAL: Duration = Duration::from_secs(60);
/// 事件去抖 + 挂载就绪等待：合并插入瞬间的连续事件，也给文件系统一点就绪时间。
const EVENT_SETTLE: Duration = Duration::from_millis(700);

pub fn spawn(app: AppHandle) {
    start_platform_listener();
    tauri::async_runtime::spawn(monitor_loop(app));
}

fn start_platform_listener() {
    #[cfg(target_os = "macos")]
    macos::start();
    #[cfg(target_os = "windows")]
    windows_devices::start();
    // 其余平台没有轻量事件源，仅靠兜底轮询
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // DEVICE_EVENT 在这些平台无人触发，引用以避免 unused 警告
        let _ = &DEVICE_EVENT;
    }
}

// ---- 统一消费循环 ----

async fn monitor_loop(app: AppHandle) {
    let mut prev_mounts: HashSet<String> = HashSet::new();
    let mut last_payload = String::new();
    let mut fallback = tokio::time::interval(FALLBACK_INTERVAL);
    fallback.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = DEVICE_EVENT.notified() => {}
            _ = fallback.tick() => {}
        }
        // 去抖：短时间内的连续事件合并为一次扫描；也给挂载完成留点时间
        tokio::time::sleep(EVENT_SETTLE).await;

        let Some(volumes) = volumes_snapshot(&app).await else {
            eprintln!("[synbackup] 枚举设备失败");
            continue;
        };
        // 先推一次：设备一插上界面就能变，不必等扫描与同步跑完
        emit_devices(&app, &volumes, &mut last_payload);

        let mounts: HashSet<String> = volumes.iter().map(|v| v.mount.clone()).collect();
        let fresh: Vec<VolumeInfo> =
            volumes.iter().filter(|v| !prev_mounts.contains(&v.mount)).cloned().collect();
        prev_mounts = mounts;

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
                notify::notify(
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
                notify::notify(
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
                notify::notify(
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
                notify::notify(
                    app,
                    text(&settings.language, "app_title"),
                    &text_fmt(&settings.language, "diff_body", &pair.name, n),
                );
            }
        }
    }
    Ok(())
}

async fn run_blocking<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

// ---- 平台事件源 ----

#[cfg(target_os = "macos")]
mod macos {
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSWorkspace, NSWorkspaceDidMountNotification, NSWorkspaceDidRenameVolumeNotification,
        NSWorkspaceDidUnmountNotification,
    };
    use objc2_foundation::NSNotification;

    use super::DEVICE_EVENT;

    /// 注册卷事件监听。在主线程调用（setup 回调保证），
    /// NSWorkspace 的挂载通知对移动设备、磁盘镜像、网络卷都适用。
    pub fn start() {
        let center = NSWorkspace::sharedWorkspace().notificationCenter();
        for name in [
            unsafe { NSWorkspaceDidMountNotification },
            unsafe { NSWorkspaceDidUnmountNotification },
            unsafe { NSWorkspaceDidRenameVolumeNotification },
        ] {
            let block = RcBlock::new(|_notification: NonNull<NSNotification>| {
                DEVICE_EVENT.notify_one();
            });
            let observer = unsafe {
                center.addObserverForName_object_queue_usingBlock(
                    Some(name),
                    None,
                    None,
                    &block,
                )
            };
            // observer 与 block 都被通知中心持有，Rust 侧泄漏保活即可
            let _ = Retained::into_raw(observer);
        }
    }
}

#[cfg(target_os = "windows")]
mod windows_devices {
    use std::thread;

    use windows::core::{PCWSTR, w};
    use windows::Win32::Foundation::{HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetModuleHandleW,
        RegisterClassW, RegisterDeviceNotificationW, TranslateMessage, MSG,
        REGISTER_NOTIFICATION_FLAGS, WNDCLASSW, WM_DEVICECHANGE,
    };

    use super::DEVICE_EVENT;

    // Windows SDK dbt.h 的 ABI 稳定值。设备事件只用于唤醒，后续以快照为准，
    // 因此只为两个常量自定数值，不为它们引入整个 Win32_Devices feature。
    const DBT_DEVICEARRIVAL: u32 = 0x8000;
    const DBT_DEVICEREMOVECOMPLETE: u32 = 0x8004;

    pub fn start() {
        thread::Builder::new()
            .name("device-watch".into())
            .spawn(|| unsafe { run() })
            .expect("无法启动设备监听线程");
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_DEVICECHANGE {
            let event = wparam.0 as u32;
            if event == DBT_DEVICEARRIVAL || event == DBT_DEVICEREMOVECOMPLETE {
                DEVICE_EVENT.notify_one();
            }
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    unsafe fn run() {
        // 消息专用窗口（HWND_MESSAGE）：不可见、不抢焦点、不进任务栏
        let Ok(module) = GetModuleHandleW(None) else {
            return;
        };
        let hinstance = HINSTANCE(module.0);
        let class_name = w!("SynbackupDeviceWatch");

        let wc = WNDCLASSW {
            style: Default::default(),
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: Default::default(),
            hCursor: Default::default(),
            hbrBackground: Default::default(),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: class_name,
        };
        if RegisterClassW(&wc).is_err() {
            return;
        }

        let Ok(hwnd) = CreateWindowExW(
            Default::default(),
            class_name,
            w!("Synbackup device watch"),
            Default::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinstance),
            None,
        ) else {
            return;
        };

        // filter 为 null：接收全部设备事件（ wndproc 里按事件类型过滤）；
        // flags 0 = DEVICE_NOTIFY_WINDOW_HANDLE
        let _ = RegisterDeviceNotificationW(
            HANDLE(hwnd.0),
            std::ptr::null(),
            REGISTER_NOTIFICATION_FLAGS(0),
        );

        // 消息循环：随进程退出而结束，资源由系统回收
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
