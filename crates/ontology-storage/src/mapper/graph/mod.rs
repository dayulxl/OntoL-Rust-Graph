//! 属性图内部表示 — 与存储后端无关的数据结构。
//!
//! 这一层定义了属性图的核心抽象：
//! - [`Node`]          — 带标签和属性的节点
//! - [`Relationship`]  — 有向边，带类型和属性
//! - [`GraphPattern`]  — 图模式匹配（用于查询构建）
//! - [`PropertyValue`] — 通用属性值类型

pub mod node;
pub mod pattern;
pub mod property;
pub mod relationship;
