//! 属性图属性值类型。
//!
//! 与存储无关的通用属性值枚举，替代 `serde_json::Value`，
//! 保持零外部依赖。

use std::collections::HashMap;

/// 属性值类型 — 覆盖属性图常见数据类型
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    List(Vec<PropertyValue>),
    Map(HashMap<String, PropertyValue>),
    Null,
}

impl PropertyValue {
    /// 提取字符串值
    pub fn as_str(&self) -> Option<&str> {
        match self {
            PropertyValue::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// 提取 i64 值
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            PropertyValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// 提取布尔值
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            PropertyValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// 提取数组（若为 List 类型）
    pub fn as_list(&self) -> Option<&Vec<PropertyValue>> {
        match self {
            PropertyValue::List(v) => Some(v),
            _ => None,
        }
    }
}

// ── 便利转换 impl ──

impl From<&str> for PropertyValue {
    fn from(s: &str) -> Self {
        PropertyValue::String(s.to_string())
    }
}

impl From<String> for PropertyValue {
    fn from(s: String) -> Self {
        PropertyValue::String(s)
    }
}

impl From<i64> for PropertyValue {
    fn from(i: i64) -> Self {
        PropertyValue::Integer(i)
    }
}

impl From<bool> for PropertyValue {
    fn from(b: bool) -> Self {
        PropertyValue::Boolean(b)
    }
}

impl From<f64> for PropertyValue {
    fn from(f: f64) -> Self {
        PropertyValue::Float(f)
    }
}

impl From<Vec<PropertyValue>> for PropertyValue {
    fn from(v: Vec<PropertyValue>) -> Self {
        PropertyValue::List(v)
    }
}
