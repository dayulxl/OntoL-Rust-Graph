//! GET|POST /patrol — 巡逻任务接口。
//!
//! **POST** — 解析 JSON → 调用推理机 TimelineEngine 执行推演 → 回写结果到图存储。
//! **GET /patrol?code=xxx** — 查询指定巡逻任务。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ontology_storage::mapper::graph::node::Node;
use ontology_storage::mapper::graph::property::PropertyValue;
use ontology_reasoner::timeline::{TimelineEngine, TimelineInput, WaypointInput};

use crate::app::AppState;
use super::super::server::json_error;

macro_rules! logf {
    ($file:expr, $($arg:tt)*) => {{
        let msg = format!($($arg)*);
        log::info!("{}", msg);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open($file) {
            use std::io::Write;
            let _ = writeln!(f, "{}", msg);
        }
    }};
}

pub fn handle(
    request: &mut tiny_http::Request,
    state: &Arc<Mutex<AppState>>,
    method: &str,
) -> (u16, String) {
    match method {
        "GET" => handle_get(request, state),
        "POST" => handle_post(request, state),
        _ => (405, json_error("Method not allowed".into())),
    }
}

// ═══════════════════════════════════════════════════════════
// GET
// ═══════════════════════════════════════════════════════════

fn handle_get(
    request: &mut tiny_http::Request,
    state: &Arc<Mutex<AppState>>,
) -> (u16, String) {
    let code = request.url().to_string()
        .split("code=").nth(1).unwrap_or("").to_string();

    let app = match state.lock() { Ok(a) => a, Err(e) => return (500, json_error(e.to_string())) };
    let all = app.repo.get_nodes_by_label("Patrol").unwrap_or_default();

    if code.is_empty() {
        let list: Vec<serde_json::Value> = all.iter().map(node_to_patrol).collect();
        return (200, serde_json::json!({ "count": list.len(), "patrols": list }).to_string());
    }
    for n in &all {
        if n.property("code").and_then(|v| v.as_str()) == Some(&code) {
            return (200, node_to_patrol(n).to_string());
        }
    }
    (404, json_error(format!("Patrol '{}' not found", code)))
}

// ═══════════════════════════════════════════════════════════
// POST — 解析 JSON → 调用推理机推演 → 写回图存储
// ═══════════════════════════════════════════════════════════

fn handle_post(
    request: &mut tiny_http::Request,
    state: &Arc<Mutex<AppState>>,
) -> (u16, String) {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        return (400, json_error("Failed to read body".into()));
    }
    let patrols: Vec<serde_json::Value> = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => return (400, json_error(format!("Invalid JSON: {}", e))),
    };

    let app = match state.lock() { Ok(a) => a, Err(e) => return (500, json_error(e.to_string())) };
    let mut results = Vec::new();
    let engine = TimelineEngine::new();

    // 日志文件
    let _ = std::fs::create_dir_all("logs");
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let log_path = format!("logs/patrol_{}.log", ts);

    for patrol in &patrols {
        let code = patrol.get("code").and_then(|v| v.as_str()).unwrap_or("");
        let name = patrol.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let pid  = patrol.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let wps_raw = patrol.get("waypoints").cloned().unwrap_or(serde_json::Value::Array(vec![]));
        let wps_arr = wps_raw.as_array().map(|a| a.to_vec()).unwrap_or_default();

        let waypoints: Vec<WaypointInput> = wps_arr.iter().map(|wp| WaypointInput {
            seq:    wp.get("seq").and_then(|v| v.as_i64()).unwrap_or(0),
            lat:    wp.get("lat").and_then(|v| v.as_f64()).unwrap_or(0.0),
            lon:    wp.get("lon").and_then(|v| v.as_f64()).unwrap_or(0.0),
            alt:    wp.get("alt").and_then(|v| v.as_f64()).unwrap_or(0.0),
            action: wp.get("action").and_then(|v| v.as_str()).unwrap_or("MOVE").to_string(),
        }).collect();

        // 找实体：通过 [:移动] 关系自动匹配
        let entity = find_entity(&app, code);
        let (entity_code, start_lat, start_lon, start_alt, speed) = match &entity {
            Some(n) => (
                n.property("code").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
                get_float(n, "Space_abs", 0),
                get_float(n, "Space_abs", 1),
                get_float(n, "Space_abs", 3),
                prop_as_f64(n.property("speed")).unwrap_or(200.0),
            ),
            None => ("?unknown".to_string(), 0.0, 0.0, 0.0, 200.0),
        };

        // ── 前置条件检查 ──
        let precondition_passed = check_preconditions(&log_path, &entity, patrol);
        if !precondition_passed.0 {
            logf!(&log_path, "   ⛔ 前置条件不满足: {}", precondition_passed.1);
            results.push(serde_json::json!({
                "code": code, "name": name, "entity": entity_code,
                "status": "blocked", "reason": precondition_passed.1,
            }));
            continue;
        }

        // ── 调用推理机 ──
        let input = TimelineInput {
            patrol_code: code.to_string(),
            patrol_name: name.to_string(),
            patrol_id: pid.to_string(),
            waypoints,
            entity_code,
            start_lat, start_lon, start_alt,
            speed,
        };
        let result = engine.simulate(&input);

        // ── 写回巡逻节点 ──
        let wps_json = serde_json::to_string(&wps_raw).unwrap_or_else(|_| "[]".into());
        let mut props = HashMap::new();
        props.insert("iri".to_string(),          PropertyValue::from(code));
        props.insert("id".to_string(),           PropertyValue::from(pid));
        props.insert("code".to_string(),         PropertyValue::from(code));
        props.insert("name".to_string(),         PropertyValue::from(name));
        props.insert("waypoints".to_string(),    PropertyValue::from(wps_json.as_str()));
        props.insert("duration".to_string(),     PropertyValue::from(result.duration));
        props.insert("priority".to_string(),     PropertyValue::from(3i64));
        props.insert("precondition".to_string(), PropertyValue::from(result.precondition.as_str()));
        props.insert("effect".to_string(),       PropertyValue::from(result.effect.as_str()));
        props.insert("cost".to_string(),         PropertyValue::from(result.cost.as_str()));
        props.insert("composedOf".to_string(),   PropertyValue::from(result.composed_of.as_str()));
        props.insert("domain".to_string(),       PropertyValue::from("Anti-SubmarineWarfare"));
        props.insert("status".to_string(),       PropertyValue::from("有效"));
        props.insert("update_time".to_string(),  PropertyValue::from("now"));

        let _ = app.repo.delete_node(code);
        if let Err(e) = app.repo.insert_node(&Node::new(vec!["Patrol".to_string()], props)) {
            return (500, json_error(e.to_string()));
        }

        // ── 每步更新实体坐标 ──
        let mut remaining_s = result.total_time_s;
        for seg in &result.segments {
            remaining_s -= seg.time_s;
            if let Some(ref ent) = entity {
                update_entity_position(
                    &app, ent, seg.lat, seg.lon, seg.alt, remaining_s.max(0.0) as i64,
                );
            }
        }

        // ── composedOf 递归推理：检查子动作链并级联传导 ──
        let composed = &result.composed_of;
        if !composed.is_empty() && composed.contains('^') {
            logf!(&log_path, "   ── composedOf 级联推理 ──");
            let mut chain_count = 0usize;
            for part in composed.split(" ^ ") {
                let part = part.trim();
                if part.is_empty() { continue; }
                if let Some(paren) = part.find('(') {
                    let _action_name = &part[..paren];
                    chain_count += 1;
                    logf!(&log_path, "      ├ [{}] {}", chain_count, part);

                    // 查找关联实体上是否有自己的 composedOf
                    let all = app.repo.get_nodes_by_label("Entity").unwrap_or_default();
                    for sub_e in &all {
                        let sub_comp = sub_e.property("composedOf")
                            .and_then(|v| v.as_str()).unwrap_or("");
                        if !sub_comp.is_empty() && sub_comp != composed as &str {
                            let sub_code = sub_e.property("code").and_then(|v| v.as_str()).unwrap_or("");
                            logf!(&log_path, "         ↳ 级联到实体 {}: {}", sub_code, sub_comp);
                            // 递归：该实体也有子动作需要继续推理
                            for sub_part in sub_comp.split(" ^ ") {
                                if !sub_part.trim().is_empty() {
                                    logf!(&log_path, "            ▸ {}", sub_part.trim());
                                }
                            }
                        }
                    }
                }
            }
            logf!(&log_path, "   ✅ 级联推理完成: {} 个子动作已传导", chain_count);
        }

        results.push(serde_json::json!({
            "code": code, "name": name, "entity": result.entity_code,
            "current_position": { "lat": start_lat, "lon": start_lon, "alt": start_alt },
            "speed_m_s": speed,
            "waypoints": wps_raw,
            "total_distance_m": result.total_distance_m,
            "total_distance_km": result.total_distance_km,
            "total_time_s": result.total_time_s,
            "duration": result.duration,
            "priority": 3,
            "precondition": result.precondition,
            "effect": result.effect,
            "composedOf": result.composed_of,
            "cost": result.cost,
            "status": "executed",
        }));
    }

    (201, serde_json::json!({ "ok": true, "count": results.len(), "patrols": results }).to_string())
}

// ═══════════════════════════════════════════════════════════
// 图操作辅助
// ═══════════════════════════════════════════════════════════

/// 检查实体的前置条件是否满足
/// 解析 SWRL 格式的 precondition 字符串，
/// 提取 `swrlb:greaterThanOrEqual(?x.power, 150)` 等约束，
/// 对比实体实际属性值进行判断。
/// 返回 (是否通过, 失败原因)
fn check_preconditions(
    log_path: &str,
    entity: &Option<Node>,
    patrol: &serde_json::Value,
) -> (bool, String) {
    let entity_node = match entity {
        Some(n) => n,
        None => return (false, "实体不存在".into()),
    };

    // 合并检查 patrol JSON 中的 precondition 和实体自身的 precondition
    let patrol_precondition = patrol.get("precondition")
        .and_then(|v| v.as_str()).unwrap_or("");

    // 检查实体自身的 precondition 属性
    let entity_precondition = entity_node
        .property("precondition")
        .and_then(|v| v.as_str()).unwrap_or("");
    let entity_code = entity_node.property("code").and_then(|v| v.as_str()).unwrap_or("?");

    // 用于检查的 SWRL 字符串列表
    let checks = [
        ("巡逻任务", patrol_precondition.to_string()),
        ("实体", entity_precondition.to_string()),
    ];

    let mut failures = Vec::new();

    for (source, swrl) in &checks {
        if swrl.is_empty() { continue; }

        // 提取 swrlb:greaterThanOrEqual(variable.power, threshold) 约束
        if let Some(pos) = swrl.find("greaterThanOrEqual(") {
            let after = &swrl[pos + "greaterThanOrEqual(".len()..];
            if let Some(end) = after.find(')') {
                let args = &after[..end];
                let parts: Vec<&str> = args.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    // 第一部分是变量名（如 ?x.power），第二部分是阈值
                    let field_name = parts[0]
                        .trim_start_matches("?x.")
                        .trim_start_matches('?')
                        .replace(".power", "power")
                        .replace(".speed", "speed");
                    let threshold: f64 = match parts[1].parse() { Ok(v) => v, Err(_) => { logf!(&log_path, "   ⚠ SWRL阈值解析失败: '{}'", parts[1]); continue; } };

                    // 从实体上取实际值
                    let actual = prop_as_f64(entity_node.property(&field_name)).unwrap_or(0.0);

                    if actual < threshold {
                        failures.push(format!(
                            "[{}] {}: {}={} < {} 不满足",
                            source, field_name, entity_code, actual, threshold
                        ));
                    } else {
                        logf!(&log_path, "   ✅ [{}] {}: {}={} >= {} 满足",
                            source, field_name, entity_code, actual, threshold);
                    }
                }
            }
        }

        // 简化版：提取 swrlb:greaterThan(var, threshold)
        if let Some(pos) = swrl.find("greaterThan(") {
            let after = &swrl[pos + "greaterThan(".len()..];
            if let Some(end) = after.find(')') {
                let args = &after[..end];
                let parts: Vec<&str> = args.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    let field_name = parts[0]
                        .trim_start_matches("?x.")
                        .trim_start_matches('?')
                        .replace(".power", "power")
                        .replace(".speed", "speed");
                    let threshold: f64 = match parts[1].parse() { Ok(v) => v, Err(_) => { logf!(&log_path, "   ⚠ SWRL阈值解析失败: '{}'", parts[1]); continue; } };
                    let actual = prop_as_f64(entity_node.property(&field_name)).unwrap_or(0.0);

                    if actual <= threshold {
                        failures.push(format!(
                            "[{}] {}: {}={} <= {} 不满足",
                            source, field_name, entity_code, actual, threshold
                        ));
                    }
                }
            }
        }

        // 提取 swrlb:equal(var, value)
        if let Some(pos) = swrl.find("equal(") {
            let after = &swrl[pos + "equal(".len()..];
            if let Some(end) = after.find(')') {
                let args = &after[..end];
                let parts: Vec<&str> = args.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    let field_name = parts[0].trim_start_matches("?x.").trim_start_matches('?');
                    let expected = parts[1].trim_matches('\'').trim_matches('"');
                    let actual = entity_node.property(field_name)
                        .and_then(|v| v.as_str()).unwrap_or("");

                    if actual != expected {
                        failures.push(format!(
                            "[{}] {}: {}='{}' != '{}' 不满足",
                            source, field_name, entity_code, actual, expected
                        ));
                    }
                }
            }
        }
    }

    if failures.is_empty() {
        (true, String::new())
    } else {
        logf!(&log_path, "   ⛔ 前置条件失败:");
        for f in &failures {
            logf!(&log_path, "     {}", f);
        }
        (false, failures.join("; "))
    }
}

fn prop_as_f64(val: Option<&PropertyValue>) -> Option<f64> {
    match val {
        Some(PropertyValue::Float(f)) => Some(*f),
        Some(PropertyValue::Integer(i)) => Some(*i as f64),
        _ => None,
    }
}

fn get_float(node: &Node, key: &str, idx: usize) -> f64 {
    node.property(key)
        .and_then(|v| v.as_list())
        .and_then(|arr| arr.get(idx))
        .and_then(|v| match v {
            PropertyValue::Float(f) => Some(*f),
            PropertyValue::Integer(i) => Some(*i as f64),
            _ => None,
        })
        .unwrap_or(0.0)
}

fn find_entity(app: &crate::app::AppState, patrol_code: &str) -> Option<Node> {
    let all = app.repo.get_nodes_by_label("Entity").unwrap_or_default();

    // 1) 通过 [:移动] 关系自动匹配巡逻任务
    let mut candidates: Vec<&Node> = Vec::new();
    for n in &all {
        let ncode = n.property("code").and_then(|v| v.as_str()).unwrap_or("");
        let rels = app.repo.get_relationships(ncode, Some("移动")).unwrap_or_default();
        for r in &rels {
            if r.end_node_id == patrol_code {
                candidates.push(n);
                break;
            }
        }
    }

    if !candidates.is_empty() {
        // 有多个匹配时，选第一个有 speed 的
        for n in &candidates {
            if n.property("speed").is_some() {
                return Some((*n).clone());
            }
        }
        return Some(candidates[0].clone());
    }

    // 2) 无关系匹配 → 取第一个有 speed 属性的 Entity 作为默认载体
    for n in &all {
        if prop_as_f64(n.property("speed")).is_some() {
            return Some(n.clone());
        }
    }
    None
}

fn update_entity_position(
    app: &crate::app::AppState, entity: &Node,
    lat: f64, lon: f64, alt: f64, remaining_dur: i64,
) {
    let a_code = entity.property("code").and_then(|v| v.as_str()).unwrap_or("");
    let all = app.repo.get_nodes_by_label("Entity").unwrap_or_default();
    for n in &all {
        if n.property("code").and_then(|v| v.as_str()) == Some(a_code) {
            let mut new_props = n.properties.clone();
            new_props.insert("Space_abs".to_string(), PropertyValue::List(vec![
                PropertyValue::Float(lat),
                PropertyValue::Float(lon),
                PropertyValue::Float(-30.0),
                PropertyValue::Float(alt),
            ]));
            new_props.insert("duration".to_string(), PropertyValue::from(remaining_dur));
            let _ = app.repo.delete_node(a_code);
            let _ = app.repo.insert_node(&Node::new(n.labels.clone(), new_props));
            return;
        }
    }
}

// ═══════════════════════════════════════════════════════════
// GET 辅助
// ═══════════════════════════════════════════════════════════

fn node_to_patrol(n: &Node) -> serde_json::Value {
    let p = |k: &str| n.property(k);
    let waypoints: serde_json::Value =
        p("waypoints").and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(serde_json::Value::Array(vec![]));

    serde_json::json!({
        "id":           p("id").and_then(|v| v.as_str()).unwrap_or(""),
        "code":         p("code").and_then(|v| v.as_str()).unwrap_or(""),
        "name":         p("name").and_then(|v| v.as_str()).unwrap_or(""),
        "waypoints":    waypoints,
        "duration":     p("duration").and_then(|v| v.as_i64()).unwrap_or(0),
        "priority":     p("priority").and_then(|v| v.as_i64()).unwrap_or(0),
        "precondition": p("precondition").and_then(|v| v.as_str()).unwrap_or(""),
        "effect":       p("effect").and_then(|v| v.as_str()).unwrap_or(""),
        "cost":         p("cost").and_then(|v| v.as_str()).unwrap_or(""),
        "composedOf":   p("composedOf").and_then(|v| v.as_str()).unwrap_or(""),
        "status":       p("status").and_then(|v| v.as_str()).unwrap_or(""),
    })
}
