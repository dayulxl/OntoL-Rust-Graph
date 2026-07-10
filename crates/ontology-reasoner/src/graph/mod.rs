//! 通用图遍历与推理模块。
//!
//! # 产品层（framework）与业务层（domain）分离
//!
//! 本模块提供**产品代码**：
//! - [`GraphExplorer`] — 通用多跳图遍历引擎
//! - [`StateChangeDetector`] — 可插拔的状态变化检测 trait
//! - 通用工具函数 — 实体查找、关系汇总、类型层次、规则匹配、下一步预测
//!
//! **业务代码**位于 `ontology-server` 层，通过实现 [`StateChangeDetector`] trait
//! 注入领域知识（如军事 ASW 的 `Space_abs` 位置解析、中文关系语义等）。
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use ontology_reasoner::graph::{GraphExplorer, ExploreConfig, StateChangeDetector};
//!
//! let explorer = GraphExplorer::new(repo);
//! let result = explorer.explore(&ExploreConfig {
//!     start_id: "P8A_001".into(),
//!     relation: "移动".into(),
//!     max_depth: 3,
//!     direction: Default::default(),
//! })?;
//!
//! // 业务方注入领域检测器
//! for hop in &result.chain {
//!     let changes = my_detector.detect_changes(
//!         hop.source_node.as_ref(), hop.target_node.as_ref(), &hop.rel_type
//!     );
//! }
//! ```

pub mod detector;
pub mod explorer;
pub mod util;

pub use detector::{DefaultStateChangeDetector, StateChangeDetector};
pub use explorer::{Direction, ExploreConfig, ExploreHop, ExploreResult, GraphExplorer};
pub use util::{
    RelCount, RuleMatch, clone_all_for_version, delete_by_cope_version, ensure_cope_version,
    find_entity_any, find_entity_by_id_code, find_incoming_relationships, find_matching_rules,
    get_type_ancestors, predict_next_steps, prop_as_f64, summarize_relations, truncate_str,
    update_entity_properties,
};
