//! SHACL (Shapes Constraint Language) — 图数据约束验证模块。
//!
//! 参考 W3C SHACL 规范，适配属性图模型，
//! 对图中节点执行形状约束验证。
//!
//! # 架构
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │                    SHACL 验证引擎                          │
//! │                                                          │
//! │  ┌──────────────┐  ┌────────────────┐  ┌─────────────┐  │
//! │  │ ShapesGraph   │  │ Target         │  │ Constraint  │  │
//! │  │ (形状定义)     │  │ (目标节点解析)   │  │ Evaluator   │  │
//! │  └──────┬───────┘  └───────┬────────┘  └──────┬──────┘  │
//! │         │                  │                    │        │
//! │         └──────────────────┼────────────────────┘        │
//! │                            │                             │
//! │                     ┌──────▼──────┐                      │
//! │                     │ GraphRepo   │ (只读查询)           │
//! │                     └─────────────┘                      │
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! # 模块组成
//!
//! | 模块 | 职责 |
//! |------|------|
//! | [`ast`] | 形状 AST（Shape, PropertyShape, NodeShape, Constraint, Target） |
//! | [`engine`] | 验证引擎 — 遍历目标节点、评估约束、生成报告 |
//! | [`result`] | 验证结果类型（ValidationResult, ValidationReport） |
//! | [`error`] | SHACL 错误类型 |
//!
//! # 快速开始
//!
//! ```rust,ignore
//! use std::rc::Rc;
//! use ontology_storage::adapters::in_memory::executor::InMemoryAdapter;
//! use ontology_reasoner::shacl::{
//!     ShaclEngine, ShapesGraph, Shape, Target, Constraint, PropertyShape, PropertyPath,
//! };
//!
//! // 1. 获取图仓库
//! let repo = Rc::new(InMemoryAdapter::new());
//!
//! // 2. 构建形状图
//! let mut shapes = ShapesGraph::new();
//! shapes.add_shape(
//!     Shape::node("EntityShape")
//!         .with_target(Target::target_class("Entity"))
//!         .with_constraint(Constraint::required())
//!         .with_property(
//!             PropertyShape::new("code")
//!                 .with_path(PropertyPath::predicate("code"))
//!                 .with_constraint(Constraint::min_length(1))
//!                 .with_constraint(Constraint::max_length(64))
//!         )
//! );
//!
//! // 3. 执行验证
//! let engine = ShaclEngine::new(repo, shapes);
//! let report = engine.validate()?;
//!
//! if report.conforms {
//!     println!("✓ 图数据合规");
//! } else {
//!     println!("✗ 发现 {} 个违规", report.violation_count);
//!     for v in report.violations() {
//!         println!("  - {}", v);
//!     }
//! }
//! ```
//!
//! # 约束类型速览
//!
//! | 约束 | 说明 | 示例 |
//! |------|------|------|
//! | `MinCount / MaxCount` | 属性值基数 | `.with_constraint(Constraint::min_count(1))` |
//! | `Datatype` | 值类型 | `.with_constraint(Constraint::datatype(NodeKind::Int))` |
//! | `MinInclusive / MaxInclusive` | 数值范围 | `.with_constraint(Constraint::min_inclusive(0.0))` |
//! | `MinLength / MaxLength` | 字符串/列表长度 | `.with_constraint(Constraint::max_length(256))` |
//! | `Pattern` | 正则匹配 | `.with_constraint(Constraint::pattern(r"^[A-Z]+$"))` |
//! | `In` | 枚举值 | `.with_constraint(Constraint::in_values(values))` |
//! | `HasValue` | 等值检查 | `.with_constraint(Constraint::has_value(expected))` |
//! | `Required / NonEmpty` | 必填/非空 | `.with_constraint(Constraint::required())` |
//! | `Not / And / Or / Xone` | 逻辑组合 | `.with_constraint(Constraint::or(vec![c1, c2]))` |
//! | `UniqueValue` | 唯一性 | `.with_constraint(Constraint::UniqueValue)` |
//! | `Class` | 节点标签约束 | `.with_constraint(Constraint::Class("Entity".into()))` |
//!
//! # 目标类型
//!
//! | 目标 | 说明 |
//! |------|------|
//! | `TargetClass(label)` | 按节点标签匹配 |
//! | `TargetNode(id)` | 按节点 ID 精确匹配 |
//! | `TargetSubjectsOf(prop)` | 拥有指定出边属性的节点 |
//! | `TargetObjectsOf(prop)` | 被指定属性指向的节点 |
//! | `AllNodes` | 图中所有节点 |

pub mod ast;
pub mod engine;
pub mod error;
pub mod result;

pub use ast::{
    Constraint, NodeKind, NodeShape, PropertyPath, PropertyShape, Severity, Shape, ShapesGraph,
    Target,
};
pub use engine::ShaclEngine;
pub use error::ShaclError;
pub use result::{ValidationReport, ValidationResult};
