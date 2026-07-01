use std::collections::HashMap;

use crate::mapper::graph::property::PropertyValue;

/// 属性图关系（有向边）。
///
/// 对应 Neo4j 中的 `(a)-[:TYPE { key: value }]->(b)`。
/// `start_node_id` 和 `end_node_id` 为节点的业务 ID（IRI）。
#[derive(Debug, Clone, PartialEq)]
pub struct Relationship {
    /// 关系类型（如 `"INSTANCE_OF"`, `"HAS_PROPERTY"`）
    pub rel_type: String,

    /// 起始节点 ID（业务标识符，对应 Node 的 iri 属性）
    pub start_node_id: String,

    /// 目标节点 ID
    pub end_node_id: String,

    /// 关系属性键值对
    pub properties: HashMap<String, PropertyValue>,
}

impl Relationship {
    /// 创建新关系
    pub fn new(
        start_node_id: impl Into<String>,
        rel_type: impl Into<String>,
        end_node_id: impl Into<String>,
        properties: HashMap<String, PropertyValue>,
    ) -> Self {
        Self {
            start_node_id: start_node_id.into(),
            rel_type: rel_type.into(),
            end_node_id: end_node_id.into(),
            properties,
        }
    }

    /// 创建无属性的简单关系
    pub fn simple(
        start: impl Into<String>,
        rel_type: impl Into<String>,
        end: impl Into<String>,
    ) -> Self {
        Self {
            start_node_id: start.into(),
            rel_type: rel_type.into(),
            end_node_id: end.into(),
            properties: HashMap::new(),
        }
    }

    /// 获取指定属性值
    pub fn property(&self, key: &str) -> Option<&PropertyValue> {
        self.properties.get(key)
    }

    /// 设置属性值
    pub fn set_property(&mut self, key: &str, value: PropertyValue) {
        self.properties.insert(key.to_string(), value);
    }
}
