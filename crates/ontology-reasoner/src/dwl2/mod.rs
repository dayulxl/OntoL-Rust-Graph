//! DWL2 DL — 描述逻辑查询语言。
//!
//! 从属性图数据库中提取本体对象（Class、Property、Individual）及其关系。
//!
//! ## 子模块
//!
//! | 模块      | 职责                                          |
//! |-----------|-----------------------------------------------|
//! | `ast`     | 类表达式 AST（Conjunction, Disjunction, ...） |
//! | `query`   | 将 AST 编译为 `GraphPattern` 并在图上执行     |

pub mod ast;
pub mod query;

pub use ast::{
    Cardinality, ClassExpression, PropertyRestriction, Quantifier,
};
pub use query::Dwl2QueryEngine;
