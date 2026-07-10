//! POST /entity/update — 修改 Entity 属性值。
//!
//! ```json
//! { "id": "P8A_001", "updates": { "status": "无效", "confidence": 0.95 } }
//! ```
//!
//! 可选 `cope_version`：如果实体 cope_version 为空（原实体），自动克隆副本再修改。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ontology_reasoner::update_entity_properties;
use ontology_storage::PropertyValue;

use super::super::server::json_error;
use crate::app::AppState;

pub fn handle(request: &mut tiny_http::Request, state: &Arc<Mutex<AppState>>) -> (u16, String) {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        return (400, json_error("Failed to read body".into()));
    }

    let parsed: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => return (400, json_error(format!("Invalid JSON: {}", e))),
    };

    let id = match parsed.get("id").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return (400, json_error("Missing 'id'".into())),
    };

    let updates_json = match parsed.get("updates").and_then(|v| v.as_object()) {
        Some(obj) => obj,
        None => return (400, json_error("Missing 'updates' object".into())),
    };

    if updates_json.is_empty() {
        return (400, json_error("'updates' must not be empty".into()));
    }

    let cope_version = parsed.get("cope_version").and_then(|v| v.as_str());

    let app = match state.lock() {
        Ok(a) => a,
        Err(e) => return (500, json_error(format!("Lock: {}", e))),
    };

    let mut updates = HashMap::new();
    for (k, v) in updates_json {
        updates.insert(k.clone(), json_to_prop(v));
    }

    match update_entity_properties(app.repo.as_ref(), id, updates, cope_version) {
        Ok(updated) => {
            let props: serde_json::Map<_, _> = updated
                .properties
                .iter()
                .map(|(k, v)| (k.clone(), prop_to_json(v)))
                .collect();
            let resp = serde_json::json!({
                "ok": true,
                "entity": {
                    "labels": updated.labels,
                    "properties": props,
                },
            });
            (200, resp.to_string())
        }
        Err(e) => (400, json_error(e)),
    }
}

fn json_to_prop(v: &serde_json::Value) -> PropertyValue {
    match v {
        serde_json::Value::String(s) => PropertyValue::String(s.clone()),
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                PropertyValue::Float(f)
            } else if let Some(i) = n.as_i64() {
                PropertyValue::Integer(i)
            } else {
                PropertyValue::String(n.to_string())
            }
        }
        serde_json::Value::Bool(b) => PropertyValue::Boolean(*b),
        serde_json::Value::Array(arr) => {
            PropertyValue::List(arr.iter().map(json_to_prop).collect())
        }
        serde_json::Value::Object(obj) => {
            let map: HashMap<String, PropertyValue> = obj
                .iter()
                .map(|(k, v)| (k.clone(), json_to_prop(v)))
                .collect();
            PropertyValue::Map(map)
        }
        serde_json::Value::Null => PropertyValue::Null,
    }
}

fn prop_to_json(v: &PropertyValue) -> serde_json::Value {
    match v {
        PropertyValue::String(s) => serde_json::Value::String(s.clone()),
        PropertyValue::Integer(i) => serde_json::Number::from(*i).into(),
        PropertyValue::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        PropertyValue::Boolean(b) => serde_json::Value::Bool(*b),
        PropertyValue::List(arr) => {
            serde_json::Value::Array(arr.iter().map(prop_to_json).collect())
        }
        PropertyValue::Map(m) => serde_json::Value::Object(
            m.iter()
                .map(|(k, v)| (k.clone(), prop_to_json(v)))
                .collect(),
        ),
        PropertyValue::Null => serde_json::Value::Null,
    }
}
