//! 同步清单：记录上次同步时每个文件在两侧的状态，用于识别
//! 「哪一侧发生了变化」与真正的双向冲突（两侧都有修改）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::config;
use super::model::FileStat;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManifestEntry {
    #[serde(default)]
    pub local: Option<FileStat>,
    #[serde(default)]
    pub remote: Option<FileStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Manifest {
    pub version: u32,
    #[serde(default)]
    pub entries: BTreeMap<String, ManifestEntry>,
}

fn manifest_path(dir: &Path, pair_id: &str) -> PathBuf {
    dir.join(config::MANIFESTS_DIR).join(format!("{pair_id}.json"))
}

pub fn load(dir: &Path, pair_id: &str) -> Manifest {
    std::fs::read_to_string(manifest_path(dir, pair_id))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, pair_id: &str, manifest: &Manifest) -> std::io::Result<()> {
    let path = manifest_path(dir, pair_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(manifest).unwrap_or_default();
    config::atomic_write(&path, json.as_bytes())
}

pub fn delete(dir: &Path, pair_id: &str) {
    let _ = std::fs::remove_file(manifest_path(dir, pair_id));
}
