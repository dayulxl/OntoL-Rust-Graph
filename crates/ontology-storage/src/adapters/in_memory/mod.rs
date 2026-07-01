//! 内存属性图适配器（开发/测试用）。
//!
//! 基于 `petgraph` 存储节点和关系。无持久化、无事务 —
//! 用于单元测试和后期切换前的原型验证。

pub mod executor;
pub mod graph;
