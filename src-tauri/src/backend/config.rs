//! 应用数据持久化：config.json（设置 + 任务）、manifests/（同步清单）、history.jsonl（历史）。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::model::Config;

pub const CONFIG_FILE: &str = "config.json";
pub const MANIFESTS_DIR: &str = "manifests";
pub const HISTORY_FILE: &str = "history.jsonl";

/// 数据目录（不依赖 Tauri 运行时，便于独立测试）
pub fn default_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join("Library/Application Support/com.mogrowang.synbackup")
    }
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
        PathBuf::from(base).join("com.mogrowang.synbackup")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".synbackup")
    }
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
