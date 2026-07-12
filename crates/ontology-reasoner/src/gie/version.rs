//! # VersionControl — 场景版本管理
//!
//! 负责管理 `cope_version`（场景版本标识），提供完整的场景生命周期管理：
//! - **版本创建**：克隆原实体到副本空间
//! - **快照**：保存当前副本空间的完整状态
//! - **回滚**：恢复到之前的快照
//! - **清理**：删除指定版本的所有副本
//!
//! ## 副本机制
//!
//! ```text
//! 原实体 (cope_version = "")  ──clone_snapshot()──▶  副本 (cope_version = "v2.0")
//!                                                        │
//!                                                        ├── 推理/修改执行
//!                                                        │
//!                                                        ├── save_snapshot()
//!                                                        │
//!                                                        ├── rollback()
//!                                                        │
//!                                                        └── cleanup()
//! ```
//!
//! 参见 ARCHITECTURE.md §7.2 和 CLAUDE.md §7.1。

use std::collections::HashMap;

/// 场景版本快照 — 记录某一时刻的副本空间状态。
///
/// 包含快照时间点和副本节点 code 列表，用于回滚和审计。
#[derive(Debug, Clone)]
pub struct SceneSnapshot {
    /// 快照对应的版本标识。
    pub version: String,
    /// 快照时的副本 code → 原始 code 映射。
    pub code_map: HashMap<String, String>,
    /// 快照中克隆的节点总数。
    pub cloned_count: usize,
    /// 快照创建时间戳（毫秒，自 epoch 起算）。
    pub timestamp_ms: u64,
}

/// 场景版本控制器。
///
/// 管理推理场景的版本标识，提供版本创建、快照、回滚、清理等完整的
/// 场景生命周期管理功能。
///
/// # 示例
///
/// ```rust,ignore
/// let mut vc = VersionControl::new("v2.0".to_string());
///
/// // 标记开始克隆
/// vc.begin_clone();
/// // ... 执行 clone_all_for_version(repo, ..., vc.current_version()) ...
/// vc.end_clone(new_code_map, cloned_count);
///
/// // 保存快照
/// vc.save_snapshot();
///
/// // ... 执行推理 ...
///
/// // 回滚到快照
/// if let Some(snap) = vc.latest_snapshot() {
///     vc.rollback_to(snap);
/// }
///
/// // 清理
/// vc.cleanup(repo);
/// ```
pub struct VersionControl {
    /// 当前活跃的版本标识。
    current_version: String,

    /// 原始 code → 副本 code 的映射（在当前版本中）。
    code_map: HashMap<String, String>,

    /// 版本快照列表（按时间排序，最新的在末尾）。
    snapshots: Vec<SceneSnapshot>,

    /// 正在执行克隆的标志。
    cloning_in_progress: bool,
}

impl VersionControl {
    // ═══════════════════════════════════════════════════════
    // 构造
    // ═══════════════════════════════════════════════════════

    /// 创建新的版本控制器，使用给定的版本标识。
    pub fn new(version: String) -> Self {
        Self {
            current_version: version,
            code_map: HashMap::new(),
            snapshots: Vec::new(),
            cloning_in_progress: false,
        }
    }

    /// 返回当前版本标识。
    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    /// 切换到新的版本标识，清空当前 code_map 和快照。
    pub fn switch_to(&mut self, version: String) {
        self.current_version = version;
        self.code_map.clear();
        self.snapshots.clear();
        self.cloning_in_progress = false;
    }

    // ═══════════════════════════════════════════════════════
    // 克隆生命周期
    // ═══════════════════════════════════════════════════════

    /// 标记克隆阶段开始。
    ///
    /// 在调用 `clone_all_for_version` 或 `clone_nodes_selective` 之前调用。
    pub fn begin_clone(&mut self) {
        self.cloning_in_progress = true;
    }

    /// 标记克隆阶段结束，记录 code 映射。
    ///
    /// # 参数
    ///
    /// - `code_map` — 原始 code → 副本 code
    /// - `cloned_count` — 已克隆的节点数量
    pub fn end_clone(&mut self, code_map: HashMap<String, String>, cloned_count: usize) {
        self.code_map = code_map;
        self.cloning_in_progress = false;
        log::info!(
            "VersionControl: 版本 '{}' 克隆完成 — {} 个节点, {} 个映射",
            self.current_version,
            cloned_count,
            self.code_map.len()
        );
    }

    /// 是否正在执行克隆。
    pub fn is_cloning(&self) -> bool {
        self.cloning_in_progress
    }

    /// 返回当前 code 映射的引用。
    pub fn code_map(&self) -> &HashMap<String, String> {
        &self.code_map
    }

    /// 将原始 code 转换为副本 code。未找到时返回原始值（宽容执行）。
    pub fn resolve_code(&self, original_code: &str) -> String {
        self.code_map
            .get(original_code)
            .cloned()
            .unwrap_or_else(|| original_code.to_string())
    }

    // ═══════════════════════════════════════════════════════
    // 快照
    // ═══════════════════════════════════════════════════════

    /// 保存当前副本空间的快照。
    ///
    /// 在每次推理迭代前后可调用此方法，保留回滚点。
    /// 返回新创建的快照引用。
    pub fn save_snapshot(&mut self) -> &SceneSnapshot {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let snapshot = SceneSnapshot {
            version: self.current_version.clone(),
            code_map: self.code_map.clone(),
            cloned_count: self.code_map.len(),
            timestamp_ms,
        };

        self.snapshots.push(snapshot);
        // SAFETY: just pushed, so last() always returns Some
        self.snapshots.last().unwrap()
    }

    /// 返回最新的快照。
    pub fn latest_snapshot(&self) -> Option<&SceneSnapshot> {
        self.snapshots.last()
    }

    /// 返回所有快照列表。
    pub fn snapshots(&self) -> &[SceneSnapshot] {
        &self.snapshots
    }

    /// 快照数量。
    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    // ═══════════════════════════════════════════════════════
    // 回滚
    // ═══════════════════════════════════════════════════════

    /// 回滚到指定快照的状态。
    ///
    /// 恢复到快照中的 code_map。注意：此方法只更新内存中的 code_map，
    /// 实际的图数据回滚需要调用方配合 `delete_by_cope_version` +
    /// 重新克隆来执行。
    ///
    /// # 参数
    ///
    /// - `snapshot` — 要回滚到的目标快照
    pub fn rollback_to(&mut self, snapshot: &SceneSnapshot) {
        self.code_map = snapshot.code_map.clone();
        log::info!(
            "VersionControl: 版本 '{}' 回滚到快照 ({} 个节点)",
            self.current_version,
            snapshot.cloned_count
        );
    }

    /// 回滚到最近一次快照。
    ///
    /// 如果没有快照，不做任何操作并返回 `false`。
    pub fn rollback_latest(&mut self) -> bool {
        if let Some(snap) = self.snapshots.last().cloned() {
            self.rollback_to(&snap);
            true
        } else {
            log::debug!(
                "VersionControl: 版本 '{}' 无快照可回滚",
                self.current_version
            );
            false
        }
    }

    // ═══════════════════════════════════════════════════════
    // 清理
    // ═══════════════════════════════════════════════════════

    /// 清空内存中的 code_map 和快照列表（图数据清理由调用方通过
    /// `delete_by_cope_version` 执行）。
    pub fn clear_memory(&mut self) {
        let snap_count = self.snapshots.len();
        let map_count = self.code_map.len();
        self.code_map.clear();
        self.snapshots.clear();
        self.cloning_in_progress = false;
        log::info!(
            "VersionControl: 版本 '{}' 内存状态已清空 ({} 个映射, {} 个快照)",
            self.current_version,
            map_count,
            snap_count
        );
    }
}

impl Default for VersionControl {
    fn default() -> Self {
        Self::new("default".to_string())
    }
}
