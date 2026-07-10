//! # ontology-storage
//!
//! 存储层 crate，提供属性图数据库的抽象仓库模式。
//!
//! ## 架构分层
//!
//! - **`repository`**  — 业务依赖的 Trait 抽象（`GraphRepository`, `Transaction`）
//! - **`mapper`**      — 核心转换层：本体模型 ↔ 属性图 ↔ Cypher 方言
//! - **`adapters`**    — 适配器实现：`memgraph` / `in_memory`
//! - **`factory`**     — 运行时工厂，根据配置返回 `Arc<dyn GraphRepository>`
//!
//! ## Feature flags
//!
//! - `memgraph` (默认) — 启用 Memgraph 后端适配器（主力）
//! - `in-memory`       — 启用内存属性图存储（测试用）

pub mod adapters;
pub mod error;
pub mod factory;
pub mod mapper;
pub mod repository;

#[cfg(any(feature = "memgraph", feature = "llm"))]
pub mod ontology;

pub use error::{GraphError, MappingError, StoreError};
pub use mapper::graph::pattern::{GraphPattern, NodePattern, RelationshipPattern};
pub use mapper::graph::property::PropertyValue;
pub use mapper::graph::{node::Node, relationship::Relationship};

#[cfg(feature = "llm")]
pub use mapper::llm;

pub use factory::StorageConfig;
pub use repository::graph_store::GraphRepository;
pub use repository::transaction::Transaction;
