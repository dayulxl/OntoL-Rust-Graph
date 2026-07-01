//! Cypher 参数绑定 — 防止注入 + 性能优化。
//!
//! Neo4j 支持 `$param` 参数化查询，类似 SQL 的 prepared statements。
//! 将参数提取到参数映射中，避免字符串拼接导致的注入风险，
//! 同时允许 Neo4j 复用查询计划。

use std::collections::HashMap;
use serde_json::Value as JsonValue;

use crate::mapper::graph::property::PropertyValue;

/// Cypher 参数映射：`{ "name": value, ... }`
pub type CypherParams = HashMap<String, JsonValue>;

/// PropertyValue → serde_json::Value 转换
fn property_to_json(value: &PropertyValue) -> JsonValue {
    match value {
        PropertyValue::String(s) => JsonValue::String(s.clone()),
        PropertyValue::Integer(i) => JsonValue::Number((*i).into()),
        PropertyValue::Float(f) => {
            serde_json::Number::from_f64(*f)
                .map(JsonValue::Number)
                .unwrap_or(JsonValue::Null)
        }
        PropertyValue::Boolean(b) => JsonValue::Bool(*b),
        PropertyValue::List(v) => JsonValue::Array(v.iter().map(property_to_json).collect()),
        PropertyValue::Map(m) => {
            let map: serde_json::Map<String, JsonValue> = m
                .iter()
                .map(|(k, v)| (k.clone(), property_to_json(v)))
                .collect();
            JsonValue::Object(map)
        }
        PropertyValue::Null => JsonValue::Null,
    }
}

/// 参数提取器 — 从模式中收集所有属性值
#[derive(Debug, Default)]
pub struct ParamCollector {
    pub params: CypherParams,
}

impl ParamCollector {
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
        }
    }

    /// 为节点模式提取参数（参数名格式：`{var}_{idx}`）
    pub fn collect_from_node(
        &mut self,
        pattern: &crate::mapper::graph::pattern::NodePattern,
    ) {
        let var = pattern.variable.as_deref().unwrap_or("n");
        for (i, (key, value)) in pattern.properties.iter().enumerate() {
            let param_name = format!("{}_{}", var, i);
            self.params.insert(param_name.clone(), property_to_json(value));
            self.params.insert(format!("{}__key", param_name), JsonValue::String(key.clone()));
        }
    }

    /// 为关系模式提取参数
    pub fn collect_from_rel(
        &mut self,
        pattern: &crate::mapper::graph::pattern::RelationshipPattern,
    ) {
        let var = pattern.variable.as_deref().unwrap_or("r");
        for (i, (key, value)) in pattern.properties.iter().enumerate() {
            let param_name = format!("{}_{}", var, i);
            self.params.insert(param_name.clone(), property_to_json(value));
            self.params.insert(format!("{}__key", param_name), JsonValue::String(key.clone()));
        }
    }

    /// 为完整图模式提取所有参数
    pub fn collect_from_pattern(
        &mut self,
        pattern: &crate::mapper::graph::pattern::GraphPattern,
    ) {
        self.collect_from_node(&pattern.start);
        self.collect_from_rel(&pattern.relationship);
        self.collect_from_node(&pattern.end);
    }

    /// 返回所有收集的参数
    pub fn into_params(self) -> CypherParams {
        self.params
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::graph::pattern::NodePattern;
    use crate::mapper::graph::property::PropertyValue;

    #[test]
    fn collects_node_params() {
        let np = NodePattern::with_label("Class")
            .with_variable("n")
            .with_property("iri", PropertyValue::from("http://example.org#Foo"));
        let mut collector = ParamCollector::new();
        collector.collect_from_node(&np);
        let expected = serde_json::Value::String("http://example.org#Foo".to_string());
        assert_eq!(collector.params.get("n_0"), Some(&expected));
    }
}
