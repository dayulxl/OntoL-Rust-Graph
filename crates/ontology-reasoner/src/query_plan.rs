//! QueryPlan — 推理层的查询抽象。
//!
//! 推理层不直接构造 `GraphPattern`，而是通过 `QueryPlan` 描述查询意图，
//! 由存储适配器自行翻译为后端查询（Cypher / in-memory scan）。
//!
//! ## 设计目的
//!
//! 解耦推理层与存储层内部 IR（`mapper::graph::pattern`）。
//! `GraphRepository::execute_plan(plan)` 替代 `query_pattern(pattern)`。

use ontology_storage::mapper::graph::property::PropertyValue;

/// 推理层查询抽象 — 不绑定任何后端
#[derive(Debug, Clone)]
pub enum QueryPlan {
    /// 按 IRI 获取单个节点
    GetByCode(String),

    /// 按标签获取所有节点
    GetByLabel(String),

    /// 获取节点的出向关系（可选按类型过滤）
    GetRelationships {
        node_code: String,
        rel_type: Option<String>,
    },

    /// 图模式匹配: (start_label) -[rel_type]-> (end_label)
    PatternMatch {
        start_label: Option<String>,
        rel_type: Option<String>,
        end_label: Option<String>,
        end_properties: Vec<(String, PropertyValue)>,
    },
}

/// 查询结果
#[derive(Debug, Clone)]
pub enum QueryResult {
    Single(Option<ontology_storage::mapper::graph::node::Node>),
    List(Vec<ontology_storage::mapper::graph::node::Node>),
    Relationships(Vec<ontology_storage::mapper::graph::relationship::Relationship>),
    PatternMatches(Vec<(
        ontology_storage::mapper::graph::node::Node,
        Vec<ontology_storage::mapper::graph::relationship::Relationship>,
        ontology_storage::mapper::graph::node::Node,
    )>),
}
