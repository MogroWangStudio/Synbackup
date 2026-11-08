//! 更新检查：按设置中的渠道查询 GitHub Releases 的 Atom 摘要源
//! （releases.atom），与当前版本做语义化比较，返回
//! "是否可更新 + 目标版本 + 更新说明 + 下载页"。
//!
//! 不走 api.github.com：匿名 API 按 IP 限流（共享出口网络下极易 403），
//! 部分网络对 api 子域本身也不稳定；Atom 源由 github.com 主站提供，无此限制。
//!
//! 不做自动下载/安装：本应用以便携版为主，签名与安装器基建缺位，
//! 打开下载页让用户手动获取是当前最稳妥的闭环。

use serde::Serialize;
use quick_xml::events::Event;
use quick_xml::Reader;
use tauri::{AppHandle, Manager};

use crate::backend;

const ATOM_URL: &str = "https://github.com/MogroWangStudio/Synbackup/releases.atom";
const DOWNLOAD_PAGE: &str = "https://github.com/MogroWangStudio/Synbackup/releases/latest";

/// Atom 源里 `<link href>` 中 tag 前缀，其后即 tag 本身
const TAG_PATH: &str = "/releases/tag/";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// 是否有比当前版本更新的发布（按渠道匹配）
    pub available: bool,
    /// 最新发布版本号（无网络/查询失败时为空）
    pub version: String,
    /// 发布说明（GitHub 渲染后的 HTML，截断到 2000 字符防超长）
    pub notes: String,
    /// 下载页地址（该版本的发布页，浏览器打开）
    pub url: String,
    /// 当前运行版本
    pub current: String,
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

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<UpdateInfo, String> {
    let core = app.state::<backend::Core>();
    let settings = core.settings();
    let current = app.package_info().version.to_string();

    let client = reqwest::Client::builder()
        .user_agent(concat!("Synbackup/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(10))
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
        Some((ver, entry)) => UpdateInfo {
            available: true,
            version: ver.to_string(),
            notes: entry.notes.chars().take(2000).collect(),
            url: if entry.url.is_empty() {
                DOWNLOAD_PAGE.into()
            } else {
                entry.url
            },
            current,
        },
        None => UpdateInfo {
            available: false,
            version: current.clone(),
            notes: String::new(),
            url: DOWNLOAD_PAGE.into(),
            current,
        },
    })
}
