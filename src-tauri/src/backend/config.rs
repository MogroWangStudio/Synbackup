//! 应用数据持久化：config.json（设置 + 任务）、manifests/（同步清单）、history.jsonl（历史）。
//!
//! 便携模式：数据默认保存在可执行程序同级的 `data/` 目录，不读写系统的
//! AppData / Application Support；旧版本遗留的数据做一次性搬迁。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::model::Config;

pub const CONFIG_FILE: &str = "config.json";
pub const MANIFESTS_DIR: &str = "manifests";
pub const HISTORY_FILE: &str = "history.jsonl";

/// 当前包名（用于回退目录与旧数据定位）
pub const APP_ID: &str = "com.mws.synbackup";

/// 便携数据目录：可执行程序同级的 `data/`。exe 路径不可得或目录不可建时
/// 退回用户数据目录（如 macOS 把 .app 装进 /Applications 后包内不可写），
/// 避免数据散落到不可预期的当前工作目录。
pub fn default_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(base) = exe.parent() {
            let portable = base.join("data");
            if fs::create_dir_all(&portable).is_ok() {
                return portable;
            }
        }
    }
    user_dir(APP_ID).unwrap_or_else(|| PathBuf::from("data"))
}

/// 平台用户数据目录（仅作便携目录不可写时的回退）
fn user_dir(id: &str) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join("Library/Application Support").join(id))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA").ok().map(|b| PathBuf::from(b).join(id))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".synbackup"))
    }
}

/// 旧版本使用过的系统数据目录（新包名与旧包名都查），供一次性搬迁。
fn legacy_dirs() -> Vec<PathBuf> {
    [APP_ID, "com.mogrowang.synbackup"]
        .iter()
        .filter_map(|id| user_dir(id))
        .collect()
}

/// 一次性搬迁：便携目录还没有数据而旧目录存在时，把旧数据整体复制过来；
/// 此后不再读写任何系统目录。
pub fn migrate_from_legacy(portable: &Path) {
    if portable.join(CONFIG_FILE).exists() {
        return;
    }
    for legacy in legacy_dirs() {
        if !legacy.join(CONFIG_FILE).exists() {
            continue;
        }
        if copy_tree(&legacy, portable).is_ok() {
            return; // 搬到一份即可
        }
    }
}

/// 递归复制目录（数据目录只有 config.json、history.jsonl 与 manifests/ 一层子目录）
fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

pub fn load_config(dir: &Path) -> Config {
    fs::read_to_string(dir.join(CONFIG_FILE))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_config(dir: &Path, config: &Config) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let json = serde_json::to_string_pretty(config).unwrap_or_default();
    atomic_write(&dir.join(CONFIG_FILE), json.as_bytes())
}

/// 原子写入：先写临时文件再替换，避免写入中途断电/拔盘导致配置损坏。
pub fn atomic_write(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(contents)?;
        f.sync_all().ok();
    }
    #[cfg(target_os = "windows")]
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(&tmp, path)
}
