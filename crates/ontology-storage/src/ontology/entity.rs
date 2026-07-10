//! Entity / Type / Patrol 节点创建。
//!
//! 每个创建函数执行：JSON 解析 → 属性校验 → Node 构建 → 写入存储。

use std::collections::HashMap;

use crate::mapper::graph::node::Node;
use crate::mapper::graph::property::PropertyValue;
use crate::mapper::graph::relationship::Relationship;
use crate::mapper::unified_mapping;
use crate::repository::graph_store::GraphRepository;

// ═══════════════════════════════════════════════════════════
// JSON 转换辅助
// ═══════════════════════════════════════════════════════════

/// 将 JSON 值转换为 PropertyValue
pub fn json_to_property(v: &serde_json::Value) -> PropertyValue {
    match v {
        serde_json::Value::String(s) => PropertyValue::from(s.as_str()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                PropertyValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                PropertyValue::Float(f)
            } else {
                PropertyValue::Null
            }
        }
        serde_json::Value::Bool(b) => PropertyValue::Boolean(*b),
        serde_json::Value::Array(arr) => {
            let items: Vec<PropertyValue> = arr.iter().map(json_to_property).collect();
            PropertyValue::List(items)
        }
        serde_json::Value::Object(map) => {
            let m: HashMap<String, PropertyValue> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_property(v)))
                .collect();
            PropertyValue::Map(m)
        }
        serde_json::Value::Null => PropertyValue::Null,
    }
}

pub fn get_str<'a>(params: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(|v| v.as_str())
}

pub fn get_i64(params: &serde_json::Value, key: &str) -> Option<i64> {
    params.get(key).and_then(|v| v.as_i64())
}

pub fn get_f64(params: &serde_json::Value, key: &str) -> Option<f64> {
    params.get(key).and_then(|v| v.as_f64())
}

pub fn set_opt_str(
    props: &mut HashMap<String, PropertyValue>,
    params: &serde_json::Value,
    key: &str,
) {
    if let Some(v) = get_str(params, key) {
        props.insert(key.to_string(), PropertyValue::from(v));
    }
}

pub fn set_opt_i64(
    props: &mut HashMap<String, PropertyValue>,
    params: &serde_json::Value,
    key: &str,
) {
    if let Some(v) = get_i64(params, key) {
        props.insert(key.to_string(), PropertyValue::Integer(v));
    }
}

pub fn set_opt_f64(
    props: &mut HashMap<String, PropertyValue>,
    params: &serde_json::Value,
    key: &str,
) {
    if let Some(v) = get_f64(params, key) {
        props.insert(key.to_string(), PropertyValue::Float(v));
    }
}

// ═══════════════════════════════════════════════════════════
// 存在性检查
// ═══════════════════════════════════════════════════════════

fn code_exists(repo: &dyn GraphRepository, code: &str) -> bool {
    repo.get_nodes_by_label(unified_mapping::ENTITY_LABEL)
        .unwrap_or_default()
        .iter()
        .any(|n| n.property("code").and_then(|v| v.as_str()) == Some(code))
}

fn type_name_exists(repo: &dyn GraphRepository, name: &str) -> bool {
    repo.get_nodes_by_label(unified_mapping::TYPE_LABEL)
        .unwrap_or_default()
        .iter()
        .any(|n| n.property("name").and_then(|v| v.as_str()) == Some(name))
}

// ═══════════════════════════════════════════════════════════
// Entity
// ═══════════════════════════════════════════════════════════

/// 从 JSON 参数构建 Entity 节点属性表（不做 I/O）。
/// 覆盖架构定义的 30 个标准字段 + 额外自定义属性。
pub fn build_entity_properties(
    params: &serde_json::Value,
) -> Result<HashMap<String, PropertyValue>, String> {
    let code = get_str(params, "code").ok_or("'code' is required")?;

    let mut props = HashMap::new();
    props.insert("code".to_string(), PropertyValue::from(code));

    // 基础字段
    set_opt_str(&mut props, params, "id");
    set_opt_str(&mut props, params, "unit_id");
    set_opt_str(&mut props, params, "graph_id");
    set_opt_str(&mut props, params, "domain");
    set_opt_i64(&mut props, params, "leven");
    set_opt_str(&mut props, params, "name");
    set_opt_str(&mut props, params, "type");
    set_opt_str(&mut props, params, "update_time");
    set_opt_str(&mut props, params, "create_time");
    set_opt_f64(&mut props, params, "confidence");
    set_opt_str(&mut props, params, "static_rule_id");
    set_opt_str(&mut props, params, "dynamic_rule_id");
    set_opt_f64(&mut props, params, "speed");
    set_opt_f64(&mut props, params, "power");
    set_opt_str(&mut props, params, "description");
    set_opt_str(&mut props, params, "status");
    set_opt_str(&mut props, params, "version");
    set_opt_str(&mut props, params, "cope_version");
    set_opt_str(&mut props, params, "source");
    set_opt_str(&mut props, params, "owner");
    set_opt_str(&mut props, params, "parent_id");

    // 行为字段 (OWL2 风格属性名，全部 String 类型)
    set_opt_str(&mut props, params, unified_mapping::HAS_PRECONDITION_KEY);
    set_opt_str(&mut props, params, unified_mapping::HAS_EFFECT_KEY);
    set_opt_str(&mut props, params, unified_mapping::HAS_COST_KEY);
    set_opt_str(&mut props, params, unified_mapping::HAS_DURATION_KEY);
    set_opt_str(&mut props, params, unified_mapping::HAS_PRIORITY_KEY);
    set_opt_str(&mut props, params, unified_mapping::COMPOSED_OF_KEY);

    // 空间字段
    if let Some(arr) = params.get("Space_abs").and_then(|v| v.as_array()) {
        let list: Vec<PropertyValue> = arr.iter().map(json_to_property).collect();
        props.insert("Space_abs".to_string(), PropertyValue::List(list));
    }

    // 边属性
    set_opt_i64(&mut props, params, "command_side");

    // 额外自定义属性
    let known_keys: std::collections::HashSet<&str> = [
        "code",
        "id",
        "unit_id",
        "graph_id",
        "domain",
        "leven",
        "name",
        "type",
        "update_time",
        "create_time",
        "confidence",
        "static_rule_id",
        "dynamic_rule_id",
        "speed",
        "power",
        "description",
        "status",
        "version",
        "cope_version",
        "source",
        "owner",
        "parent_id",
        "hasPrecondition",
        "hasEffect",
        "hasCost",
        "hasDuration",
        "hasPriority",
        "composedOf",
        "Space_abs",
        "command_side",
    ]
    .iter()
    .copied()
    .collect();

    if let Some(obj) = params.as_object() {
        for (k, v) in obj {
            if !known_keys.contains(k.as_str()) {
                props.insert(k.clone(), json_to_property(v));
            }
        }
    }

    Ok(props)
}

/// 创建 Entity 节点。
///
/// 执行 code 唯一性校验 → 构建属性 → 写入存储。
/// 如未提供 `id`，自动生成唯一技术标识符。
pub fn create_entity(
    repo: &dyn GraphRepository,
    params: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let code = get_str(params, "code").ok_or("'code' is required for create_entity")?;

    if code_exists(repo, code) {
        return Err(format!("Entity with code '{}' already exists", code));
    }

    // 确保 id 存在（未提供时自动生成）
    let mut params = params.clone();
    if get_str(&params, "id").is_none()
        && let Some(obj) = params.as_object_mut()
    {
        obj.insert(
            "id".to_string(),
            serde_json::Value::String(generate_entity_id(code)),
        );
    }

    let props = build_entity_properties(&params)?;
    let node = Node::new(vec![unified_mapping::ENTITY_LABEL.to_string()], props);
    let node_id = repo
        .insert_node(&node)
        .map_err(|e| format!("Insert failed: {}", e))?;

    Ok(serde_json::json!({
        "action": "create_entity",
        "code": code,
        "node_id": node_id,
    }))
}

/// 为实体生成唯一技术标识符。
fn generate_entity_id(code: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("urn:entity:{}-{}", code, ts)
}

// ═══════════════════════════════════════════════════════════
// Type
// ═══════════════════════════════════════════════════════════

/// 创建 Type 节点，可选创建 subClassOf 关系到父类型。
pub fn create_type(
    repo: &dyn GraphRepository,
    params: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let name = get_str(params, "name").ok_or("'name' is required for create_type")?;

    if type_name_exists(repo, name) {
        return Err(format!("Type '{}' already exists", name));
    }

    let mut props = HashMap::new();
    props.insert("name".to_string(), PropertyValue::from(name));
    if let Some(desc) = get_str(params, "description") {
        props.insert("description".to_string(), PropertyValue::from(desc));
    }
    if let Some(domain) = get_str(params, "domain") {
        props.insert("domain".to_string(), PropertyValue::from(domain));
    }
    set_opt_i64(&mut props, params, "leven");
    set_opt_str(&mut props, params, "code");

    let type_node = Node::new(vec![unified_mapping::TYPE_LABEL.to_string()], props);
    let type_id = repo
        .insert_node(&type_node)
        .map_err(|e| format!("Insert failed: {}", e))?;

    // subClassOf
    let mut parent_info: Option<serde_json::Value> = None;
    if let Some(parent_name) = get_str(params, "parent_type") {
        if type_name_exists(repo, parent_name) {
            let rel = Relationship::simple(
                format!("type_{}", name),
                unified_mapping::SUB_CLASS_OF_REL,
                format!("type_{}", parent_name),
            );
            repo.insert_relationship(&rel)
                .map_err(|e| format!("subClassOf failed: {}", e))?;
            parent_info = Some(serde_json::json!({"parent_type": parent_name}));
        } else {
            return Err(format!("Parent type '{}' not found", parent_name));
        }
    }

    Ok(serde_json::json!({
        "action": "create_type",
        "name": name,
        "node_id": type_id,
        "parent": parent_info,
    }))
}

// ═══════════════════════════════════════════════════════════
// Patrol
// ═══════════════════════════════════════════════════════════

/// 创建 Patrol 巡逻任务节点。
pub fn create_patrol(
    repo: &dyn GraphRepository,
    params: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let name = get_str(params, "name").ok_or("'name' is required for create_patrol")?;
    let code = get_str(params, "code").unwrap_or(name);

    let mut props = HashMap::new();
    props.insert("code".to_string(), PropertyValue::from(code));
    props.insert("name".to_string(), PropertyValue::from(name));

    set_opt_str(&mut props, params, "description");
    set_opt_str(&mut props, params, "status");
    set_opt_str(&mut props, params, "domain");
    set_opt_i64(&mut props, params, "command_side");
    set_opt_i64(&mut props, params, "duration");
    set_opt_f64(&mut props, params, "confidence");

    if let Some(arr) = params.get("path").and_then(|v| v.as_array()) {
        let list: Vec<PropertyValue> = arr.iter().map(json_to_property).collect();
        props.insert("path".to_string(), PropertyValue::List(list));
    }

    if let Some(arr) = params.get("entities").and_then(|v| v.as_array()) {
        let list: Vec<PropertyValue> = arr.iter().map(json_to_property).collect();
        props.insert("entities".to_string(), PropertyValue::List(list));
    }

    // 额外自定义属性
    let known_keys: std::collections::HashSet<&str> = [
        "code",
        "name",
        "description",
        "status",
        "domain",
        "command_side",
        "duration",
        "confidence",
        "path",
        "entities",
    ]
    .iter()
    .copied()
    .collect();

    if let Some(obj) = params.as_object() {
        for (k, v) in obj {
            if !known_keys.contains(k.as_str()) {
                props.insert(k.clone(), json_to_property(v));
            }
        }
    }

    let node = Node::new(vec![unified_mapping::PATROL_LABEL.to_string()], props);
    let node_id = repo
        .insert_node(&node)
        .map_err(|e| format!("Insert failed: {}", e))?;

    Ok(serde_json::json!({
        "action": "create_patrol",
        "name": name,
        "code": code,
        "node_id": node_id,
    }))
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_to_property_string() {
        let v = serde_json::Value::String("hello".into());
        assert_eq!(json_to_property(&v), PropertyValue::from("hello"));
    }

    #[test]
    fn json_to_property_integer() {
        let v = serde_json::Value::Number(serde_json::Number::from(42));
        assert_eq!(json_to_property(&v), PropertyValue::Integer(42));
    }

    #[test]
    fn json_to_property_float() {
        let v = serde_json::Value::Number(serde_json::Number::from_f64(3.14).unwrap());
        assert_eq!(json_to_property(&v), PropertyValue::Float(3.14));
    }

    #[test]
    fn json_to_property_bool() {
        assert_eq!(
            json_to_property(&serde_json::Value::Bool(true)),
            PropertyValue::Boolean(true)
        );
    }

    #[test]
    fn json_to_property_array() {
        let arr = serde_json::json!(["a", "b"]);
        let result = json_to_property(&arr);
        match result {
            PropertyValue::List(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], PropertyValue::from("a"));
            }
            _ => panic!("Expected List"),
        }
    }

    #[test]
    fn json_to_property_null() {
        assert_eq!(
            json_to_property(&serde_json::Value::Null),
            PropertyValue::Null
        );
    }

    #[test]
    fn build_entity_props_with_required_fields() {
        let params = serde_json::json!({
            "code": "TEST_001",
            "name": "测试实体",
            "type": "测试类型",
            "command_side": 0
        });
        let props = build_entity_properties(&params).unwrap();
        assert_eq!(props.get("code").and_then(|v| v.as_str()), Some("TEST_001"));
        assert_eq!(props.get("name").and_then(|v| v.as_str()), Some("测试实体"));
        assert_eq!(props.get("type").and_then(|v| v.as_str()), Some("测试类型"));
    }

    #[test]
    fn build_entity_props_requires_code() {
        let params = serde_json::json!({"name": "no code"});
        assert!(build_entity_properties(&params).is_err());
    }

    #[test]
    fn build_entity_props_space_abs() {
        let params = serde_json::json!({
            "code": "TEST_002",
            "Space_abs": [23.1291, 12.8, -30.0, 0.0]
        });
        let props = build_entity_properties(&params).unwrap();
        assert!(props.contains_key("Space_abs"));
    }
}
