use std::collections::HashMap;

use crate::mapper::graph::property::PropertyValue;

/// 属性图节点。
///
/// 对应 Neo4j 中的 `(n:Label1:Label2 { key: value, ... })`。
/// 使用 `labels` 向量表示多标签（Neo4j 原生支持）。
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// 节点标签列表（如 `["Class", "Deprecated"]`）
    pub labels: Vec<String>,

    /// 属性键值对
    pub properties: HashMap<String, PropertyValue>,
}

impl Node {
    /// 创建新节点
    pub fn new(labels: Vec<String>, properties: HashMap<String, PropertyValue>) -> Self {
        Self { labels, properties }
    }

    /// 创建只有单个标签的空节点
    pub fn with_label(label: &str) -> Self {
        Self {
            labels: vec![label.to_string()],
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

    /// 检查是否具有某个标签
    pub fn has_label(&self, label: &str) -> bool {
        self.labels.iter().any(|l| l == label)
    }
}
