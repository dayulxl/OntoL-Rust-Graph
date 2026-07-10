use std::collections::HashMap;

use crate::mapper::graph::property::PropertyValue;

/// 节点匹配模式。
///
/// 所有字段均为 `Option` — 用于构建部分匹配查询。
/// 如 `NodePattern { labels: Some("Class"), properties: {}, .. }`
/// 匹配所有标签为 `:Class` 的节点。
#[derive(Debug, Clone, Default)]
pub struct NodePattern {
    /// 精确匹配的标签
    pub labels: Option<String>,

    /// 必须匹配的属性条件（AND 语义）
    pub properties: HashMap<String, PropertyValue>,

    /// 变量名（Cypher 中用，如 `MATCH (n:Class)` 中的 `n`）
    pub variable: Option<String>,
}

impl NodePattern {
    pub fn with_label(label: &str) -> Self {
        Self {
            labels: Some(label.to_string()),
            ..Default::default()
        }
    }

    pub fn with_variable(mut self, var: &str) -> Self {
        self.variable = Some(var.to_string());
        self
    }

    pub fn with_property(mut self, key: &str, value: PropertyValue) -> Self {
        self.properties.insert(key.to_string(), value);
        self
    }
}

/// 关系匹配模式。
#[derive(Debug, Clone, Default)]
pub struct RelationshipPattern {
    /// 关系类型
    pub rel_type: Option<String>,

    /// 关系属性条件
    pub properties: HashMap<String, PropertyValue>,

    /// Cypher 变量名
    pub variable: Option<String>,

    /// 方向：`true` 表示 `->`（出），`false` 表示 `<-`（入）
    pub outgoing: bool,
}

impl RelationshipPattern {
    pub fn with_type(rel_type: &str) -> Self {
        Self {
            rel_type: Some(rel_type.to_string()),
            outgoing: true,
            ..Default::default()
        }
    }

    pub fn incoming(mut self) -> Self {
        self.outgoing = false;
        self
    }

    pub fn with_variable(mut self, var: &str) -> Self {
        self.variable = Some(var.to_string());
        self
    }
}

/// 图模式 — 节点→关系→节点的路径模式。
///
/// 对应 Cypher 的 `(start)-[rel]->(end)` 片段。
/// 用于构建查询（如"查找所有 Person 节点及其 KNOWS 关系"）。
#[derive(Debug, Clone)]
pub struct GraphPattern {
    /// 起始节点匹配模式
    pub start: NodePattern,

    /// 关系匹配模式
    pub relationship: RelationshipPattern,

    /// 目标节点匹配模式
    pub end: NodePattern,
}

impl GraphPattern {
    /// 创建新的图模式
    pub fn new(start: NodePattern, relationship: RelationshipPattern, end: NodePattern) -> Self {
        Self {
            start,
            relationship,
            end,
        }
    }

    /// 快捷创建：匹配特定标签节点之间的特定类型关系
    pub fn link(start_label: &str, rel_type: &str, end_label: &str) -> Self {
        Self {
            start: NodePattern::with_label(start_label).with_variable("s"),
            relationship: RelationshipPattern::with_type(rel_type).with_variable("r"),
            end: NodePattern::with_label(end_label).with_variable("e"),
        }
    }
}
