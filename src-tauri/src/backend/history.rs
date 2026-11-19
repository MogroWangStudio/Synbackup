//! 同步历史：JSONL 追加写入，读取时倒序返回。

use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
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
    if let Ok(mut line) = serde_json::to_string(entry) {
        // 一次写入：分两次 write_all 时被进程中断，文件里会留下半行，
        // 之后每条读取都会在同一处解析失败
        line.push('\n');
        let _ = f.write_all(line.as_bytes());
    }
}

pub fn read(dir: &Path, limit: usize) -> Vec<HistoryEntry> {
    if limit == 0 {
        return Vec::new();
    }
    let Some(text) = tail_lines(&dir.join(config::HISTORY_FILE), limit) else {
        return Vec::new();
    };
    let mut out: Vec<HistoryEntry> =
        text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    out.reverse();
    out.truncate(limit);
    out
}

/// 取文件末尾足够容纳 `want` 行的片段，避免为了最近 200 条把整部历史读一遍
/// （每条记录最多 200 项明细，历史文件会随时间线性长到几十 MB）。
///
/// 从文件尾按块向前扩窗，直到凑够 `want + 1` 个换行或触及文件头。起点落在
/// 文件中间时首行是被截断的半行，丢掉即可；`from_utf8_lossy` 保证在多字节
/// 字符中间下刀也不会 panic。
fn tail_lines(path: &Path, want: usize) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut window = 64 * 1024u64;
    loop {
        let start = len.saturating_sub(window);
        f.seek(SeekFrom::Start(start)).ok()?;
        let mut buf = vec![0u8; (len - start) as usize];
        f.read_exact(&mut buf).ok()?;

        let newlines = buf.iter().filter(|b| **b == b'\n').count();
        if start == 0 {
            return Some(String::from_utf8_lossy(&buf).into_owned());
        }
        if newlines > want {
            let text = String::from_utf8_lossy(&buf).into_owned();
            return Some(match text.find('\n') {
                Some(i) => text[i + 1..].to_string(),
                None => String::new(),
            });
        }
        window = window.saturating_mul(4);
    }
}

/// 历史总条数（= 累计同步次数）。逐行流式计数，不整读文件。
pub fn count(dir: &Path) -> u64 {
    let Ok(f) = std::fs::File::open(dir.join(config::HISTORY_FILE)) else {
        return 0;
    };
    BufReader::new(f)
        .lines()
        .map_while(Result::ok)
        .filter(|l| !l.trim().is_empty())
        .count() as u64
}

pub fn clear(dir: &Path) {
    let _ = std::fs::remove_file(dir.join(config::HISTORY_FILE));
}
