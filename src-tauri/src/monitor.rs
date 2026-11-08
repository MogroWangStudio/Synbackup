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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use ::notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter, Manager};

use crate::backend;
use crate::backend::model::{
    Decision, PendingReview, SyncAction, SyncPair, Trigger, VolumeInfo,
};
use crate::notify;
use crate::sync_flow::{self, ScanFailure};
use crate::{autosync_body, text, text_fmt};

/// 平台监听器发出的"设备可能变化"信号（无载荷，处理一律以快照为准）。
pub static DEVICE_EVENT: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// 兜底全量复查间隔：事件监听覆盖 macOS / Windows 的主体场景，
/// 该节奏用于事件遗漏的保险，也是无事件源平台的唯一来源。
const FALLBACK_INTERVAL: Duration = Duration::from_secs(60);
/// 事件去抖 + 挂载就绪等待：合并插入瞬间的连续事件，也给文件系统一点就绪时间。
const EVENT_SETTLE: Duration = Duration::from_millis(700);

/// 文件监视事件去抖：批量写入（含同步自身的写入）落定后再复扫，
/// 避免同步过程中对中间状态反复提醒
const WATCH_DEBOUNCE: Duration = Duration::from_millis(1500);

/// 文件监视器注册表：pair_id → 监视器（一个 watcher 同时监视本地与设备目录）。
/// watcher 存活即监视有效；drop（设备拔出时移除）即解除监视。
static PAIR_WATCHES: OnceLock<Mutex<HashMap<String, RecommendedWatcher>>> = OnceLock::new();
/// 各任务上一次差异签名（路径 + 建议动作）：内容未变不重复提醒 / 不重复同步
static LAST_SIGS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
/// 文件监视事件通道：watcher 回调把 pair_id 发给消费线程去抖
static WATCH_TX: OnceLock<Mutex<std::sync::mpsc::Sender<String>>> = OnceLock::new();

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
    let mut prev_volumes: Vec<VolumeInfo> = Vec::new();
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
        // 设备拔出：通知 + 停止对应任务的文件监视
        let gone: Vec<VolumeInfo> = prev_volumes
            .iter()
            .filter(|v| !mounts.contains(&v.mount))
            .cloned()
            .collect();
        prev_mounts = mounts;
        prev_volumes = volumes.clone();

        for vol in &gone {
            handle_device_removed(&app, vol);
        }
        for vol in fresh {
            if let Err(e) = handle_volume_inserted(&app, &vol, false).await {
                eprintln!("[synbackup] 处理设备 {} 失败: {e}", vol.label);
            }
        }
        // 兜底：为所有在线任务补建文件监视（监视器丢失 / 目录晚建等场景）
        let core = app.state::<backend::Core>();
        for vol in &volumes {
            let bound: Vec<SyncPair> = core
                .pairs()
                .into_iter()
                .filter(|p| p.enabled && p.device_label == vol.label)
                .collect();
            for pair in bound {
                ensure_pair_watchers(&app, &pair, vol);
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
    sync_flow::run_blocking(move || backend::devices::snapshot_with_pairs(&pairs))
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

/// 同步前询问通知（仅在 notify_on_insert 开启时发送）
fn notify_ask_for_review(
    app: &AppHandle,
    settings: &backend::model::AppSettings,
    pair_name: &str,
    pair_id: &str,
    n: usize,
) {
    if !settings.notify_on_insert {
        return;
    }
    let body = text_fmt(&settings.language, "ask_body", pair_name, n);
    notify::notify_ask(
        app,
        &settings.language,
        &text(&settings.language, "app_title"),
        &body,
        pair_id,
    );
}

/// 通知「开始同步」响应：重新扫描并按建议方向执行同步
/// （与自动同步同路径；冲突项按建议跳过）
pub async fn sync_with_suggestions(app: &AppHandle, pair_id: &str) -> Result<(), String> {
    let core = app.state::<backend::Core>();
    let (dir, settings, pair) = {
        let pair = core
            .pairs()
            .into_iter()
            .find(|p| p.id == pair_id)
            .ok_or_else(|| "任务不存在".to_string())?;
        (core.dir.clone(), core.settings(), pair)
    };
    if !pair.enabled {
        return Ok(());
    }
    let Some(volumes) = volumes_snapshot(app).await else {
        return Err("枚举设备失败".into());
    };
    let Some(vol) = volumes.iter().find(|v| {
        v.matched_pair_ids.iter().any(|id| id == &pair.id) || v.label == pair.device_label
    }) else {
        return Err("设备未连接".into());
    };
    let scan = match sync_flow::prepare_and_scan(&dir, &pair, std::slice::from_ref(vol)).await {
        Err(ScanFailure::DeviceNotConnected) => return Err("设备未连接".into()),
        Err(ScanFailure::LocalMissing) => return Err("本地文件夹不存在".into()),
        Err(ScanFailure::Failed(e)) => return Err(e),
        Ok(scan) => scan,
    };
    if scan.items.is_empty() {
        core.touch_last_sync(&pair.id)?;
        return Ok(());
    }
    let decisions: Vec<Decision> = scan
        .items
        .iter()
        .map(|i| Decision { path: i.path.clone(), action: i.suggested })
        .collect();
    notify::notify(
        app,
        text(&settings.language, "app_title"),
        &text_fmt(&settings.language, "sync_started_body", &pair.name, scan.items.len()),
    );
    let (result, updated_manifest) = {
        let lr = scan.local_root;
        let rr = scan.remote_root;
        let items = scan.items;
        let mut manifest = scan.manifest;
        sync_flow::run_blocking(move || {
            let res = backend::execute::apply(&lr, &rr, &items, &decisions, &mut manifest, &|_, _, _| {});
            (res, manifest)
        })
        .await?
    };
    sync_flow::finish_sync(app, &core, &dir, &pair, &result, &updated_manifest, Trigger::Manual)?;
    notify::notify(
        app,
        text(&settings.language, "app_title"),
        &autosync_body(
            &settings.language,
            &pair.name,
            (result.copied_to_remote + result.copied_to_local) as usize,
            result.errors.len(),
        ),
    );
    Ok(())
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

/// 设备侧被监视目录：卷挂载点 + 任务相对路径（相对路径为空即卷根目录）
fn device_watch_dir(pair: &SyncPair, vol: &VolumeInfo) -> PathBuf {
    let base = vol.mount.trim_end_matches(['/', '\\']);
    let rel = pair.remote_path.trim_start_matches(['/', '\\']);
    if rel.is_empty() {
        PathBuf::from(base)
    } else {
        Path::new(base).join(rel)
    }
}

/// 为任务建立文件监视（本地 + 设备目录，递归）。已建立则跳过。
/// 监视失败（目录不存在等）不注册，由兜底循环择机重试。
fn ensure_pair_watchers(app: &AppHandle, pair: &SyncPair, vol: &VolumeInfo) {
    let pair_id = pair.id.clone();
    let tx = WATCH_TX
        .get_or_init(|| {
            let (tx, rx) = std::sync::mpsc::channel::<String>();
            start_watch_consumer(app.clone(), rx);
            Mutex::new(tx)
        })
        .lock()
        .unwrap()
        .clone();
    let map = PAIR_WATCHES.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut reg) = map.lock() else { return };
    if reg.contains_key(&pair.id) {
        return;
    }
    let Ok(mut watcher) = ::notify::recommended_watcher(
        move |res: ::notify::Result<::notify::Event>| {
            if res.is_ok() {
                let _ = tx.send(pair_id.clone());
            }
        },
    ) else {
        return;
    };
    let mut watched = 0;
    let local = PathBuf::from(&pair.local_path);
    if local.is_dir() {
        if watcher.watch(&local, RecursiveMode::Recursive).is_ok() {
            watched += 1;
        }
    }
    let dev = device_watch_dir(pair, vol);
    if dev.is_dir() {
        if watcher.watch(&dev, RecursiveMode::Recursive).is_ok() {
            watched += 1;
        }
    }
    if watched > 0 {
        reg.insert(pair.id.clone(), watcher);
    }
}

/// 文件监视事件消费线程：按 pair 去抖后触发一次差异复扫
fn start_watch_consumer(app: AppHandle, rx: std::sync::mpsc::Receiver<String>) {
    std::thread::spawn(move || {
        let mut pending: HashMap<String, Instant> = HashMap::new();
        loop {
            match rx.recv_timeout(Duration::from_millis(300)) {
                Ok(pair_id) => {
                    pending.insert(pair_id, Instant::now());
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
            let now = Instant::now();
            let due: Vec<String> = pending
                .iter()
                .filter(|(_, t)| now.duration_since(**t) >= WATCH_DEBOUNCE)
                .map(|(k, _)| k.clone())
                .collect();
            for pair_id in due {
                pending.remove(&pair_id);
                let app2 = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = rescan_watched_pair(&app2, &pair_id).await {
                        eprintln!("[synbackup] 监视重扫 {pair_id} 失败: {e}");
                    }
                });
            }
        }
    });
}

/// 监视重扫：差异签名（路径 + 建议动作）有变化时更新待处理并通知 /
/// 按设置自动同步；签名未变只静默跳过（不重复提醒）；差异消失则移除待处理
async fn rescan_watched_pair(app: &AppHandle, pair_id: &str) -> Result<(), String> {
    let core = app.state::<backend::Core>();
    let (dir, settings, pair) = {
        let pair = core
            .pairs()
            .into_iter()
            .find(|p| p.id == pair_id)
            .ok_or_else(|| "任务不存在".to_string())?;
        (core.dir.clone(), core.settings(), pair)
    };
    if !pair.enabled {
        return Ok(());
    }
    let Some(volumes) = volumes_snapshot(app).await else {
        return Ok(()); // 枚举失败：等下一轮
    };
    let Some(vol) = volumes.iter().find(|v| {
        v.matched_pair_ids.iter().any(|id| id == &pair.id) || v.label == pair.device_label
    }) else {
        return Ok(()); // 设备已离线：交给拔出事件处理
    };
    let scan = match sync_flow::prepare_and_scan(&dir, &pair, std::slice::from_ref(vol)).await {
        Err(ScanFailure::DeviceNotConnected) | Err(ScanFailure::LocalMissing) => return Ok(()),
        Err(ScanFailure::Failed(e)) => return Err(e),
        Ok(scan) => scan,
    };
    let sig = scan
        .items
        .iter()
        .map(|i| format!("{}:{:?}", i.path, i.suggested))
        .collect::<Vec<_>>()
        .join("|");
    {
        let sigs = LAST_SIGS.get_or_init(|| Mutex::new(HashMap::new()));
        let Ok(mut m) = sigs.lock() else { return Ok(()) };
        if m.get(pair_id).map(String::as_str) == Some(sig.as_str()) {
            return Ok(()); // 内容未变：不重复提醒
        }
        m.insert(pair_id.to_string(), sig);
    }
    if scan.items.is_empty() {
        // 差异消失（同步完成 / 文件被删除）：静默移除待处理
        core.remove_pending(pair_id);
        return Ok(());
    }
    let review = PendingReview::new(&pair, scan.items);
    let n = review.items.len();
    core.put_pending(review.clone());
    let _ = app.emit("review-available", &review);
    if !settings.notify_on_insert {
        return Ok(());
    }
    if settings.ask_before_sync {
        notify_ask_for_review(app, &settings, &pair.name, &pair.id, n);
    } else {
        notify::notify(
            app,
            text(&settings.language, "app_title"),
            &text_fmt(&settings.language, "watch_diff_body", &pair.name, n),
        );
    }
    Ok(())
}

/// 设备拔出：通知用户 + 停止对应任务的文件监视
fn handle_device_removed(app: &AppHandle, vol: &VolumeInfo) {
    let core = app.state::<backend::Core>();
    let settings = core.settings();
    let all_pairs = core.pairs();
    let bound: Vec<&SyncPair> = all_pairs
        .iter()
        .filter(|p| p.enabled && p.device_label == vol.label)
        .collect();
    if bound.is_empty() {
        return;
    }
    if settings.notify_on_insert {
        notify::notify(
            app,
            text(&settings.language, "app_title"),
            &text_fmt(&settings.language, "device_offline_body", &vol.label, 0),
        );
    }
    let map = PAIR_WATCHES.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut reg) = map.lock() else { return };
    let sigs = LAST_SIGS.get_or_init(|| Mutex::new(HashMap::new()));
    for p in bound {
        reg.remove(&p.id);
        if let Ok(mut m) = sigs.lock() {
            m.remove(&p.id);
        }
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

    let notify_user = |body: &str| {
        if settings.notify_on_insert {
            notify::notify(app, text(&settings.language, "app_title"), body);
        }
    };

    for pair in pairs {
        // 自动同步也得先扫描：只开了"无冲突自动同步"而关了"插入时自动检查"时，
        // 以前会直接 continue，自动同步（以及它的通知）永远不会发生。
        // 该守卫放在设备解析之前：两者都关闭时不为每个任务白读一遍设备标记。
        if !settings.auto_scan_on_insert && !settings.auto_sync_no_conflict && !force_scan {
            continue;
        }
        let scan = match sync_flow::prepare_and_scan(&dir, &pair, std::slice::from_ref(vol)).await {
            Err(ScanFailure::DeviceNotConnected) => continue,
            Err(ScanFailure::LocalMissing) => {
                notify_user(&text_fmt(&settings.language, "local_missing", &pair.name, 0));
                continue;
            }
            Err(ScanFailure::Failed(e)) => return Err(e),
            Ok(scan) => scan,
        };

        // 扫描成功即设备在线：建立 / 确认该任务的文件监视
        ensure_pair_watchers(app, &pair, vol);

        let core = app.state::<backend::Core>();
        if scan.items.is_empty() {
            core.touch_last_sync(&pair.id)?;
            continue;
        }

        // 建议操作里没有 Skip（即全部可按建议方向复制）视为无冲突
        let safe = scan.items.iter().all(|i| i.suggested != SyncAction::Skip);

        if settings.ask_before_sync {
            // 同步前询问：不自动执行，发带「开始同步 / 取消任务」按钮的
            // 持久通知（macOS），由用户在通知里决定后续
            let review = PendingReview::new(&pair, scan.items);
            let n = review.items.len();
            core.put_pending(review.clone());
            core.record_scan_history(
                &pair,
                &review.items,
                if force_scan { Trigger::Manual } else { Trigger::Auto },
            );
            let _ = app.emit("review-available", &review);
            notify_ask_for_review(app, &settings, &pair.name, &pair.id, n);
        } else if safe && settings.auto_sync_no_conflict {
            // 无冲突：按建议方向自动执行
            notify_user(&text_fmt(
                &settings.language,
                "sync_started_body",
                &pair.name,
                scan.items.len(),
            ));
            let decisions: Vec<Decision> = scan
                .items
                .iter()
                .map(|i| Decision { path: i.path.clone(), action: i.suggested })
                .collect();
            let (result, updated_manifest) = {
                let lr = scan.local_root;
                let rr = scan.remote_root;
                let items = scan.items;
                let mut manifest = scan.manifest;
                sync_flow::run_blocking(move || {
                    let res = backend::execute::apply(&lr, &rr, &items, &decisions, &mut manifest, &|_, _, _| {});
                    (res, manifest)
                })
                .await?
            };
            sync_flow::finish_sync(app, &core, &dir, &pair, &result, &updated_manifest, Trigger::Auto)?;
            notify_user(&autosync_body(
                &settings.language,
                &pair.name,
                (result.copied_to_remote + result.copied_to_local) as usize,
                result.errors.len(),
            ));
        } else {
            let review = PendingReview::new(&pair, scan.items);
            let n = review.items.len();
            core.put_pending(review.clone());
            // 检查到差异但未执行同步：写入"检查到差异"历史记录
            core.record_scan_history(
                &pair,
                &review.items,
                if force_scan { Trigger::Manual } else { Trigger::Auto },
            );
            let _ = app.emit("review-available", &review);
            notify_user(&text_fmt(&settings.language, "diff_body", &pair.name, n));
        }
    }
    Ok(())
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
    // GetModuleHandleW 属于 System::LibraryLoader（不是 WindowsAndMessaging），
    // 对应 Cargo.toml 里的 Win32_System_LibraryLoader feature。
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
        RegisterDeviceNotificationW, TranslateMessage, MSG, REGISTER_NOTIFICATION_FLAGS,
        WNDCLASSW, WM_DEVICECHANGE,
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
        // RegisterClassW 返回 ATOM（u16），0 表示失败
        if RegisterClassW(&wc) == 0 {
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
