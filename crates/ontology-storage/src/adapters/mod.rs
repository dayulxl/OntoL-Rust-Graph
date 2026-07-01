//! 适配器层 — 实现 `GraphRepository` trait 的具体后端。
//!
//! | 模块         | 后端     | Feature flag  | 状态        |
//! |-------------|----------|---------------|-------------|
//! | `neo4j`     | Neo4j    | `neo4j`       | 前期落地    |
//! | `in_memory` | 内存图   | `in-memory`   | 后期切换    |

#[cfg(feature = "neo4j")]
pub mod neo4j;

#[cfg(feature = "in-memory")]
pub mod in_memory;
