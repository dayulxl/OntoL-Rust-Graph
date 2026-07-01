//! 本体模型 ↔ 属性图 双向映射。
//!
//! 将 OWL 本体概念（Class, Property, Individual）编码为属性图节点与关系，
//! 同时支持反向还原。
//! - **Class**       → `(:Class { iri, label, ... })` 节点
//! - **Property**    → `[:HAS_PROPERTY { domain, range, ... }]` 关系
//! - **Individual**  → `(:Individual { iri, type, ... })` 节点

use std::collections::HashMap;

use crate::error::MappingError;
use crate::mapper::graph::node::Node;
use crate::mapper::graph::property::PropertyValue;
use crate::mapper::graph::relationship::Relationship;

// ── 本体模型（最小化定义） ──

/// OWL Class 的简化表示
#[derive(Debug, Clone, PartialEq)]
pub struct Class {
    pub iri: String,
    pub label: Option<String>,
    pub comment: Option<String>,
    pub super_classes: Vec<String>,
}

/// OWL Object Property
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectProperty {
    pub iri: String,
    pub label: Option<String>,
    pub domain: Option<String>,
    pub range: Option<String>,
    pub is_transitive: bool,
    pub is_symmetric: bool,
}

/// OWL Data Property
#[derive(Debug, Clone, PartialEq)]
pub struct DataProperty {
    pub iri: String,
    pub label: Option<String>,
    pub domain: Option<String>,
    pub range_xsd_type: Option<String>,
}

/// OWL Named Individual
#[derive(Debug, Clone, PartialEq)]
pub struct Individual {
    pub iri: String,
    pub label: Option<String>,
    pub class_iris: Vec<String>,
    pub property_values: Vec<(String, PropertyValue)>,
}

// ── 映射函数 ──

/// Class → Node: 标签统一为 `:Class`，IRI 和元数据存入属性
pub fn class_to_node(class: &Class) -> Node {
    let mut props = HashMap::new();
    props.insert("iri".to_string(), PropertyValue::from(class.iri.as_str()));
    if let Some(ref lbl) = class.label {
        props.insert("label".to_string(), PropertyValue::from(lbl.as_str()));
    }
    if let Some(ref c) = class.comment {
        props.insert("comment".to_string(), PropertyValue::from(c.as_str()));
    }

    Node::new(vec!["Class".to_string()], props)
}

/// Individual → Node: 标签为 `:Individual`
pub fn individual_to_node(ind: &Individual) -> Node {
    let mut props = HashMap::new();
    props.insert("iri".to_string(), PropertyValue::from(ind.iri.as_str()));
    if let Some(ref lbl) = ind.label {
        props.insert("label".to_string(), PropertyValue::from(lbl.as_str()));
    }

    Node::new(vec!["Individual".to_string()], props)
}

/// ObjectProperty → Relationship 集合
pub fn object_property_to_relationships(
    prop: &ObjectProperty,
) -> Result<Vec<Relationship>, MappingError> {
    let mut rels = Vec::new();

    let mut base_props = HashMap::new();
    base_props.insert("iri".to_string(), PropertyValue::from(prop.iri.as_str()));
    if prop.is_transitive {
        base_props.insert("transitive".to_string(), PropertyValue::Boolean(true));
    }
    if prop.is_symmetric {
        base_props.insert("symmetric".to_string(), PropertyValue::Boolean(true));
    }

    // domain → HAS_PROPERTY → property_iri
    if let Some(ref domain_iri) = prop.domain {
        rels.push(Relationship::new(
            domain_iri.as_str(),
            "HAS_PROPERTY",
            prop.iri.as_str(),
            base_props.clone(),
        ));
    }

    // property_iri → HAS_RANGE → range_iri
    if let Some(ref range_iri) = prop.range {
        rels.push(Relationship::new(
            prop.iri.as_str(),
            "HAS_RANGE",
            range_iri.as_str(),
            base_props,
        ));
    }

    Ok(rels)
}

/// Individual → 多条关系（`INSTANCE_OF` + 属性值）
pub fn individual_to_relationships(ind: &Individual) -> Vec<Relationship> {
    let mut rels = Vec::new();

    for class_iri in &ind.class_iris {
        let mut props = HashMap::new();
        props.insert("type".to_string(), PropertyValue::from("rdf:type"));
        rels.push(Relationship::new(
            ind.iri.as_str(),
            "INSTANCE_OF",
            class_iri.as_str(),
            props,
        ));
    }

    for (prop_iri, value) in &ind.property_values {
        let mut props = HashMap::new();
        props.insert("value".to_string(), value.clone());
        rels.push(Relationship::new(
            ind.iri.as_str(),
            "HAS_VALUE",
            prop_iri.as_str(),
            props,
        ));
    }

    rels
}
