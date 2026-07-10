//! POST /context — 获取 Entity 图上下文快照。
//!
//! ```json
//! { "code": "P8A_001", "depth": 2 }
//! ```

use super::super::server::json_error;
use crate::app::AppState;
use ontology_storage::mapper::unified_mapping;
use std::collections::HashSet;
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
    let code = match q.get("code").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return (400, json_error("Missing 'code'".into())),
    };
    let _depth = q.get("depth").and_then(|v| v.as_u64()).unwrap_or(2).min(3) as usize;

    let app = match state.lock() {
        Ok(a) => a,
        Err(e) => return (500, json_error(format!("Lock: {}", e))),
    };

    // ── 查找主实体（按 code 扫描 Entity 节点） ──
    let all_entities = app
        .repo
        .get_nodes_by_label(unified_mapping::ENTITY_LABEL)
        .unwrap_or_default();
    let entity = match all_entities
        .iter()
        .find(|n| n.property("code").and_then(|v| v.as_str()) == Some(code))
    {
        Some(n) => n.clone(),
        None => return (404, json_error(format!("Entity '{}' not found", code))),
    };

    let mut props = serde_json::Map::new();
    for (k, v) in &entity.properties {
        props.insert(k.clone(), p2j(v));
    }

    // ── 出向关系（所有类型） ──
    let outgoing_raw = app.repo.get_relationships(code, None).unwrap_or_default();
    let mut outgoing = Vec::new();
    let mut seen = HashSet::new();
    seen.insert(code.to_string());

    for rel in &outgoing_raw {
        let target = app.repo.get_node(&rel.end_node_id).ok().flatten();
        let tprops: serde_json::Map<_, _> = target
            .iter()
            .flat_map(|n| n.properties.iter().map(|(k, v)| (k.clone(), p2j(v))))
            .collect();
        outgoing.push(serde_json::json!({
            "relation": rel.rel_type,
            "target_code": &rel.end_node_id,
            "target_props": tprops,
        }));
    }

    // ── 入向关系 ──
    // InMemory 不支持入向查询，Neo4j 模式下由执行器处理
    let mut incoming = Vec::new();
    // 遍历所有 Entity，查它们有没有关系和当前节点相连
    let all = app
        .repo
        .get_nodes_by_label(unified_mapping::ENTITY_LABEL)
        .unwrap_or_default();
    for n in &all {
        let ncode = n.property("code").and_then(|v| v.as_str()).unwrap_or("");
        if seen.contains(ncode) {
            continue;
        }
        let rels = app.repo.get_relationships(ncode, None).unwrap_or_default();
        for rel in &rels {
            if rel.end_node_id == code {
                seen.insert(ncode.to_string());
                incoming.push(serde_json::json!({
                    "relation": rel.rel_type,
                    "source_code": ncode,
                }));
            }
        }
    }

    // ── 分类层级 ──
    let etype = entity
        .property("type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let type_path = get_type_ancestors(&app, etype);

    let resp = serde_json::json!({
        "entity": {
            "code": code,
            "labels": entity.labels,
            "properties": props,
        },
        "outgoing": outgoing,
        "incoming": incoming,
        "type_hierarchy": type_path,
        "summary": format!(
            "Entity '{}' ({}) — {} outgoing rels, {} incoming rels, type chain: {}",
            entity.property("name").and_then(|v| v.as_str()).unwrap_or("?"),
            code,
            outgoing.len(),
            incoming.len(),
            type_path.join(" → "),
        ),
    });
    (200, resp.to_string())
}

fn p2j(v: &ontology_storage::PropertyValue) -> serde_json::Value {
    match v {
        ontology_storage::PropertyValue::String(s) => serde_json::Value::String(s.clone()),
        ontology_storage::PropertyValue::Integer(i) => serde_json::Number::from(*i).into(),
        ontology_storage::PropertyValue::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        ontology_storage::PropertyValue::Boolean(b) => serde_json::Value::Bool(*b),
        ontology_storage::PropertyValue::List(v) => {
            serde_json::Value::Array(v.iter().map(p2j).collect())
        }
        ontology_storage::PropertyValue::Map(m) => {
            serde_json::Value::Object(m.iter().map(|(k, v)| (k.clone(), p2j(v))).collect())
        }
        ontology_storage::PropertyValue::Null => serde_json::Value::Null,
    }
}

fn get_type_ancestors(app: &crate::app::AppState, type_name: &str) -> Vec<String> {
    let mut path = vec![type_name.to_string()];
    if type_name.is_empty() {
        return path;
    }
    // 简化 BFS: 查 Type 节点沿 subClassOf 向上
    let types = app
        .repo
        .get_nodes_by_label(unified_mapping::TYPE_LABEL)
        .unwrap_or_default();
    // Build adjacency for subClassOf
    let mut parents: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for t in &types {
        let tname = t.property("name").and_then(|v| v.as_str()).unwrap_or("");
        let rels = app.repo.get_relationships(tname, None).unwrap_or_default();
        for r in &rels {
            if r.rel_type == unified_mapping::SUB_CLASS_OF_REL {
                let parent = app.repo.get_node(&r.end_node_id).ok().flatten();
                if let Some(pn) = parent
                    && let Some(pname) = pn.property("name").and_then(|v| v.as_str())
                {
                    parents.insert(tname.to_string(), pname.to_string());
                }
            }
        }
    }
    let mut current = type_name.to_string();
    for _ in 0..10 {
        if let Some(p) = parents.get(&current) {
            path.push(p.clone());
            current = p.clone();
        } else {
            break;
        }
    }
    path
}
