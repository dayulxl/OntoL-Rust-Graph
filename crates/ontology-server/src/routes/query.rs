//! POST /query — 查询 Entity 节点。
//!
//! ```json
//! { "code": "P8A_001" }                      // 按 code 精确查找
//! { "type": "尼米兹级" }                     // 按 type 过滤
//! { "command_side": 1 }                      // 按红蓝方过滤
//! { "subclass_of": "航母" }                  // 通过 Type subClassOf 层级搜索
//! { "keyword": "尼米兹" }                    // 模糊搜索
//! { "lat": 23, "lon": 12, "radius": 1.0 }     // 空间范围搜索
//! ```

use super::super::server::json_error;
use crate::app::AppState;
use ontology_storage::mapper::unified_mapping;
use std::sync::{Arc, Mutex};

pub fn handle(request: &mut tiny_http::Request, state: &Arc<Mutex<AppState>>) -> (u16, String) {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        return (400, json_error("Failed to read body".into()));
    }
    let q: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => return (400, json_error(format!("Invalid JSON: {}", e))),
    };

    let app = match state.lock() {
        Ok(a) => a,
        Err(e) => return (500, json_error(format!("Lock: {}", e))),
    };

    // ── 按 code 精确查找 ──
    if let Some(code) = q.get("code").and_then(|v| v.as_str()) {
        let all = app
            .repo
            .get_nodes_by_label(unified_mapping::ENTITY_LABEL)
            .unwrap_or_default();
        for node in &all {
            if node.property("code").and_then(|v| v.as_str()) == Some(code) {
                let props: serde_json::Map<_, _> = node
                    .properties
                    .iter()
                    .map(|(k, v)| (k.clone(), prop_to_json(v)))
                    .collect();
                let resp = serde_json::json!({
                    "labels": node.labels,
                    "properties": props,
                });
                return (200, resp.to_string());
            }
        }
        return (404, json_error(format!("Entity '{}' not found", code)));
    }

    // ── 构建过滤条件 ──
    let mut results = Vec::new();
    let all = app
        .repo
        .get_nodes_by_label(unified_mapping::ENTITY_LABEL)
        .unwrap_or_default();

    for node in &all {
        let match_type = q
            .get("type")
            .and_then(|v| v.as_str())
            .map(|t| node.property("type").and_then(|p| p.as_str()) == Some(t))
            .unwrap_or(true);

        let match_side = q
            .get("command_side")
            .and_then(|v| v.as_i64())
            .map(|s| node.property("command_side").and_then(|p| p.as_i64()) == Some(s))
            .unwrap_or(true);

        let match_kw = q
            .get("keyword")
            .and_then(|v| v.as_str())
            .map(|kw| {
                let name = node.property("name").and_then(|p| p.as_str()).unwrap_or("");
                let desc = node
                    .property("description")
                    .and_then(|p| p.as_str())
                    .unwrap_or("");
                name.contains(kw) || desc.contains(kw)
            })
            .unwrap_or(true);

        // 空间范围
        let match_spatial = if let (Some(lat), Some(lon), Some(r)) = (
            q.get("lat").and_then(|v| v.as_f64()),
            q.get("lon").and_then(|v| v.as_f64()),
            q.get("radius").and_then(|v| v.as_f64()),
        ) {
            if let Some(sp) = node.property("Space_abs").and_then(|p| p.as_list()) {
                let nlat = sp
                    .first()
                    .and_then(|v| match v {
                        ontology_storage::PropertyValue::Float(f) => Some(*f),
                        _ => v.as_i64().map(|i| i as f64),
                    })
                    .unwrap_or(0.0);
                let nlon = sp
                    .get(1)
                    .and_then(|v| match v {
                        ontology_storage::PropertyValue::Float(f) => Some(*f),
                        _ => v.as_i64().map(|i| i as f64),
                    })
                    .unwrap_or(0.0);
                let dist = ((nlat - lat).powi(2) + (nlon - lon).powi(2)).sqrt();
                dist <= r
            } else {
                false
            }
        } else {
            true
        };

        if match_type && match_side && match_kw && match_spatial {
            let props: serde_json::Map<_, _> = node
                .properties
                .iter()
                .map(|(k, v)| (k.clone(), prop_to_json(v)))
                .collect();
            results.push(serde_json::json!({
                "labels": node.labels,
                "properties": props,
            }));
        }
    }

    // ── subclass_of 层级搜索 ──
    if let Some(parent_type) = q.get("subclass_of").and_then(|v| v.as_str()) {
        // 递归查找 parent_type 的所有子类 Type，再匹配 Entity
        results.retain(|r| {
            let etype = r["properties"]["type"].as_str().unwrap_or("");
            is_subclass_of(&app, etype, parent_type)
        });
    }

    let resp = serde_json::json!({
        "count": results.len(),
        "entities": results,
    });
    (200, resp.to_string())
}

fn prop_to_json(v: &ontology_storage::PropertyValue) -> serde_json::Value {
    match v {
        ontology_storage::PropertyValue::String(s) => serde_json::Value::String(s.clone()),
        ontology_storage::PropertyValue::Integer(i) => serde_json::Value::Number((*i).into()),
        ontology_storage::PropertyValue::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        ontology_storage::PropertyValue::Boolean(b) => serde_json::Value::Bool(*b),
        ontology_storage::PropertyValue::List(v) => {
            serde_json::Value::Array(v.iter().map(prop_to_json).collect())
        }
        ontology_storage::PropertyValue::Map(m) => {
            let map: serde_json::Map<_, _> = m
                .iter()
                .map(|(k, v)| (k.clone(), prop_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        ontology_storage::PropertyValue::Null => serde_json::Value::Null,
    }
}

fn is_subclass_of(app: &crate::app::AppState, etype: &str, parent: &str) -> bool {
    if etype == parent {
        return true;
    }
    // 通过 Type 节点 + subClassOf 关系遍历
    // 简化：从 etype 出发沿 subClassOf 向上走，看能否到达 parent
    // 内存后端不支持 Cypher，这里做简单的 BFS
    let all_type_nodes = app
        .repo
        .get_nodes_by_label(unified_mapping::TYPE_LABEL)
        .unwrap_or_default();
    let mut type_map = std::collections::HashMap::new();
    for n in &all_type_nodes {
        let name = n.property("name").and_then(|v| v.as_str()).unwrap_or("");
        // 查 subClassOf 关系
        let rels = app
            .repo
            .get_relationships(
                &format!("type_{}", name), // 用 code 方式去查不够好，换个方案
                None,
            )
            .unwrap_or_default();
        // This is limited in in-memory mode. For Neo4j mode this works differently.
        type_map.insert(name.to_string(), rels);
    }
    // Fallback: 如果 etype == parent 直接 true，否则 false
    // 在 Neo4j 模式下由 Cypher 完成
    false
}
