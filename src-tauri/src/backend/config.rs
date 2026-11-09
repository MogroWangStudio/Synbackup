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
/// 数据目录指针：始终位于便携目录内，内容为自定义数据目录的绝对路径。
/// 指针与数据分开存放，"换目录"才不需要先读新目录里的配置。
pub const LOCATION_FILE: &str = "location.txt";
/// 更新包暂存目录（切换数据目录时不随迁）
pub const UPDATES_DIR: &str = "updates";

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

/// 实际生效的数据目录：便携目录里的指针优先，指针缺失 / 失效时用便携目录。
///
/// 指针指向的目录必须已存在（用户换到移动硬盘后又拔掉时不做任何事，
/// 静默回落到便携目录，避免在内置盘上"凭空重建"一份数据）。界面上的
/// 数据目录以 `get_app_info` 返回值为准，用户随时能看到真实位置。
pub fn resolve_dir() -> PathBuf {
    let base = default_dir();
    let Ok(raw) = fs::read_to_string(base.join(LOCATION_FILE)) else {
        return base;
    };
    let target = PathBuf::from(raw.trim());
    if target.is_absolute() && target.is_dir() {
        target
    } else {
        base
    }
}

/// 指针位置（便携目录），切换目录时写这里
pub fn pointer_file() -> PathBuf {
    default_dir().join(LOCATION_FILE)
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
    copy_tree_skipping(src, dst, &[])
}

/// 递归复制目录，跳过指定名字的顶层条目（更新包暂存与指针文件不随迁）
fn copy_tree_skipping(src: &Path, dst: &Path, skip: &[&str]) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if skip.iter().any(|s| name.to_string_lossy().as_ref() == *s) {
            continue;
        }
        let target = dst.join(&name);
        if entry.file_type()?.is_dir() {
            copy_tree_skipping(&entry.path(), &target, &[])?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// 切换数据目录：校验 → 目标为空时把现有数据复制过去 → 写指针。
/// 立即生效需要重启：`Core.dir` 在启动时确定，进程内改指会让
/// 正在进行的监视与同步落到两套目录上，重启是唯一干净的切换点。
pub fn switch_data_dir(current: &Path, target: &Path) -> Result<PathBuf, String> {
    let raw = target.to_string_lossy().trim().to_string();
    if raw.is_empty() {
        return Err("数据目录不能为空".into());
    }
    let target = PathBuf::from(&raw);
    if !target.is_absolute() {
        return Err("请选择绝对路径".into());
    }
    fs::create_dir_all(&target).map_err(|e| format!("无法创建目录: {e}"))?;
    // 可写性实测：只读的移动硬盘、受系统保护的目录在这里就拦住，
    // 而不是等到重启后写配置才发现数据丢了
    let probe = target.join(".synbackup-write-test");
    fs::write(&probe, b"ok").map_err(|e| format!("目标目录不可写: {e}"))?;
    let _ = fs::remove_file(&probe);

    if let (Ok(a), Ok(b)) = (fs::canonicalize(current), fs::canonicalize(&target)) {
        if a == b {
            return Err("与当前数据目录相同".into());
        }
        // 互相嵌套会让复制把目标抄进自己，或把数据摊到父目录里
        if b.starts_with(&a) || a.starts_with(&b) {
            return Err("不能选择当前数据目录的上级或下级文件夹".into());
        }
    }

    // 目标已有配置：视为用户主动复用该目录，不覆盖它的数据
    if !target.join(CONFIG_FILE).exists() {
        copy_tree_skipping(current, &target, &[LOCATION_FILE, UPDATES_DIR])
            .map_err(|e| format!("复制现有数据失败: {e}"))?;
    }

    let pointer = pointer_file();
    if let Some(parent) = pointer.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("无法写入数据目录指针: {e}"))?;
    }
    fs::write(&pointer, target.to_string_lossy().as_bytes())
        .map_err(|e| format!("无法写入数据目录指针: {e}"))?;
    Ok(target)
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
