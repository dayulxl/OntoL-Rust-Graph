//! 工具/函数调用定义 — 与 LLM Function Calling 对齐的通用模型。
//!
//! 不绑定任何特定 LLM 厂商（OpenAI / Anthropic），
//! 提供可序列化为 JSON 的通用 ToolDefinition 结构。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ── JSON Schema 子结构 ──

/// JSON Schema 属性定义
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PropertySchema {
    /// 类型："string" | "number" | "integer" | "boolean" | "array" | "object" | "null"
    #[serde(rename = "type")]
    pub r#type: String,

    /// 属性说明
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// 枚举约束（仅当 type = "string" 时常用）
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,

    /// 数组元素类型（仅当 type = "array" 时）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<PropertySchema>>,

    /// 嵌套对象属性（仅当 type = "object" 时）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, PropertySchema>>,

    /// 嵌套对象的必填字段
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

impl PropertySchema {
    /// 创建简单标量属性
    pub fn scalar(r#type: &str, description: &str) -> Self {
        PropertySchema {
            r#type: r#type.to_string(),
            description: Some(description.to_string()),
            enum_values: None,
            items: None,
            properties: None,
            required: None,
        }
    }

    /// 创建枚举属性
    pub fn enumerated(r#type: &str, description: &str, values: Vec<String>) -> Self {
        PropertySchema {
            r#type: r#type.to_string(),
            description: Some(description.to_string()),
            enum_values: Some(values),
            items: None,
            properties: None,
            required: None,
        }
    }

    /// 创建数组属性
    pub fn array(description: &str, item_schema: PropertySchema) -> Self {
        PropertySchema {
            r#type: "array".to_string(),
            description: Some(description.to_string()),
            enum_values: None,
            items: Some(Box::new(item_schema)),
            properties: None,
            required: None,
        }
    }

    /// 创建嵌套对象属性
    pub fn object(
        description: &str,
        properties: HashMap<String, PropertySchema>,
        required: Vec<String>,
    ) -> Self {
        PropertySchema {
            r#type: "object".to_string(),
            description: Some(description.to_string()),
            enum_values: None,
            items: None,
            properties: Some(properties),
            required: Some(required),
        }
    }
}

// ── JSON Schema 顶层结构 ──

/// JSON Schema 对象（用于 function parameters 定义）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonSchema {
    /// 固定为 "object"
    #[serde(rename = "type")]
    pub r#type: String,

    /// 顶层属性映射
    pub properties: HashMap<String, PropertySchema>,

    /// 必填字段列表
    pub required: Vec<String>,

    /// 禁止额外属性（严格模式）
    #[serde(default = "default_additional_properties")]
    pub additional_properties: bool,
}

fn default_additional_properties() -> bool {
    false
}

impl JsonSchema {
    /// 创建一个空的 object schema
    pub fn new() -> Self {
        JsonSchema {
            r#type: "object".to_string(),
            properties: HashMap::new(),
            required: Vec::new(),
            additional_properties: false,
        }
    }

    /// 添加一个必填属性
    pub fn with_required(mut self, name: &str, schema: PropertySchema) -> Self {
        self.properties.insert(name.to_string(), schema);
        self.required.push(name.to_string());
        self
    }

    /// 添加一个可选属性
    pub fn with_optional(mut self, name: &str, schema: PropertySchema) -> Self {
        self.properties.insert(name.to_string(), schema);
        self
    }
}

impl Default for JsonSchema {
    fn default() -> Self {
        Self::new()
    }
}

// ── 工具定义 ──

/// 工具/函数定义（OpenAI Function Calling / Anthropic Tool Use 通用格式）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    /// 工具名称（LLM 通过此名称决定调用哪个函数）
    pub name: String,

    /// 工具用途描述（LLM 据此判断何时使用）
    pub description: String,

    /// 参数 JSON Schema
    pub parameters: JsonSchema,
}

impl ToolDefinition {
    /// 创建新的工具定义
    pub fn new(name: &str, description: &str, parameters: JsonSchema) -> Self {
        ToolDefinition {
            name: name.to_string(),
            description: description.to_string(),
            parameters,
        }
    }
}

// ── 工具调用结果 ──

/// LLM 返回的工具调用
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    /// 调用 ID（用于关联响应）
    pub id: String,

    /// 被调用的工具名称
    pub name: String,

    /// 调用参数（JSON 对象）
    pub arguments: serde_json::Value,
}

impl ToolCall {
    /// 尝试将 arguments 解析为强类型
    pub fn parse_arguments<T: for<'de> serde::Deserialize<'de>>(
        &self,
    ) -> Result<T, serde_json::Error> {
        serde_json::from_value(self.arguments.clone())
    }
}

// ── 批量工具定义 ──

/// 工具集合，便于一次注册多个工具
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolSet {
    /// 注册的工具列表
    pub tools: Vec<ToolDefinition>,
}

impl ToolSet {
    pub fn new(tools: Vec<ToolDefinition>) -> Self {
        ToolSet { tools }
    }

    /// 按名称查找工具定义
    pub fn find(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.iter().find(|t| t.name == name)
    }
}

impl From<Vec<ToolDefinition>> for ToolSet {
    fn from(tools: Vec<ToolDefinition>) -> Self {
        ToolSet::new(tools)
    }
}

// ── 测试 ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_query_tool_definition() {
        let params = JsonSchema::new()
            .with_required("class_iri", PropertySchema::scalar("string", "类的 IRI 标识符"))
            .with_optional("limit", PropertySchema::scalar("integer", "返回结果的最大数量"));

        let tool = ToolDefinition::new(
            "query_individuals_by_class",
            "根据类 IRI 查询所有属于该类的个体",
            params,
        );

        let json = serde_json::to_string_pretty(&tool).unwrap();
        assert!(json.contains("query_individuals_by_class"));
        assert!(json.contains("class_iri"));
    }

    #[test]
    fn property_schema_array() {
        let item = PropertySchema::scalar("string", "个体 IRI");
        let arr = PropertySchema::array("个体 IRI 列表", item);

        assert_eq!(arr.r#type, "array");
        assert!(arr.items.is_some());
    }

    #[test]
    fn tool_call_parse_arguments() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Args {
            class_iri: String,
            limit: Option<i64>,
        }

        let call = ToolCall {
            id: "call_001".into(),
            name: "query_individuals_by_class".into(),
            arguments: serde_json::json!({"class_iri": "ex:Person", "limit": 10}),
        };

        let args: Args = call.parse_arguments().unwrap();
        assert_eq!(args.class_iri, "ex:Person");
        assert_eq!(args.limit, Some(10));
    }
}
