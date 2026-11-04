//! 更新检查：按设置中的渠道查询 GitHub Releases,
//! 与当前版本做语义化比较，返回"是否可更新 + 目标版本 + 更新说明 + 下载页"。
//!
//! 不做自动下载/安装：本应用以便携版为主，签名与安装器基建缺位，
//! 打开下载页让用户手动获取是当前最稳妥的闭环。

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::backend;

const API_URL: &str = "https://api.github.com/repos/MogroWangStudio/Synbackup/releases";
const DOWNLOAD_PAGE: &str = "https://github.com/MogroWangStudio/Synbackup/releases/latest";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// 是否有比当前版本更新的发布（按渠道匹配）
    pub available: bool,
    /// 最新发布版本号（无网络/查询失败时为空）
    pub version: String,
    /// 发布说明（markdown 原文，截断到 2000 字符防超长）
    pub notes: String,
    /// 下载页地址（浏览器打开）
    pub url: String,
    /// 当前运行版本
    pub current: String,
}

/// 渠道 → 是否接受预发布：稳定通道只看正式版，测试通道接受 Pre-Release
fn accepts_prerelease(channel: &str) -> bool {
    channel == "beta"
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

    // 取前 10 条足够覆盖两种渠道的最新发布
    let resp = client
        .get(API_URL)
        .query(&[("per_page", "10")])
        .send()
        .await
        .map_err(|e| format!("网络请求失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("GitHub 返回错误: {e}"))?;

    let releases: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let Some(list) = releases.as_array() else {
        return Err("GitHub 返回格式异常".into());
    };

    let current_ver = semver::Version::parse(&current)
        .map_err(|_| format!("当前版本号无法解析: {current}"))?;
    let accept_pre = accepts_prerelease(&settings.update_channel);

    // 渠道过滤 + 语义化比较,取比当前新的最新发布;
    // prerelease 与 draft 一律不作为稳定通道的候选
    let target = list
        .iter()
        .filter(|r| r.get("draft").and_then(|v| v.as_bool()) != Some(true))
        .filter(|r| accept_pre || r.get("prerelease").and_then(|v| v.as_bool()) != Some(true))
        .filter_map(|r| {
            let tag = r.get("tag_name")?.as_str()?;
            let ver = semver::Version::parse(tag.trim_start_matches(['v', 'V'])).ok()?;
            Some((r, ver))
        })
        .max_by_key(|(_, ver)| ver.clone())
        .filter(|(_, ver)| *ver > current_ver);

    Ok(match target {
        Some((r, ver)) => UpdateInfo {
            available: true,
            version: ver.to_string(),
            notes: r
                .get("body")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .chars()
                .take(2000)
                .collect(),
            url: DOWNLOAD_PAGE.into(),
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
