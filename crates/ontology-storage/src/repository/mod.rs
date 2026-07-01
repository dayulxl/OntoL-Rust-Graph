//! 抽象仓库层 — 业务代码依赖的 Trait 定义。
//!
//! 这一层不依赖任何具体存储实现，上层通过 `Arc<dyn GraphRepository>`
//! 调用，由 `factory` 模块在运行时注入具体适配器。

pub mod graph_store;
pub mod transaction;
