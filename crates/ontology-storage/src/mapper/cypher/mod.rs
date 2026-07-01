//! Neo4j Cypher 方言层。
//!
//! 仅在 `neo4j` feature 开启时编译。
//! 负责将 `graph::pattern` 转化为参数化的 Cypher 查询语句。

pub mod builder;
pub mod params;
