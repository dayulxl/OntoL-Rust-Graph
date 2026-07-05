//! 适配器层 — 实现 `GraphRepository` trait 的具体后端。
//!
//! | 模块         | 后端     | Feature flag  | 状态        |
//! |-------------|----------|---------------|-------------|
//! | `memgraph`  | Memgraph (内存图) | `memgraph` | 主力后端    |
//! | `in_memory` | 内存图   | `in-memory`   | 测试用      |

#[cfg(feature = "memgraph")]
pub mod memgraph;

#[cfg(feature = "in-memory")]
pub mod in_memory;
