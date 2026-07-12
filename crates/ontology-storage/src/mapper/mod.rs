//! 核心转换层 — 本体模型与属性图之间的双向映射。
//!
//! ## 设计原则
//!
//! 属性图模型（Property Graph）而非 RDF 三元组：
//! - **Class**    → `(:Class { iri, label, ... })` 节点
//! - **Property** → `[:HAS_PROPERTY { domain, range, ... }]` 关系
//! - **Individual** → `(:Individual { iri, type, ... })` 节点
//!
//! ## 子模块
//!
//! | 模块                  | 职责                                    |
//! |-----------------------|-----------------------------------------|
//! | `unified_mapping`     | 统一映射层 — 图 ↔ OWL2 词汇表 SSOT     |
//! | `model_mapper`        | Class / Property / Individual ↔ 属性图 |
//! | `iri_normalizer`      | IRI ↔ 内部 ID / Name 映射              |
//! | `property_converter`  | Rust 类型 → Neo4j 属性值               |
//! | `graph`               | 属性图内部表示（存储无关）              |
//! | `cypher` (feature)    | Neo4j 方言层，仅当 `neo4j` feature 开启 |

pub mod graph;
pub mod iri_normalizer;
pub mod model_mapper;
pub mod property_converter;
pub mod query_plan;
pub mod unified_mapping;

#[cfg(feature = "llm")]
pub mod llm;
