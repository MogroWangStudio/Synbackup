//! 更新检查与自动更新（下载 / 替换 / 重启）。
//!
//! 检查：按设置中的渠道查询 GitHub Releases 的 Atom 摘要源
//! （releases.atom），与当前版本做语义化比较，返回
//! "是否可更新 + 目标版本 + 更新说明 + 下载页 + 本平台安装包"。
//!
//! 不走 api.github.com：匿名 API 按 IP 限流（共享出口网络下极易 403），
//! 部分网络对 api 子域本身也不稳定；Atom 源由 github.com 主站提供，无此限制。
//!
//! 自动更新：便携版没有安装器，流程自己走完——下载对应平台的产物到
//! `data/updates/`，再把"替换 + 重启"交给 `relaunch` 里的系统脚本。
//! 替换必须发生在进程退出之后：Windows 无法覆盖正在运行的 exe，
//! macOS 的 .app 同理，所以安装命令只做暂存与派发，然后立即退出。

use std::fs;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;
use quick_xml::events::Event;
use quick_xml::Reader;
use tauri::{AppHandle, Emitter, Manager};

use crate::backend;

const ATOM_URL: &str = "https://github.com/MogroWangStudio/Synbackup/releases.atom";
const DOWNLOAD_PAGE: &str = "https://github.com/MogroWangStudio/Synbackup/releases/latest";
/// 产物下载前缀（tag 带 v，产物名不带）
const RELEASE_ASSET: &str = "https://github.com/MogroWangStudio/Synbackup/releases/download";

/// Atom 源里 `<link href>` 中 tag 前缀，其后即 tag 本身
const TAG_PATH: &str = "/releases/tag/";

/// 下载取消开关：下载循环按块检查（协作式，跨命令共享）
static CANCEL: AtomicBool = AtomicBool::new(false);
/// 是否已有下载在跑：同一时刻只允许一个下载写 `updates/`
static BUSY: AtomicBool = AtomicBool::new(false);

/// 下载占用守卫：任何返回路径（含出错）都会在析构时放开 BUSY
struct BusyGuard;

impl Drop for BusyGuard {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// 是否有比当前版本更新的发布（按渠道匹配）
    pub available: bool,
    /// 最新发布版本号（无网络/查询失败时为空）
    pub version: String,
    /// 发布说明（纯文本，截断到 3000 字符防超长）
    pub notes: String,
    /// 下载页地址（该版本的发布页，浏览器打开）
    pub url: String,
    /// 当前运行版本
    pub current: String,
    /// 本平台的安装包文件名（空 = 该平台不走自动更新）
    pub asset: String,
    /// 是否支持自动下载并安装
    pub auto: bool,
}

/// 渠道 → 是否接受预发布：稳定通道只看正式版，测试通道接受 Pre-Release
fn accepts_prerelease(channel: &str) -> bool {
    channel == "beta"
}

/// tag → 语义化版本 + 是否预发布。
/// 预发布判定与 CI 发布规则同源：去 v/V 前缀后含 `-` 即 Pre-Release
/// （draft 不会出现在 Atom 源中，无需过滤）。
fn parse_tag(tag: &str) -> Option<(semver::Version, bool)> {
    let t = tag.trim_start_matches(['v', 'V']);
    Some((semver::Version::parse(t).ok()?, t.contains('-')))
}

/// 当前平台对应的发布产物名（与 `.github/workflows/release.yml` 的
/// 整理产物步骤一致）。只列 CI 实际构建的组合：其余架构/系统
/// 退回手动下载页，不猜测产物名。
fn asset_for(version: &str) -> Option<String> {
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "windows" if arch == "x86_64" => Some(format!("Synbackup-{version}-windows-x64.exe")),
        "macos" if arch == "aarch64" => Some(format!("Synbackup-{version}-macos-arm64.dmg")),
        _ => None,
    }
}

#[derive(Debug, Default)]
struct FeedEntry {
    /// 发布 tag（如 v0.4.0-beta1）
    tag: String,
    /// 该版本的发布页
    url: String,
    /// 发布说明（GitHub 渲染后的 HTML）
    notes: String,
}

/// 解析 Atom 摘要源，提取各发布的 tag / 发布页 / 说明。
/// tag 取发布页链接 TAG_PATH 之后的部分而非 id 末段——tag 本身可含 `/`；
/// 说明取 `<content>`（XML 转义的 HTML，由 quick-xml 反转义）。
fn parse_feed(xml: &str) -> Result<Vec<FeedEntry>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut entries = Vec::new();
    let mut cur: Option<FeedEntry> = None;
    let mut in_content = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"entry" => cur = Some(FeedEntry::default()),
                b"content" if cur.is_some() => in_content = true,
                _ => {}
            },
            // link 为自闭合元素，发布页链接携带 tag
            Ok(Event::Empty(e)) if e.name().as_ref() == b"link" => {
                let Some(entry) = cur.as_mut() else { continue };
                for attr in e.attributes() {
                    let Ok(attr) = attr else { continue };
                    if attr.key.as_ref() != b"href" {
                        continue;
                    }
                    let Ok(href) = attr.unescape_value() else { continue };
                    if let Some(pos) = href.find(TAG_PATH) {
                        entry.tag = href[pos + TAG_PATH.len()..].to_string();
                        entry.url = href.into_owned();
                        break;
                    }
                }
            }
            Ok(Event::Text(t)) if in_content => {
                if let (Some(entry), Ok(text)) = (cur.as_mut(), t.unescape()) {
                    entry.notes.push_str(&text);
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"content" => in_content = false,
                b"entry" => {
                    if let Some(entry) = cur.take() {
                        entries.push(entry);
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("GitHub 返回格式异常: {e}")),
            _ => {}
        }
    }
    Ok(entries)
}

/// 发布说明是渲染后的 HTML，界面只做纯文本展示：
/// 段落 / 列表项转成换行，其余标签剥离，常见实体还原。
/// 不做 Markdown 渲染也不注入 innerHTML——发布说明来自外部内容。
fn html_to_text(html: &str) -> String {
    let mut out = String::new();
    let mut tag = String::new();
    let mut in_tag = false;

    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let closing = tag.starts_with('/');
                let name = tag
                    .trim_start_matches('/')
                    .split(|c: char| c.is_whitespace() || c == '/')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                match name.as_str() {
                    "p" | "div" | "ul" | "ol" | "pre" | "blockquote" | "h1" | "h2" | "h3"
                    | "h4" | "h5" | "h6" => {
                        if closing {
                            out.push_str("\n\n");
                        }
                    }
                    "li" => {
                        if closing {
                            out.push('\n');
                        } else {
                            out.push_str("· ");
                        }
                    }
                    "br" => out.push('\n'),
                    _ => {}
                }
            }
            _ if in_tag => tag.push(ch),
            _ => out.push(ch),
        }
    }

    let text = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");

    // 压掉连续空行与行尾空白，发布说明在界面上是密排段落
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() && lines.last().map(|l: &String| l.is_empty()).unwrap_or(true) {
            continue;
        }
        lines.push(line.to_string());
    }
    while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    lines.join("\n")
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<UpdateInfo, String> {
    let core = app.state::<backend::Core>();
    let settings = core.settings();
    let current = app.package_info().version.to_string();

    let client = reqwest::Client::builder()
        .user_agent(concat!("Synbackup/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(ATOM_URL)
        .send()
        .await
        .map_err(|e| format!("网络请求失败: {e}"))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("网络请求失败: {e}"))?;
    if !status.is_success() {
        return Err(format!("GitHub 返回错误: {status}"));
    }

    let current_ver = semver::Version::parse(&current)
        .map_err(|_| format!("当前版本号无法解析: {current}"))?;
    let accept_pre = accepts_prerelease(&settings.update_channel);

    // 渠道过滤 + 语义化比较,取比当前新的最新发布;
    // 预发布按 tag 判定,与 CI 发布规则一致
    let target = parse_feed(&body)?
        .into_iter()
        .filter_map(|e| {
            let (ver, pre) = parse_tag(&e.tag)?;
            if !accept_pre && pre {
                return None;
            }
            if ver <= current_ver {
                return None;
            }
            Some((ver, e))
        })
        .max_by(|a, b| a.0.cmp(&b.0));

    Ok(match target {
        Some((ver, entry)) => {
            let version = ver.to_string();
            let asset = asset_for(&version).unwrap_or_default();
            UpdateInfo {
                available: true,
                auto: !asset.is_empty(),
                asset,
                version,
                notes: html_to_text(&entry.notes).chars().take(3000).collect(),
                url: if entry.url.is_empty() {
                    DOWNLOAD_PAGE.into()
                } else {
                    entry.url
                },
                current,
            }
        }
        None => UpdateInfo {
            available: false,
            auto: false,
            asset: String::new(),
            version: current.clone(),
            notes: String::new(),
            url: DOWNLOAD_PAGE.into(),
            current,
        },
    })
}

#[derive(Clone, Serialize)]
struct Progress {
    received: u64,
    total: u64,
}

/// 下载本平台安装包到 `data/updates/`，返回落地路径。
/// 进度经 `update-progress` 事件推给界面（约 256 KB 一次，避免刷爆 IPC）。
#[tauri::command]
pub async fn download_update(app: AppHandle, version: String) -> Result<String, String> {
    // 上一轮下载尚未收尾（用户关掉向导又立刻重查）时直接拒绝：
    // 两个下载写同一个 .part 会把安装包写坏
    if BUSY.swap(true, Ordering::SeqCst) {
        return Err("已有下载任务在进行，请稍候".into());
    }
    let _busy = BusyGuard;

    let core = app.state::<backend::Core>();
    let asset = asset_for(&version).ok_or_else(|| "当前平台不支持自动更新".to_string())?;
    let dir = core.dir.join(backend::config::UPDATES_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("无法创建下载目录: {e}"))?;
    // 旧包一并清掉：同一目录只留当前这一份，避免占空间也避免用错文件
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let _ = fs::remove_file(entry.path());
        }
    }

    let target = dir.join(&asset);
    let part = dir.join(format!("{asset}.part"));
    CANCEL.store(false, Ordering::SeqCst);

    let client = reqwest::Client::builder()
        .user_agent(concat!("Synbackup/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{RELEASE_ASSET}/v{version}/{asset}");
    let mut resp = client
        .get(url.as_str())
        .send()
        .await
        .map_err(|e| format!("下载失败: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(if status.as_u16() == 404 {
            format!("该版本没有 {asset}，请到发布页手动下载")
        } else {
            format!("下载失败: {status}")
        });
    }
    let total = resp.content_length().unwrap_or(0);

    let mut file = fs::File::create(&part).map_err(|e| format!("无法写入下载文件: {e}"))?;
    let mut received: u64 = 0;
    let mut reported: u64 = 0;
    loop {
        if CANCEL.load(Ordering::SeqCst) {
            drop(file);
            let _ = fs::remove_file(&part);
            return Err("已取消下载".into());
        }
        let chunk = match resp.chunk().await {
            Ok(Some(c)) => c,
            Ok(None) => break,
            Err(e) => {
                drop(file);
                let _ = fs::remove_file(&part);
                return Err(format!("下载中断: {e}"));
            }
        };
        if let Err(e) = file.write_all(&chunk) {
            drop(file);
            let _ = fs::remove_file(&part);
            return Err(format!("无法写入下载文件: {e}"));
        }
        received += chunk.len() as u64;
        if received - reported >= 256 * 1024 || received == total {
            reported = received;
            let _ = app.emit("update-progress", Progress { received, total });
        }
    }
    if let Err(e) = file.flush() {
        return Err(format!("无法写入下载文件: {e}"));
    }
    drop(file);

    // 长度对不上就丢弃：截断的 exe 装上去等于弄坏用户手里的便携版
    if total > 0 && received != total {
        let _ = fs::remove_file(&part);
        return Err(format!("下载不完整（{received}/{total} 字节），请重试"));
    }
    if received < 1024 * 1024 {
        let _ = fs::remove_file(&part);
        return Err("下载内容异常，请到发布页手动下载".into());
    }
    fs::rename(&part, &target).map_err(|e| format!("无法保存安装包: {e}"))?;
    Ok(target.to_string_lossy().to_string())
}

/// 取消进行中的下载（界面上的"取消下载"）
#[tauri::command]
pub fn cancel_update_download() {
    CANCEL.store(true, Ordering::SeqCst);
}

/// 安装已下载的包并退出，由后台脚本完成替换与重启。
#[tauri::command]
pub async fn install_update(app: AppHandle, version: String) -> Result<(), String> {
    let core = app.state::<backend::Core>();
    let asset = asset_for(&version).ok_or_else(|| "当前平台不支持自动更新".to_string())?;
    let staged = core.dir.join(backend::config::UPDATES_DIR).join(&asset);
    if !staged.is_file() {
        return Err("安装包不存在，请重新下载".into());
    }
    let current = std::env::current_exe().map_err(|e| format!("无法定位当前程序: {e}"))?;

    // 同步进行中不动进程：安装会强制退出，正在复制的文件会留下半截。
    // 全局同步锁是唯一的权威信号（自动同步也在它下面跑）。
    let _sync = core
        .sync_lock
        .try_lock()
        .map_err(|_| "同步任务进行中，请完成后再更新".to_string())?;

    prepare_install(&app, &current, &staged, &version)?;

    // 脚本已在等本进程退出；立刻退出，把替换窗口留给它
    app.exit(0);
    Ok(())
}

// ---- 平台安装动作 ----
// 三个同名函数按平台各写一份：调用点不必被 cfg 切成三块，
// 各平台的差异（exe 还是 .app、要不要重命名）留在各自的实现里。

/// Windows：把下载好的 exe 覆盖到原位并按新版本号重命名
#[cfg(target_os = "windows")]
fn prepare_install(
    app: &AppHandle,
    current: &std::path::Path,
    staged: &std::path::Path,
    version: &str,
) -> Result<(), String> {
    let from = app.package_info().version.to_string();
    let target = current.with_file_name(versioned_exe_name(current, &from, version));
    crate::relaunch::spawn_replace_exe(current, staged, &target)
}

/// macOS：从 DMG 里取出 .app 暂存到旧包旁边，退出后整体替换
#[cfg(target_os = "macos")]
fn prepare_install(
    _app: &AppHandle,
    current: &std::path::Path,
    staged: &std::path::Path,
    _version: &str,
) -> Result<(), String> {
    let app_root = crate::relaunch::bundle_root(current)
        .ok_or_else(|| "当前不是 .app 包，无法自动替换".to_string())?;
    // 暂存在同一个父目录：跨卷 mv 会退化成"复制 + 删除"，
    // 而此刻旧包已被移除，复制中途失败就没有退路了
    let staged_app = app_root.with_file_name(".Synbackup-update.app");
    stage_from_dmg(staged, &staged_app)?;
    crate::relaunch::spawn_replace_bundle(&app_root, &staged_app)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn prepare_install(
    _app: &AppHandle,
    _current: &std::path::Path,
    _staged: &std::path::Path,
    _version: &str,
) -> Result<(), String> {
    Err("当前平台不支持自动更新".into())
}

/// 新 exe 的文件名：把旧版本号换成新版本号（文件名即版本，便于用户分辨）；
/// 用户改过名字（如 `Synbackup.exe`）时补上版本号，避免覆盖同名文件后
/// 无从判断手里是哪个版本。
#[cfg(target_os = "windows")]
fn versioned_exe_name(current: &std::path::Path, from: &str, to: &str) -> String {
    let stem = current
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Synbackup");
    let ext = current.extension().and_then(|s| s.to_str()).unwrap_or("exe");
    let renamed = if stem.contains(from) {
        stem.replacen(from, to, 1)
    } else {
        format!("{stem}-{to}")
    };
    format!("{renamed}.{ext}")
}

/// 挂载 DMG，把里面的 .app 复制到 `stage`，随即卸载。
/// 只复制不替换：替换要等本进程退出，见 `relaunch::spawn_replace_bundle`。
#[cfg(target_os = "macos")]
fn stage_from_dmg(dmg: &std::path::Path, stage: &std::path::Path) -> Result<(), String> {
    use std::process::Command;

    let mount = std::env::temp_dir().join(format!("synbackup-update-{}", std::process::id()));
    let _ = fs::remove_dir_all(&mount);
    fs::create_dir_all(&mount).map_err(|e| format!("无法创建挂载点: {e}"))?;

    let attach = Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-mountpoint"])
        .arg(&mount)
        .arg(dmg)
        .output()
        .map_err(|e| format!("无法挂载安装包: {e}"))?;
    if !attach.status.success() {
        let _ = fs::remove_dir_all(&mount);
        let msg = String::from_utf8_lossy(&attach.stderr).trim().to_string();
        return Err(format!("无法挂载安装包: {msg}"));
    }

    let staged = (|| -> Result<(), String> {
        let app = fs::read_dir(&mount)
            .map_err(|e| format!("无法读取安装包内容: {e}"))?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().map(|x| x == "app").unwrap_or(false))
            .ok_or_else(|| "安装包里没有应用".to_string())?;
        let _ = fs::remove_dir_all(stage);
        let out = Command::new("ditto")
            .arg(&app)
            .arg(stage)
            .output()
            .map_err(|e| format!("无法展开新版本: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err("无法展开新版本".into())
        }
    })();

    let _ = Command::new("hdiutil")
        .args(["detach", "-quiet"])
        .arg(&mount)
        .status();
    let _ = fs::remove_dir_all(&mount);
    staged
}
