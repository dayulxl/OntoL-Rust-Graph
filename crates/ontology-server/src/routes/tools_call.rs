//! POST /tools/call — LLM Function Calling 统一调度。
//!
//! 大模型通过 `GET /tools` 发现可用工具，再通过 `POST /tools/call` 调用。
//!
//! 请求格式（OpenAI Function Calling 兼容）：
//! ```json
//! { "name": "simulate_strike", "arguments": { "code": "S_001", ... } }
//! ```
//!
//! 响应：
//! ```json
//! { "tool": "simulate_strike", "ok": true, "result": { ... } }
//! ```

use std::sync::{Arc, Mutex};

use ontology_reasoner::graph::find_entity_any;
use ontology_storage::mapper::unified_mapping;

use super::super::server::json_error;
use crate::app::AppState;

/// 工具调度表入口
pub fn handle(request: &mut tiny_http::Request, state: &Arc<Mutex<AppState>>) -> (u16, String) {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        return (400, json_error("Failed to read body".into()));
    }

    let call: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => return (400, json_error(format!("Invalid JSON: {}", e))),
    };

    let tool_name = call.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let arguments = call
        .get("arguments")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    if tool_name.is_empty() {
        return (400, json_error("Missing 'name' field".into()));
    }

    let mut app = match state.lock() {
        Ok(a) => a,
        Err(e) => return (500, json_error(format!("Lock: {}", e))),
    };

    let repo = app.repo.as_ref();

    let result: Result<serde_json::Value, String> = match tool_name {
        // ── 查询类 ──
        "search_entities" => dispatch_search_entities(&arguments, &app),
        "get_entity_context" => dispatch_get_entity_context(&arguments, &app),

        // ── 推理类 ──
        "load_swrl_rule" => dispatch_load_swrl_rule(&arguments, &mut app),
        "execute_reasoning" => dispatch_execute_reasoning(&arguments, &mut app),

        // ── 创建类 ──
        "create_entity" => {
            let p = &arguments;
            ontology_storage::ontology::entity::create_entity(repo, p)
                .map(|v| serde_json::json!({"created": v}))
        }
        "create_type" => {
            let p = &arguments;
            ontology_storage::ontology::entity::create_type(repo, p)
                .map(|v| serde_json::json!({"created": v}))
        }
        "create_patrol" => {
            let p = &arguments;
            ontology_storage::ontology::entity::create_patrol(repo, p)
                .map(|v| serde_json::json!({"created": v}))
        }
        "create_relationship" => {
            let p = &arguments;
            ontology_storage::ontology::relationship::create_relationship(repo, p)
                .map(|v| serde_json::json!({"created": v}))
        }

        // ── 推演类 ──
        "simulate_patrol" => dispatch_simulate_patrol(&arguments, &app),
        "simulate_strike" => dispatch_simulate_strike(&arguments, &app),

        // ── 向前推理 ──
        "infer_forward" => dispatch_infer_forward(&arguments, &app),

        // ── 更新类 ──
        "update_entity" => dispatch_update_entity(&arguments, &app),

        other => Err(format!(
            "Unknown tool: '{}'. See GET /tools for available tools.",
            other
        )),
    };

    match result {
        Ok(data) => {
            let resp = serde_json::json!({
                "tool": tool_name,
                "ok": true,
                "result": data,
            });
            (200, resp.to_string())
        }
        Err(e) => {
            let resp = serde_json::json!({
                "tool": tool_name,
                "ok": false,
                "error": e,
                "arguments": arguments,
            });
            (400, resp.to_string())
        }
    }
}

// ═══════════════════════════════════════════════════════════
// 分发实现
// ═══════════════════════════════════════════════════════════

fn dispatch_search_entities(
    args: &serde_json::Value,
    app: &AppState,
) -> Result<serde_json::Value, String> {
    let code = args.get("code").and_then(|v| v.as_str());
    let typ = args.get("type").and_then(|v| v.as_str());
    let side = args.get("command_side").and_then(|v| v.as_i64());
    let kw = args.get("keyword").and_then(|v| v.as_str());

    let all = app
        .repo
        .get_nodes_by_label(unified_mapping::ENTITY_LABEL)
        .map_err(|e| e.to_string())?;
    let mut results: Vec<serde_json::Value> = Vec::new();

    for node in &all {
        // code 精确匹配优先
        if let Some(c) = code
            && node.property("code").and_then(|v| v.as_str()) != Some(c)
        {
            continue;
        }
        if let Some(t) = typ
            && node.property("type").and_then(|v| v.as_str()) != Some(t)
        {
            continue;
        }
        if let Some(s) = side
            && node.property("command_side").and_then(|v| v.as_i64()) != Some(s)
        {
            continue;
        }
        if let Some(k) = kw {
            let name = node.property("name").and_then(|v| v.as_str()).unwrap_or("");
            let desc = node
                .property("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !name.contains(k) && !desc.contains(k) {
                continue;
            }
        }

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

    Ok(serde_json::json!({ "count": results.len(), "entities": results }))
}

fn dispatch_get_entity_context(
    args: &serde_json::Value,
    app: &AppState,
) -> Result<serde_json::Value, String> {
    let code = args
        .get("code")
        .and_then(|v| v.as_str())
        .ok_or("'code' is required")?;
    let depth = args
        .get("depth")
        .and_then(|v| v.as_i64())
        .unwrap_or(2)
        .clamp(1, 3) as usize;

    // 查找实体 — 使用统一的跨标签查找（Entity/Event/Patrol/Strike/Type/Behavior）
    let entity = find_entity_any(app.repo.as_ref(), code)
        .ok_or_else(|| format!("Entity '{}' not found", code))?;

    let mut context = serde_json::json!({
        "entity": node_to_json(&entity),
        "neighbors": [],
    });

    // BFS 图展开
    let mut visited = std::collections::HashSet::new();
    visited.insert(code.to_string());
    let mut current_codes = vec![code.to_string()];

    for _ in 0..depth {
        let mut next_codes = Vec::new();
        for c in &current_codes {
            let rels = app.repo.get_relationships(c, None).unwrap_or_default();
            for r in &rels {
                let neighbor_code = r.end_node_id.clone();
                if !visited.contains(&neighbor_code) {
                    visited.insert(neighbor_code.clone());
                    // 找邻居节点
                    for label in &[
                        unified_mapping::ENTITY_LABEL,
                        unified_mapping::TYPE_LABEL,
                        unified_mapping::PATROL_LABEL,
                        unified_mapping::BEHAVIOR_LABEL,
                    ] {
                        let nodes = app.repo.get_nodes_by_label(label).unwrap_or_default();
                        if let Some(n) = nodes.iter().find(|n| {
                            n.property("code").and_then(|v| v.as_str()) == Some(&neighbor_code)
                        }) {
                            context["neighbors"]
                                .as_array_mut()
                                .unwrap()
                                .push(serde_json::json!({
                                    "code": neighbor_code,
                                    "label": label,
                                    "relation": r.rel_type,
                                    "node": node_to_json(n),
                                }));
                            break;
                        }
                    }
                    next_codes.push(neighbor_code);
                }
            }
        }
        current_codes = next_codes;
    }

    Ok(context)
}

fn dispatch_load_swrl_rule(
    args: &serde_json::Value,
    app: &mut AppState,
) -> Result<serde_json::Value, String> {
    let rule_text = args
        .get("rule_text")
        .and_then(|v| v.as_str())
        .ok_or("'rule_text' is required")?;

    app.reasoner
        .load_swrl_rule(rule_text)
        .map_err(|e| format!("Parse error: {}", e))?;

    Ok(serde_json::json!({
        "loaded": true,
        "rule_count": app.reasoner.rule_count(),
    }))
}

fn dispatch_execute_reasoning(
    args: &serde_json::Value,
    app: &mut AppState,
) -> Result<serde_json::Value, String> {
    let _incremental = args
        .get("incremental")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let report = app
        .reasoner
        .reason()
        .map_err(|e| format!("Reasoning error: {}", e))?;

    Ok(serde_json::json!({
        "rules_loaded": report.rules_loaded,
        "total_steps": report.stats.total_steps,
        "derived_facts": report.stats.total_derived,
        "fuse_trips": report.stats.fuse_trips,
        "total_ms": report.total_ms,
    }))
}

fn dispatch_simulate_patrol(
    args: &serde_json::Value,
    app: &AppState,
) -> Result<serde_json::Value, String> {
    let code = args.get("code").and_then(|v| v.as_str()).unwrap_or("");
    let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
    if code.is_empty() || name.is_empty() {
        return Err("'code' and 'name' are required".into());
    }

    let wps_raw = args
        .get("waypoints")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    let wps_arr = wps_raw.as_array().map(|a| a.to_vec()).unwrap_or_default();

    use ontology_reasoner::timeline::{TimelineEngine, TimelineInput, WaypointInput};
    let waypoints: Vec<WaypointInput> = wps_arr
        .iter()
        .map(|wp| WaypointInput {
            seq: wp.get("seq").and_then(|v| v.as_i64()).unwrap_or(0),
            lat: wp.get("lat").and_then(|v| v.as_f64()).unwrap_or(0.0),
            lon: wp.get("lon").and_then(|v| v.as_f64()).unwrap_or(0.0),
            alt: wp.get("alt").and_then(|v| v.as_f64()).unwrap_or(0.0),
            action: wp
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("MOVE")
                .to_string(),
        })
        .collect();

    // 匹配实体
    let (code, lat, lon, alt, speed) = find_entity_by_code(
        app,
        args.get("attacker").and_then(|v| v.as_str()),
        &waypoints,
    );
    let input = TimelineInput {
        patrol_code: code.to_string(),
        patrol_name: name.to_string(),
        patrol_id: String::new(),
        waypoints,
        entity_code: code,
        start_lat: lat,
        start_lon: lon,
        start_alt: alt,
        speed,
    };
    let engine = TimelineEngine::new();
    let result = engine.simulate(&input);
    let entity_code = serde_json::json!({
        "entity": result.entity_code,
        "total_distance_m": result.total_distance_m,
        "total_distance_km": result.total_distance_km,
        "total_time_s": result.total_time_s,
        "duration": result.duration,
        "segments": result.segments.iter().map(|s| serde_json::json!({
            "seq": s.seq, "lat": s.lat, "lon": s.lon, "alt": s.alt,
            "action": s.action, "dist_m": s.dist_m, "time_s": s.time_s,
        })).collect::<Vec<_>>(),
        "precondition": result.precondition,
        "effect": result.effect,
        "cost": result.cost,
        "composedOf": result.composed_of,
    });

    Ok(entity_code)
}

fn dispatch_simulate_strike(
    args: &serde_json::Value,
    app: &AppState,
) -> Result<serde_json::Value, String> {
    let code = args.get("code").and_then(|v| v.as_str()).unwrap_or("");
    let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let attacker_code = args
        .get("attacker")
        .and_then(|v| v.as_str())
        .ok_or("'attacker' is required")?;
    let target_code = args
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or("'target' is required")?;
    let weapon_type = args
        .get("weapon_type")
        .and_then(|v| v.as_str())
        .ok_or("'weapon_type' is required")?;

    let attacker = find_entity_by_label(app, attacker_code);
    let target = find_entity_by_label(app, target_code);

    use ontology_reasoner::timeline::{StrikeInput, TimelineEngine};
    let engine = TimelineEngine::new();

    let (a_lat, a_lon, a_alt, a_conf) = match &attacker {
        Some(n) => (
            get_float(n, "Space_abs", 0),
            get_float(n, "Space_abs", 1),
            get_float(n, "Space_abs", 3),
            prop_as_f64(n.property("confidence")).unwrap_or(0.8),
        ),
        None => (
            args.get("attacker_lat")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            args.get("attacker_lon")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            args.get("attacker_alt")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            args.get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.7),
        ),
    };

    let (t_lat, t_lon, t_depth) = match &target {
        Some(n) => (
            get_float(n, "Space_abs", 0),
            get_float(n, "Space_abs", 1),
            -get_float(n, "Space_abs", 2),
        ),
        None => (
            args.get("target_lat")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            args.get("target_lon")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            args.get("target_depth")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
        ),
    };

    let weapon_range = args
        .get("weapon_range_m")
        .and_then(|v| v.as_f64())
        .unwrap_or(10_000.0);
    let confidence = args
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(a_conf);

    let input = StrikeInput {
        strike_code: code.to_string(),
        strike_name: name.to_string(),
        strike_id: String::new(),
        attacker_code: attacker_code.to_string(),
        attacker_lat: a_lat,
        attacker_lon: a_lon,
        attacker_alt: a_alt,
        target_code: target_code.to_string(),
        target_lat: t_lat,
        target_lon: t_lon,
        target_depth: t_depth,
        weapon_type: weapon_type.to_string(),
        weapon_range_m: weapon_range,
        confidence,
    };

    let result = engine.simulate_strike(&input);

    Ok(serde_json::json!({
        "attacker": result.attacker_code,
        "target": result.target_code,
        "weapon_type": result.weapon_type,
        "distance_m": result.distance_m,
        "distance_km": format!("{:.2}", result.distance_m / 1000.0),
        "in_range": result.in_range,
        "hit_probability": result.hit_probability,
        "damage_level": result.damage_level,
        "total_time_s": result.total_time_s,
        "duration": result.duration,
        "precondition": result.precondition,
        "effect": result.effect,
        "cost": result.cost,
        "composedOf": result.composed_of,
    }))
}

fn dispatch_update_entity(
    args: &serde_json::Value,
    app: &AppState,
) -> Result<serde_json::Value, String> {
    use ontology_reasoner::graph::update_entity_properties;
    use std::collections::HashMap;

    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("'id' is required")?;
    let updates_json = args
        .get("updates")
        .and_then(|v| v.as_object())
        .ok_or("'updates' is required and must be an object")?;

    if updates_json.is_empty() {
        return Err("'updates' must not be empty".into());
    }

    let cope_version = args.get("cope_version").and_then(|v| v.as_str());

    let mut updates = HashMap::new();
    for (k, v) in updates_json {
        updates.insert(k.clone(), json_to_prop_inner(v));
    }

    let updated = update_entity_properties(app.repo.as_ref(), id, updates, cope_version)
        .map_err(|e| format!("Update failed: {}", e))?;

    let props: serde_json::Map<_, _> = updated
        .properties
        .iter()
        .map(|(k, v)| (k.clone(), prop_to_json(v)))
        .collect();

    Ok(serde_json::json!({
        "updated": true,
        "entity": { "labels": updated.labels, "properties": props },
    }))
}

fn json_to_prop_inner(v: &serde_json::Value) -> ontology_storage::PropertyValue {
    use ontology_storage::PropertyValue;
    match v {
        serde_json::Value::String(s) => PropertyValue::String(s.clone()),
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                PropertyValue::Float(f)
            } else {
                PropertyValue::Integer(n.as_i64().unwrap_or(0))
            }
        }
        serde_json::Value::Bool(b) => PropertyValue::Boolean(*b),
        serde_json::Value::Array(arr) => {
            PropertyValue::List(arr.iter().map(json_to_prop_inner).collect())
        }
        serde_json::Value::Object(obj) => {
            let map: std::collections::HashMap<String, PropertyValue> = obj
                .iter()
                .map(|(k, v)| (k.clone(), json_to_prop_inner(v)))
                .collect();
            PropertyValue::Map(map)
        }
        serde_json::Value::Null => PropertyValue::Null,
    }
}

// ═══════════════════════════════════════════════════════════
// 向前推理（infer-forward）
// ═══════════════════════════════════════════════════════════

fn dispatch_infer_forward(
    args: &serde_json::Value,
    app: &AppState,
) -> Result<serde_json::Value, String> {
    use ontology_reasoner::graph::{Direction, ExploreConfig, GraphExplorer};

    let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let code = args.get("code").and_then(|v| v.as_str());
    if id.is_empty() && code.is_none() {
        return Err("至少需要 'id' 或 'code' 之一".into());
    }
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("'name' is required")?;
    let relation = args.get("relation").and_then(|v| v.as_str()).unwrap_or("");
    let cope_version = args
        .get("cope_version")
        .and_then(|v| v.as_str())
        .ok_or("'cope_version' is required")?;
    let depth = args
        .get("depth")
        .and_then(|v| v.as_u64())
        .unwrap_or(3)
        .min(5) as usize;
    let direction_str = args
        .get("direction")
        .and_then(|v| v.as_str())
        .unwrap_or("outgoing");
    let confidence_threshold = args.get("confidence_threshold").and_then(|v| v.as_f64());

    let explorer = GraphExplorer::new(app.repo.clone());

    let config = ExploreConfig {
        start_id: id.to_string(),
        start_code: code.map(|s| s.to_string()),
        relation: relation.to_string(),
        max_depth: depth,
        direction: direction_str.parse::<Direction>().unwrap_or_default(),
        confidence_threshold,
        cope_version: Some(cope_version.to_string()),
        inference_only: true,
    };

    let result = explorer.explore(&config)?;

    // 组装摘要
    let chain_count = result.chain.len();
    let source_code = result
        .source
        .property("code")
        .and_then(|v| v.as_str())
        .unwrap_or(id);
    let stopped_count = result.chain.iter().filter(|h| h.stop_propagation).count();
    let with_conf: Vec<f64> = result.chain.iter().filter_map(|h| h.confidence).collect();
    let avg_conf = if with_conf.is_empty() {
        None
    } else {
        Some(with_conf.iter().sum::<f64>() / with_conf.len() as f64)
    };

    let chain_json: Vec<serde_json::Value> = result
        .chain
        .iter()
        .map(|hop| {
            let target_id: &str = &hop.target_id;
            let target_json = hop
                .target_node
                .as_ref()
                .map(node_to_json)
                .unwrap_or_else(|| serde_json::json!({"id": target_id}));

            let next_rels: Vec<serde_json::Value> = hop
                .target_outgoing
                .iter()
                .map(|rc| {
                    serde_json::json!({
                        "relation": rc.relation,
                        "count": rc.count,
                        "example_targets": rc.example_targets,
                    })
                })
                .collect();

            let rules: Vec<serde_json::Value> = hop
                .matching_rules
                .iter()
                .map(|rm| {
                    serde_json::json!({
                        "rule_name": rm.rule_name,
                        "source_file": rm.source_file,
                    })
                })
                .collect();

            serde_json::json!({
                "hop": hop.hop,
                "direction": hop.direction,
                "source_id": hop.source_id,
                "target_id": target_id,
                "rel_type": hop.rel_type,
                "target": target_json,
                "type_hierarchy": hop.target_type_chain,
                "confidence": hop.confidence,
                "stop_propagation": hop.stop_propagation,
                "next_relations": next_rels,
                "matching_rules": rules,
            })
        })
        .collect();

    let avg_str = match avg_conf {
        Some(v) => format!("{:.2}", v),
        None => "N/A".into(),
    };

    let rel_display = if relation.is_empty() {
        "所有关系"
    } else {
        relation
    };
    let summary = format!(
        "{}: 实体 '{}' 沿 '{}' 关系 {} 方向遍历 {} 跳, 匹配 {} 条规则。有置信度数据的节点平均: {}, 因低于阈值停止: {} 处",
        name,
        source_code,
        rel_display,
        direction_str,
        chain_count,
        result.chain.iter().flat_map(|h| &h.matching_rules).count(),
        avg_str,
        stopped_count,
    );

    Ok(serde_json::json!({
        "source": node_to_json(&result.source),
        "relation": rel_display,
        "direction": direction_str,
        "confidence_threshold": confidence_threshold,
        "chain": chain_json,
        "hops": chain_count,
        "confidence": {
            "average": avg_conf.map(|v| format!("{:.2}", v)).unwrap_or("N/A".into()),
            "nodes_with_confidence": with_conf.len(),
            "stop_propagation_count": stopped_count,
        },
        "summary": summary,
    }))
}

// ═══════════════════════════════════════════════════════════
// 辅助函数
// ═══════════════════════════════════════════════════════════

fn prop_to_json(v: &ontology_storage::mapper::graph::property::PropertyValue) -> serde_json::Value {
    use ontology_storage::mapper::graph::property::PropertyValue;
    match v {
        PropertyValue::String(s) => serde_json::Value::String(s.clone()),
        PropertyValue::Integer(i) => serde_json::Value::Number((*i).into()),
        PropertyValue::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        PropertyValue::Boolean(b) => serde_json::Value::Bool(*b),
        PropertyValue::List(items) => {
            serde_json::Value::Array(items.iter().map(prop_to_json).collect())
        }
        PropertyValue::Map(m) => {
            let map: serde_json::Map<_, _> = m
                .iter()
                .map(|(k, v)| (k.clone(), prop_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        PropertyValue::Null => serde_json::Value::Null,
    }
}

fn node_to_json(n: &ontology_storage::mapper::graph::node::Node) -> serde_json::Value {
    let props: serde_json::Map<_, _> = n
        .properties
        .iter()
        .map(|(k, v)| (k.clone(), prop_to_json(v)))
        .collect();
    serde_json::json!({
        "labels": n.labels,
        "properties": props,
    })
}

fn prop_as_f64(
    val: Option<&ontology_storage::mapper::graph::property::PropertyValue>,
) -> Option<f64> {
    use ontology_storage::mapper::graph::property::PropertyValue;
    match val {
        Some(PropertyValue::Float(f)) => Some(*f),
        Some(PropertyValue::Integer(i)) => Some(*i as f64),
        _ => None,
    }
}

fn get_float(node: &ontology_storage::mapper::graph::node::Node, key: &str, idx: usize) -> f64 {
    node.property(key)
        .and_then(|v| v.as_list())
        .and_then(|arr| arr.get(idx))
        .and_then(|v| match v {
            ontology_storage::mapper::graph::property::PropertyValue::Float(f) => Some(*f),
            ontology_storage::mapper::graph::property::PropertyValue::Integer(i) => Some(*i as f64),
            _ => None,
        })
        .unwrap_or(0.0)
}

fn find_entity_by_label(
    app: &AppState,
    code: &str,
) -> Option<ontology_storage::mapper::graph::node::Node> {
    if code.is_empty() {
        return None;
    }
    let all = app
        .repo
        .get_nodes_by_label(unified_mapping::ENTITY_LABEL)
        .unwrap_or_default();
    all.into_iter()
        .find(|n| n.property("code").and_then(|v| v.as_str()) == Some(code))
}

/// 匹配巡逻实体：通过 [:移动] 关系 → speed → 取第一个
fn find_entity_by_code(
    app: &AppState,
    specified: Option<&str>,
    waypoints: &[ontology_reasoner::timeline::WaypointInput],
) -> (String, f64, f64, f64, f64) {
    let all = app
        .repo
        .get_nodes_by_label(unified_mapping::ENTITY_LABEL)
        .unwrap_or_default();

    // 指定了 attacker code → 直接找
    if let Some(code) = specified {
        for n in &all {
            if n.property("code").and_then(|v| v.as_str()) == Some(code) {
                return (
                    code.to_string(),
                    get_float(n, "Space_abs", 0),
                    get_float(n, "Space_abs", 1),
                    get_float(n, "Space_abs", 3),
                    prop_as_f64(n.property("speed")).unwrap_or(200.0),
                );
            }
        }
    }

    // 回退：取第一个有 speed 的 Entity，坐标用第一个航点
    for n in &all {
        if let Some(speed) = prop_as_f64(n.property("speed")) {
            let code = n
                .property("code")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            let lat = waypoints.first().map(|w| w.lat).unwrap_or(0.0);
            let lon = waypoints.first().map(|w| w.lon).unwrap_or(0.0);
            return (code, lat, lon, 0.0, speed);
        }
    }

    ("unknown".into(), 0.0, 0.0, 0.0, 200.0)
}
