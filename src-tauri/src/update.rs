//! 更新检查与自动更新（下载 / 替换 / 重启）。
//!
//! 检查：按设置中的渠道查询 GitHub Releases 的 Atom 摘要源
//! （releases.atom），与当前版本做语义化比较，返回
//! "是否可更新 + 目标版本 + 更新说明 + 下载页 + 本平台安装包"。
//!
//! 不走 api.github.com：匿名 API 按 IP 限流（共享出口网络下极易 403），
//! 部分网络对 api 子域本身也不稳定；Atom 源由 github.com 主站提供，无此限制。
//!
//! 自动更新：便携版没有安装器，流程自己走完——下载对应平台的 zip 包到
//! `data/updates/`，解压校验后再把"替换 + 重启"交给 `relaunch` 里的系统脚本。
//! 替换必须发生在进程退出之后：Windows 无法覆盖正在运行的 exe，
//! macOS 的 .app 同理，所以安装命令只做暂存与派发，然后立即退出。
//!
//! 产物名带版本号（`Synbackup_<版本>_windows-x64.zip` /
//! `Synbackup_<版本>_macos-arm64.zip`），按 tag 拼出唯一下载路径；
//! 包内文件名固定（`Synbackup.exe` / `Synbackup.app`），替换时也不改名——
//! 用户手里的程序文件名从此与升级多少次无关。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;
use quick_xml::events::Event;
use quick_xml::Reader;
use tauri::{AppHandle, Emitter, Manager};

use crate::backend;

const ATOM_URL: &str = "https://github.com/MogroWangStudio/Synbackup/releases.atom";
const DOWNLOAD_PAGE: &str = "https://github.com/MogroWangStudio/Synbackup/releases/latest";
/// 产物下载前缀（tag 带 v；包名带版本号，包内文件名固定）
const RELEASE_ASSET: &str = "https://github.com/MogroWangStudio/Synbackup/releases/download";

/// gh-proxy 加速前缀：任意 GitHub 直链前拼此域名即经代理下载
/// （https://gh-proxy.com/docs/github-accelerator），代理端跟随 302、
/// 支持 Range 分块，检查 / 下载 / 测速三条链路共用
const GH_PROXY_PREFIX: &str = "https://gh-proxy.org/";

/// 未指定测速目标时的兜底直链：仓库源码归档，始终存在且有数 MB，
/// 读一小段就足够量出可用性与速度
const SPEED_TEST_URL: &str =
    "https://github.com/MogroWangStudio/Synbackup/archive/refs/heads/main.zip";

/// 按下载来源处理直链：gh-proxy 来源时在链接前拼加速前缀
fn mirror_url(url: &str, source: &str) -> String {
    match source {
        "ghproxy" => format!("{GH_PROXY_PREFIX}{url}"),
        _ => url.to_string(),
    }
}

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
    /// 本平台安装包的下载直链（GitHub 原始链接，测速用；空 = 不支持自动更新）
    pub asset_url: String,
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
/// 整理产物步骤一致）。压缩包名带版本号，包内文件名固定——升级后用户
/// 手里的程序名不会跟着变。只列 CI 实际构建的
/// 组合：其余架构/系统退回手动下载页，不猜测产物名。
fn asset_for(version: &str) -> Option<String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some(format!("Synbackup_{version}_windows-x64.zip")),
        ("macos", "aarch64") => Some(format!("Synbackup_{version}_macos-arm64.zip")),
        _ => None,
    }
}

#[derive(Debug, Default)]
struct FeedEntry {
    /// 发布 tag（如 v0.4.0-beta1）
    tag: String,
    /// 该版本的发布页
    url: String,
    /// 发布时间（Atom `<updated>` 原文，RFC3339；界面自己解析显示）
    updated: String,
    /// 发布说明（GitHub 渲染后的 HTML）
    notes: String,
}

/// 解析 Atom 摘要源，提取各发布的 tag / 发布页 / 发布时间 / 说明。
/// tag 取发布页链接 TAG_PATH 之后的部分而非 id 末段——tag 本身可含 `/`；
/// 说明取 `<content>`（XML 转义的 HTML，由 quick-xml 反转义）。
fn parse_feed(xml: &str) -> Result<Vec<FeedEntry>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut entries = Vec::new();
    let mut cur: Option<FeedEntry> = None;
    let mut in_content = false;
    let mut in_updated = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"entry" => cur = Some(FeedEntry::default()),
                b"content" if cur.is_some() => in_content = true,
                b"updated" if cur.is_some() => in_updated = true,
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
            Ok(Event::Text(t)) if in_updated => {
                if let (Some(entry), Ok(text)) = (cur.as_mut(), t.unescape()) {
                    entry.updated = text.into_owned();
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"content" => in_content = false,
                b"updated" => in_updated = false,
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
        .get(mirror_url(ATOM_URL, &settings.update_source))
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
                asset_url: if asset.is_empty() {
                    String::new()
                } else {
                    format!("{RELEASE_ASSET}/v{version}/{asset}")
                },
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
            asset_url: String::new(),
            version: current.clone(),
            notes: String::new(),
            url: DOWNLOAD_PAGE.into(),
            current,
        },
    })
}

/// 版本列表条目（设置 → 关于的「版本列表」弹窗）。
/// is_downgrade 由后端按语义化版本比好，前端不必再实现版本比较。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionEntry {
    pub version: String,
    /// 预发布（tag 含 `-`，与渠道判定同源）
    pub is_prerelease: bool,
    /// 是否早于当前运行版本（降级目标）
    pub is_downgrade: bool,
    /// 是否正是当前运行版本
    pub is_current: bool,
    /// 发布时间（Atom `<updated>` 原文 RFC3339；前端解析显示）
    pub updated: String,
    /// 发布说明（纯文本，截断到 3000 字符）
    pub notes: String,
    /// 发布页
    pub url: String,
    /// 本平台安装包名（空 = 该版本无本平台产物或平台不支持自动更新）
    pub asset: String,
    /// 本平台安装包直链（GitHub 原始链接）
    pub asset_url: String,
    /// 是否支持自动下载安装
    pub auto: bool,
}

/// 全部发布的版本列表（新版本在前），供手动选择任意版本安装或降级。
/// 与检查更新同一条 Atom 源，但不做"比当前新"过滤——降级也是它的职责。
#[tauri::command]
pub async fn list_update_versions(app: AppHandle) -> Result<Vec<VersionEntry>, String> {
    let current_ver = semver::Version::parse(app.package_info().version.to_string().as_str())
        .map_err(|_| "当前版本号无法解析".to_string())?;

    let client = reqwest::Client::builder()
        .user_agent(concat!("Synbackup/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let core = app.state::<backend::Core>();
    let resp = client
        .get(mirror_url(ATOM_URL, &core.settings().update_source))
        .send()
        .await
        .map_err(|e| format!("网络请求失败: {e}"))?;
    let status = resp.status();
    let body = resp.text().await.map_err(|e| format!("网络请求失败: {e}"))?;
    if !status.is_success() {
        return Err(format!("GitHub 返回错误: {status}"));
    }

    let mut entries: Vec<(semver::Version, FeedEntry)> = parse_feed(&body)?
        .into_iter()
        .filter_map(|e| parse_tag(&e.tag).map(|(v, _)| (v, e)))
        .collect();
    // 新版本在前：语义化版本倒序（重复 tag 去重，保留第一份）
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    entries.dedup_by(|a, b| a.0 == b.0);

    Ok(entries
        .into_iter()
        .map(|(ver, entry)| {
            let version = ver.to_string();
            let asset = asset_for(&version).unwrap_or_default();
            VersionEntry {
                is_downgrade: ver < current_ver,
                is_current: ver == current_ver,
                is_prerelease: version.contains('-'),
                asset_url: if asset.is_empty() {
                    String::new()
                } else {
                    format!("{RELEASE_ASSET}/v{version}/{asset}")
                },
                asset: asset.clone(),
                notes: html_to_text(&entry.notes).chars().take(3000).collect(),
                updated: entry.updated,
                url: if entry.url.is_empty() {
                    DOWNLOAD_PAGE.into()
                } else {
                    entry.url
                },
                auto: !asset.is_empty(),
                version,
            }
        })
        .collect())
}

#[derive(Clone, Serialize)]
struct Progress {
    received: u64,
    total: u64,
}

/// 清空目录（含子目录）：旧包与上一次的解压结果都要清掉，
/// 同一目录只留当前这一份，避免占空间也避免用错文件
fn clear_dir(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            let _ = fs::remove_dir_all(&path);
        } else {
            let _ = fs::remove_file(&path);
        }
    }
}

/// 在解压结果里按条件找第一个匹配项（先看顶层，再向下找一层以上）
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn find_entry(dir: &Path, pred: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if pred(&path) {
            return Some(path);
        }
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            if let Some(found) = find_entry(&path, pred) {
                return Some(found);
            }
        }
    }
    None
}

/// 解压更新包，返回包内本平台程序的路径。
///
/// 每次重建解压目录：上一轮安装可能已经把包里的文件移走，残留半套
/// 东西比重新解压更危险。
fn unpack_update(zip: &Path, dir: &Path) -> Result<PathBuf, String> {
    let unpacked = dir.join("unpacked");
    let _ = fs::remove_dir_all(&unpacked);
    fs::create_dir_all(&unpacked).map_err(|e| format!("无法创建解压目录: {e}"))?;

    extract_archive(zip, &unpacked)?;
    staged_program(&unpacked)
}

/// Windows：逐条写入，`enclosed_name` 拒绝 `..` 与绝对路径，
/// 恶意构造的包写不到 `updates/` 之外
#[cfg(target_os = "windows")]
fn extract_archive(zip: &Path, dest: &Path) -> Result<(), String> {
    let file = fs::File::open(zip).map_err(|e| format!("无法读取更新包: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("更新包已损坏: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("更新包已损坏: {e}"))?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out = dest.join(&rel);
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(|e| format!("无法解压更新包: {e}"))?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("无法解压更新包: {e}"))?;
        }
        let mut dst = fs::File::create(&out).map_err(|e| format!("无法解压更新包: {e}"))?;
        std::io::copy(&mut entry, &mut dst).map_err(|e| format!("无法解压更新包: {e}"))?;
    }
    Ok(())
}

/// macOS：交给 ditto。.app 里可能有符号链接，自行逐个写文件会把它变成
/// 普通文件（解出来的包结构就坏了）；ditto 按 zip 内的元数据还原链接、
/// 权限与扩展属性，签名信息与首次启动行为才和手动解压一致。
#[cfg(target_os = "macos")]
fn extract_archive(zip: &Path, dest: &Path) -> Result<(), String> {
    use std::process::Command;

    let out = Command::new("ditto")
        .args(["-x", "-k"])
        .arg(zip)
        .arg(dest)
        .output()
        .map_err(|e| format!("无法解压更新包: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(format!("更新包已损坏: {msg}"))
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn extract_archive(_zip: &Path, _dest: &Path) -> Result<(), String> {
    Err("当前平台不支持自动更新".into())
}

/// 包里本平台程序的位置（包内文件名固定，与版本无关）
#[cfg(target_os = "windows")]
fn staged_program(unpacked: &Path) -> Result<PathBuf, String> {
    find_entry(unpacked, &|p| {
        p.extension()
            .map(|x| x.eq_ignore_ascii_case("exe"))
            .unwrap_or(false)
    })
    .ok_or_else(|| "更新包里没有程序，请到发布页手动下载".to_string())
}

/// 包里本平台程序的位置（.app 是目录，按扩展名匹配）
#[cfg(target_os = "macos")]
fn staged_program(unpacked: &Path) -> Result<PathBuf, String> {
    find_entry(unpacked, &|p| {
        p.extension().map(|x| x == "app").unwrap_or(false)
    })
    .ok_or_else(|| "更新包里没有应用，请到发布页手动下载".to_string())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn staged_program(_unpacked: &Path) -> Result<PathBuf, String> {
    Err("当前平台不支持自动更新".into())
}

/// 下载并解压本平台更新包，返回解压出的程序路径。
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
    let settings = core.settings();
    let asset = asset_for(&version).ok_or_else(|| "当前平台不支持自动更新".to_string())?;
    let dir = core.dir.join(backend::config::UPDATES_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("无法创建下载目录: {e}"))?;
    clear_dir(&dir);

    let target = dir.join(&asset);
    let part = dir.join(format!("{asset}.part"));
    CANCEL.store(false, Ordering::SeqCst);

    let client = reqwest::Client::builder()
        .user_agent(concat!("Synbackup/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let url = mirror_url(&format!("{RELEASE_ASSET}/v{version}/{asset}"), &settings.update_source);
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

    // 下载完就解压并校验：包能不能用要在用户点"安装"之前就知道，
    // 而不是等退出进程后由替换脚本才发现包里没有程序
    let staged = unpack_update(&target, &dir)?;
    Ok(staged.to_string_lossy().to_string())
}

/// 下载来源测速结果：reachable = 请求成功且读到数据，不可达时 error 携带原因
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedProbe {
    /// 请求成功且读到数据
    pub reachable: bool,
    /// 发出请求到收到响应头的耗时（毫秒）
    pub latency_ms: u64,
    /// 实际读取的字节数
    pub bytes: u64,
    /// 读取全程耗时（毫秒）
    pub elapsed_ms: u64,
    /// 平均速度（字节/秒）
    pub speed_bps: u64,
    /// 实际请求的链接（含加速前缀，便于界面与排查对上号）
    pub url: String,
    /// 失败原因（成功时为空）
    pub error: String,
}

impl SpeedProbe {
    fn unreachable(url: &str, error: String) -> Self {
        Self {
            reachable: false,
            latency_ms: 0,
            bytes: 0,
            elapsed_ms: 0,
            speed_bps: 0,
            url: url.to_string(),
            error,
        }
    }
}

/// 测速：按当前下载来源对一条更新直链发起带 Range 的请求，读最多 1 MiB
/// （10 秒封顶），返回延迟与平均速度。用于在切换来源前确认链接可用、
/// 快慢如何。`url` 缺省时测仓库源码归档——它始终存在，量的是同一条路。
#[tauri::command]
pub async fn probe_update_speed(app: AppHandle, url: Option<String>) -> Result<SpeedProbe, String> {
    const MAX_PROBE_BYTES: u64 = 1024 * 1024;
    const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

    let core = app.state::<backend::Core>();
    let settings = core.settings();
    let target = mirror_url(
        url.as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(SPEED_TEST_URL),
        &settings.update_source,
    );

    let client = reqwest::Client::builder()
        .user_agent(concat!("Synbackup/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let started = std::time::Instant::now();
    let mut resp = match client
        .get(&target)
        .header(reqwest::header::RANGE, "bytes=0-1048575")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return Ok(SpeedProbe::unreachable(&target, e.to_string())),
    };
    let latency_ms = started.elapsed().as_millis() as u64;
    let status = resp.status();
    if !status.is_success() {
        return Ok(SpeedProbe::unreachable(&target, format!("HTTP {status}")));
    }

    let mut bytes: u64 = 0;
    let deadline = tokio::time::Instant::from_std(started + PROBE_TIMEOUT);
    loop {
        if bytes >= MAX_PROBE_BYTES {
            break;
        }
        // 到点即停：手头读到多少算多少，量速不需要读完整段
        let chunk = match tokio::time::timeout_at(deadline, resp.chunk()).await {
            Ok(Ok(Some(c))) => c,
            Ok(Ok(None)) | Err(_) => break,
            Ok(Err(e)) => return Ok(SpeedProbe::unreachable(&target, e.to_string())),
        };
        bytes += chunk.len() as u64;
    }
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let speed_bps = if elapsed_ms > 0 {
        ((bytes as f64) / (elapsed_ms as f64 / 1000.0)) as u64
    } else {
        0
    };
    Ok(SpeedProbe {
        reachable: bytes > 0,
        latency_ms,
        bytes,
        elapsed_ms,
        speed_bps,
        url: target,
        error: String::new(),
    })
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
    let dir = core.dir.join(backend::config::UPDATES_DIR);
    let archive = dir.join(asset);
    if !archive.is_file() {
        return Err("安装包不存在，请重新下载".into());
    }
    // 重新解压一遍：上一轮若已把包内文件移走，目录里可能只剩半套
    let staged = unpack_update(&archive, &dir)?;
    let current = std::env::current_exe().map_err(|e| format!("无法定位当前程序: {e}"))?;

    // 同步进行中不动进程：安装会强制退出，正在复制的文件会留下半截。
    // 全局同步锁是唯一的权威信号（自动同步也在它下面跑）。
    let _sync = core
        .sync_lock
        .try_lock()
        .map_err(|_| "同步任务进行中，请完成后再更新".to_string())?;

    prepare_install(&current, &staged)?;

    // 脚本已在等本进程退出；立刻退出，把替换窗口留给它
    app.exit(0);
    Ok(())
}

// ---- 平台安装动作 ----
// 三个同名函数按平台各写一份：调用点不必被 cfg 切成三块，
// 各平台的差异（exe 还是 .app，暂存到哪里）留在各自的实现里。

/// Windows：包里的 exe 覆盖到当前程序的位置。不按版本号改名——
/// 装完还是同一个文件名，用户桌面上的快捷方式、任务栏固定项都不会失效。
#[cfg(target_os = "windows")]
fn prepare_install(current: &std::path::Path, staged: &std::path::Path) -> Result<(), String> {
    crate::relaunch::spawn_replace_exe(current, staged, current)
}

/// macOS：把包里的 .app 暂存到旧包旁边，退出后整体替换
#[cfg(target_os = "macos")]
fn prepare_install(current: &std::path::Path, staged: &std::path::Path) -> Result<(), String> {
    let app_root = crate::relaunch::bundle_root(current)
        .ok_or_else(|| "当前不是 .app 包，无法自动替换".to_string())?;
    // 暂存在同一个父目录：跨卷 mv 会退化成"复制 + 删除"，
    // 而此刻旧包已被移除，复制中途失败就没有退路了
    let staged_app = app_root.with_file_name(".Synbackup-update.app");
    stage_from_app(staged, &staged_app)?;
    crate::relaunch::spawn_replace_bundle(&app_root, &staged_app)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn prepare_install(_current: &std::path::Path, _staged: &std::path::Path) -> Result<(), String> {
    Err("当前平台不支持自动更新".into())
}

/// 把解压出来的 .app 用 ditto 复制到 `stage`（不是 zip 内的简单拷贝：
/// ditto 保留资源分支、扩展属性与权限，签名与首次启动行为才和手动安装一致）。
/// 只复制不替换：替换要等本进程退出，见 `relaunch::spawn_replace_bundle`。
#[cfg(target_os = "macos")]
fn stage_from_app(src: &std::path::Path, stage: &std::path::Path) -> Result<(), String> {
    use std::process::Command;

    let _ = fs::remove_dir_all(stage);
    let out = Command::new("ditto")
        .arg(src)
        .arg(stage)
        .output()
        .map_err(|e| format!("无法展开新版本: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err("无法展开新版本".into())
    }
}


