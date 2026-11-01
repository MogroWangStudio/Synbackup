//! 同步历史：JSONL 追加写入，读取时倒序返回。

use std::io::Write;
use std::path::Path;

use super::config;
use super::model::HistoryEntry;

pub fn append(dir: &Path, entry: &HistoryEntry) {
    let _ = std::fs::create_dir_all(dir);
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(config::HISTORY_FILE))
    else {
        return;
    };
    if let Ok(line) = serde_json::to_string(entry) {
        let _ = f.write_all(line.as_bytes());
        let _ = f.write_all(b"\n");
    }
}

pub fn read(dir: &Path, limit: usize) -> Vec<HistoryEntry> {
    let Ok(text) = std::fs::read_to_string(dir.join(config::HISTORY_FILE)) else {
        return Vec::new();
    };
    let mut out: Vec<HistoryEntry> =
        text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    out.reverse();
    out.truncate(limit);
    out
}

pub fn clear(dir: &Path) {
    let _ = std::fs::remove_file(dir.join(config::HISTORY_FILE));
}
