//! 后端核心：状态管理 + 各子模块。
//!
//! 设计原则：本模块不依赖任何 Tauri / WebView 类型，
//! WebView 进程退出后所有逻辑仍可独立运行。

pub mod config;
pub mod devices;
pub mod execute;
pub mod history;
pub mod manifest;
pub mod model;
pub mod scan;

use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use model::{AppSettings, ApplyResult, Config, HistoryEntry, PendingReview, SyncPair, Trigger, now_ms};

/// 单条历史最多保留的明细条数：自动同步一次可能涉及上千个文件，
/// 全部写进 JSONL 会让历史文件无限膨胀，界面也读不动。
pub const HISTORY_ITEM_LIMIT: usize = 200;

#[derive(Default)]
struct Inner {
    settings: AppSettings,
    pairs: Vec<SyncPair>,
}

pub struct Core {
    /// 数据目录（config.json / manifests/ / history.jsonl）
    pub dir: PathBuf,
    inner: RwLock<Inner>,
    pending: Mutex<Vec<PendingReview>>,
    /// 全局同步互斥锁：同一时刻只执行一个同步任务
    pub sync_lock: tokio::sync::Mutex<()>,
}

impl Core {
    pub fn new() -> Self {
        let dir = config::default_dir();
        // 便携化后首次启动：把旧版本写在系统目录的数据搬过来
        config::migrate_from_legacy(&dir);
        let cfg = config::load_config(&dir);
        Self {
            dir,
            inner: RwLock::new(Inner { settings: cfg.settings, pairs: cfg.pairs }),
            pending: Mutex::new(Vec::new()),
            sync_lock: tokio::sync::Mutex::new(()),
        }
    }

    pub fn config_snapshot(&self) -> Config {
        let g = self.inner.read().unwrap();
        Config { settings: g.settings.clone(), pairs: g.pairs.clone() }
    }

    pub fn settings(&self) -> AppSettings {
        self.inner.read().unwrap().settings.clone()
    }

    pub fn pairs(&self) -> Vec<SyncPair> {
        self.inner.read().unwrap().pairs.clone()
    }

    pub fn pair_by_id(&self, id: &str) -> Option<SyncPair> {
        self.inner.read().unwrap().pairs.iter().find(|p| p.id == id).cloned()
    }

    fn save(&self) -> Result<(), String> {
        let g = self.inner.read().unwrap();
        config::save_config(&self.dir, &Config {
            settings: g.settings.clone(),
            pairs: g.pairs.clone(),
        })
        .map_err(|e| e.to_string())
    }

    pub fn update_settings(&self, settings: AppSettings) -> Result<(), String> {
        self.inner.write().unwrap().settings = settings;
        self.save()
    }

    pub fn upsert_pair(&self, mut pair: SyncPair) -> Result<(), String> {
        pair.name = pair.name.trim().to_string();
        if pair.name.is_empty() {
            return Err("任务名称不能为空".into());
        }
        let local = PathBuf::from(&pair.local_path);
        if !local.is_absolute() || !local.is_dir() {
            return Err("本地文件夹不存在".into());
        }
        pair.device_label = pair.device_label.trim().to_string();
        if pair.device_label.is_empty() {
            return Err("请选择目标设备".into());
        }
        let rel = devices::normalize_rel(&pair.remote_path);
        if rel.split('/').any(|p| p == "..") {
            return Err("设备上的文件夹路径不合法".into());
        }
        pair.remote_path = rel;

        let mut g = self.inner.write().unwrap();
        if pair.id.is_empty() {
            pair.id = uuid::Uuid::new_v4().to_string();
            pair.created_at = now_ms();
            g.pairs.push(pair);
        } else {
            let Some(slot) = g.pairs.iter_mut().find(|p| p.id == pair.id) else {
                return Err("任务不存在".into());
            };
            let created_at = slot.created_at;
            *slot = pair;
            slot.created_at = created_at;
        }
        drop(g);
        self.save()
    }

    pub fn delete_pair(&self, id: &str) -> Result<(), String> {
        self.inner.write().unwrap().pairs.retain(|p| p.id != id);
        self.remove_pending(id);
        manifest::delete(&self.dir, id);
        self.save()
    }

    pub fn set_pair_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        {
            let mut g = self.inner.write().unwrap();
            let Some(slot) = g.pairs.iter_mut().find(|p| p.id == id) else {
                return Err("任务不存在".into());
            };
            slot.enabled = enabled;
        }
        self.save()
    }

    pub fn touch_last_sync(&self, pair_id: &str) -> Result<(), String> {
        {
            let mut g = self.inner.write().unwrap();
            if let Some(p) = g.pairs.iter_mut().find(|p| p.id == pair_id) {
                p.last_sync_at = Some(now_ms());
            }
        }
        self.save()
    }

    pub fn record_history(&self, pair: &SyncPair, result: &ApplyResult, trigger: Trigger) {
        let entry = HistoryEntry {
            ts: now_ms(),
            trigger,
            pair_id: pair.id.clone(),
            pair_name: pair.name.clone(),
            device_label: pair.device_label.clone(),
            copied_to_remote: result.copied_to_remote,
            copied_to_local: result.copied_to_local,
            deleted_remote: result.deleted_remote,
            deleted_local: result.deleted_local,
            skipped: result.skipped,
            errors: result.errors.clone(),
            items: result.items.iter().take(HISTORY_ITEM_LIMIT).cloned().collect(),
            items_truncated: result.items.len().saturating_sub(HISTORY_ITEM_LIMIT) as u32,
        };
        history::append(&self.dir, &entry);
    }

    // ---- 待处理差异 ----

    pub fn put_pending(&self, review: PendingReview) {
        let mut g = self.pending.lock().unwrap();
        g.retain(|r| r.pair_id != review.pair_id);
        g.push(review);
    }

    pub fn pending_snapshot(&self) -> Vec<PendingReview> {
        self.pending.lock().unwrap().clone()
    }

    pub fn remove_pending(&self, pair_id: &str) {
        self.pending.lock().unwrap().retain(|r| r.pair_id != pair_id);
    }
}
