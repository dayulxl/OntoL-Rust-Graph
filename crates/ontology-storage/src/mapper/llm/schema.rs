//! 本体概念 → JSON Schema 自动生成。
//!
//! 将 `model_mapper` 中定义的本体模型（Class, Property, Individual）
//! 自动转换为 JSON Schema，供 LLM structured output 使用。

use std::collections::HashMap;

use crate::error::MappingError;
use crate::mapper::graph::property::PropertyValue;
use crate::mapper::llm::tool::{JsonSchema, PropertySchema, ToolDefinition};
use crate::mapper::model_mapper::{Class, DataProperty, Individual, ObjectProperty};

// ── Class → JSON Schema ──

impl From<&Class> for JsonSchema {
    fn from(class: &Class) -> Self {
        let mut schema = JsonSchema::new()
            .with_required(
                "iri",
                PropertySchema::scalar("string", "类的唯一 IRI 标识符"),
            )
            .with_optional(
                "label",
                PropertySchema::scalar("string", "人类可读的类标签"),
            );

        if !class.super_classes.is_empty() {
            let super_class_schema = PropertySchema::array(
                "父类 IRI 列表",
                PropertySchema::scalar("string", "父类 IRI"),
            );
            schema = schema.with_required("super_classes", super_class_schema);
        }

        schema
    }
}

// ── Individual → JSON Schema ──

impl From<&Individual> for JsonSchema {
    fn from(ind: &Individual) -> Self {
        JsonSchema::new()
            .with_required("iri", PropertySchema::scalar("string", "个体的唯一 IRI"))
            .with_optional("label", PropertySchema::scalar("string", "个体的显示标签"))
            .with_required(
                "class_iris",
                PropertySchema::array(
                    "该个体所属的类 IRI 列表",
                    PropertySchema::scalar("string", "类 IRI"),
                ),
            )
    }
}

// ── ObjectProperty → JSON Schema ──

impl From<&ObjectProperty> for JsonSchema {
    fn from(prop: &ObjectProperty) -> Self {
        JsonSchema::new()
            .with_required("iri", PropertySchema::scalar("string", "属性的唯一 IRI"))
            .with_optional("label", PropertySchema::scalar("string", "属性的显示标签"))
            .with_optional("domain", PropertySchema::scalar("string", "定义域类 IRI"))
            .with_optional("range", PropertySchema::scalar("string", "值域类 IRI"))
            .with_optional(
                "is_transitive",
                PropertySchema::scalar("boolean", "是否可传递"),
            )
            .with_optional(
                "is_symmetric",
                PropertySchema::scalar("boolean", "是否对称"),
            )
    }
}

// ── DataProperty → JSON Schema ──

impl From<&DataProperty> for JsonSchema {
    fn from(prop: &DataProperty) -> Self {
        JsonSchema::new()
            .with_required(
                "iri",
                PropertySchema::scalar("string", "数据属性的唯一 IRI"),
            )
            .with_optional(
                "label",
                PropertySchema::scalar("string", "数据属性的显示标签"),
            )
            .with_optional("domain", PropertySchema::scalar("string", "定义域类 IRI"))
            .with_optional(
                "range_xsd_type",
                PropertySchema::scalar("string", "值域的 XSD 类型（如 xsd:string）"),
            )
    }
}

// ── PropertyValue → JSON Schema ──

/// 将 PropertyValue 的类型映射为 JSON Schema 类型字符串
fn property_value_to_json_type(value: &PropertyValue) -> &'static str {
    match value {
        PropertyValue::String(_) => "string",
        PropertyValue::Integer(_) => "integer",
        PropertyValue::Float(_) => "number",
        PropertyValue::Boolean(_) => "boolean",
        PropertyValue::List(_) => "array",
        PropertyValue::Map(_) => "object",
        PropertyValue::Null => "null",
    }
}

/// 从一组 (name, PropertyValue) 对推导 JSON Schema
pub fn property_values_to_schema(
    values: &[(String, PropertyValue)],
) -> Result<JsonSchema, MappingError> {
    let mut schema = JsonSchema::new();
    for (name, value) in values {
        let json_type = property_value_to_json_type(value);
        let prop_schema = PropertySchema::scalar(json_type, &format!("属性 {}", name));

        // 对于 List 类型，尝试推导元素类型
        let prop_schema = match value {
            PropertyValue::List(items) if !items.is_empty() => {
                let inner_type = property_value_to_json_type(&items[0]);
                PropertySchema::array(
                    &format!("属性 {} 的列表值", name),
                    PropertySchema::scalar(inner_type, &format!("{} 的元素", name)),
                )
            }
            _ => prop_schema,
        };

        schema = schema.with_required(name, prop_schema);
    }
    Ok(schema)
}

// ── 业务工具集 ──

/// 获取本体查询相关的工具定义集合
///
/// 将 GraphRepository 的核心能力暴露为 LLM Function Calling 工具，
/// 使 LLM 可以通过结构化 JSON 调用本体查询操作。
pub fn ontology_query_tools() -> Vec<ToolDefinition> {
    vec![
        // 1. 按类查询个体
        ToolDefinition::new(
            "query_individuals_by_class",
            "根据本体类的 IRI 查询所有属于该类的个体（实例）。例如：查询所有 'Person' 类的实例。",
            JsonSchema::new()
                .with_required(
                    "class_iri",
                    PropertySchema::scalar("string", "目标类的 IRI，如 ex:Person"),
                )
                .with_optional(
                    "limit",
                    PropertySchema::scalar("integer", "返回结果的最大数量，默认 50"),
                ),
        ),
        // 2. 查询节点属性
        ToolDefinition::new(
            "get_entity_properties",
            "获取指定实体（节点）的所有属性，包括 IRI、标签和自定义属性。",
            JsonSchema::new()
                .with_required(
                    "entity_iri",
                    PropertySchema::scalar("string", "目标实体的 IRI 标识符"),
                )
                .with_optional(
                    "property_names",
                    PropertySchema::array(
                        "要返回的特定属性名列表（空则返回全部）",
                        PropertySchema::scalar("string", "属性名称"),
                    ),
                ),
        ),
        // 3. 查询关系
        ToolDefinition::new(
            "query_relationships",
            "查询指定节点的出方向关系。可以按关系类型过滤。",
            JsonSchema::new()
                .with_required("node_iri", PropertySchema::scalar("string", "源节点的 IRI"))
                .with_optional(
                    "rel_type",
                    PropertySchema::enumerated(
                        "string",
                        "关系类型过滤",
                        vec![
                            "INSTANCE_OF".into(),
                            "HAS_PROPERTY".into(),
                            "HAS_VALUE".into(),
                            "HAS_RANGE".into(),
                        ],
                    ),
                ),
        ),
        // 4. 图模式查询
        ToolDefinition::new(
            "query_pattern",
            "执行图模式匹配查询。根据节点标签和关系类型模式搜索子图。",
            JsonSchema::new()
                .with_required(
                    "node_label",
                    PropertySchema::enumerated(
                        "string",
                        "节点标签",
                        vec!["Class".into(), "Individual".into()],
                    ),
                )
                .with_optional(
                    "filters",
                    PropertySchema::object(
                        "节点属性过滤条件，键为属性名，值为匹配值",
                        {
                            let mut props = HashMap::new();
                            props.insert(
                                "iri".to_string(),
                                PropertySchema::scalar("string", "IRI 过滤（支持前缀匹配）"),
                            );
                            props.insert(
                                "label".to_string(),
                                PropertySchema::scalar("string", "标签过滤（支持模糊匹配）"),
                            );
                            props
                        },
                        vec![],
                    ),
                ),
        ),
    ]
}
