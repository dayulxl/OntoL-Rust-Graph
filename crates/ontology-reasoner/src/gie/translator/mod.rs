//! # 语言翻译器
//!
//! 将各语言前缀表达式翻译为 [`QueryPlan`] 查询计划。
//!
//! ## 子模块
//!
//! | 模块 | 语言 / 前缀 | 说明 |
//! |------|------------|------|
//! | `swrl` | SWRL 规则 (`swrl:`) | 规则前提 → 图模式匹配 QueryPlan |
//! | `dwl2` | DWL2 类表达式 (`owl2:`) | ClassExpression → 实例检索 QueryPlan |
//! | `jsonpath` | JSONPath RFC 9535 (`$.`) | JSON 路径 → 图遍历 QueryPlan |
//!
//! ## 架构约束
//!
//! - 翻译器的输出必须是 [`QueryPlan`]，严禁直接构造 [`GraphPattern`]
//! - 翻译器不持有图仓库引用 — 它们只是纯翻译函数
//! - 翻译失败时返回可读的错误信息，不 panic（宽容执行）

pub mod dwl2;
pub mod jsonpath;
pub mod swrl;
