//! Rust 原生类型 → 属性图属性值 转换。
//!
//! 处理 Rust 类型与图数据库属性类型之间的双向转换，
//! 包括 DateTime、数组、嵌套结构等。

use crate::error::MappingError;
use crate::mapper::graph::property::PropertyValue;

/// 将 Rust 常见类型统一转换为 `PropertyValue`
pub fn string_to_property(s: &str) -> PropertyValue {
    PropertyValue::String(s.to_string())
}

pub fn int_to_property(i: i64) -> PropertyValue {
    PropertyValue::Integer(i)
}

pub fn bool_to_property(b: bool) -> PropertyValue {
    PropertyValue::Boolean(b)
}

/// 将 RFC 3339 / ISO 8601 时间戳字符串转换为属性值
pub fn datetime_to_property(iso_string: &str) -> Result<PropertyValue, MappingError> {
    if iso_string.contains('T') {
        Ok(PropertyValue::String(iso_string.to_string()))
    } else {
        Err(MappingError::PropertyConversion(
            format!("Invalid datetime format, expected ISO 8601: {}", iso_string)
        ))
    }
}

/// 将字符串数组转换为 List 属性值
pub fn string_list_to_property(items: Vec<&str>) -> PropertyValue {
    PropertyValue::List(
        items.into_iter().map(|s| PropertyValue::String(s.to_string())).collect()
    )
}

/// 从属性值提取字符串
pub fn property_to_string(value: &PropertyValue) -> Option<String> {
    value.as_str().map(String::from)
}

/// 从属性值提取 i64
pub fn property_to_i64(value: &PropertyValue) -> Option<i64> {
    value.as_i64()
}

/// 从属性值提取字符串列表
pub fn property_to_string_list(value: &PropertyValue) -> Option<Vec<String>> {
    value.as_list().map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_conversion() {
        let val = string_to_property("hello");
        assert_eq!(val.as_str(), Some("hello"));
    }

    #[test]
    fn datetime_valid() {
        let result = datetime_to_property("2024-01-01T00:00:00Z");
        assert!(result.is_ok());
    }

    #[test]
    fn datetime_invalid() {
        let result = datetime_to_property("2024-01-01 00:00:00");
        assert!(result.is_err());
    }

    #[test]
    fn list_conversion() {
        let val = string_list_to_property(vec!["a", "b", "c"]);
        let extracted = property_to_string_list(&val);
        assert_eq!(extracted, Some(vec!["a".to_string(), "b".to_string(), "c".to_string()]));
    }
}
