//! GET|POST /strike — 打击决策推演接口。
//!
//! **POST** — 解析 JSON → 调用推理机 TimelineEngine 执行打击推演 → 回写结果到图存储。
//! **GET /strike?code=xxx** — 查询指定打击任务。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ontology_reasoner::timeline::{StrikeInput, TimelineEngine};
use ontology_storage::mapper::graph::node::Node;
use ontology_storage::mapper::graph::property::PropertyValue;
use ontology_storage::mapper::unified_mapping;

use super::super::server::json_error;
use crate::app::AppState;

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

fn handle_get(request: &mut tiny_http::Request, state: &Arc<Mutex<AppState>>) -> (u16, String) {
    let code = request
        .url()
        .split("code=")
        .nth(1)
        .unwrap_or("")
        .to_string();

    let app = match state.lock() {
        Ok(a) => a,
        Err(e) => return (500, json_error(e.to_string())),
    };
    let all = app
        .repo
        .get_nodes_by_label(unified_mapping::STRIKE_LABEL)
        .unwrap_or_default();

    if code.is_empty() {
        let list: Vec<serde_json::Value> = all.iter().map(node_to_strike).collect();
        return (
            200,
            serde_json::json!({ "count": list.len(), "strikes": list }).to_string(),
        );
    }
    for n in &all {
        if n.property("code").and_then(|v| v.as_str()) == Some(&code) {
            return (200, node_to_strike(n).to_string());
        }
    }
    (404, json_error(format!("Strike '{}' not found", code)))
}

// ═══════════════════════════════════════════════════════════
// POST — 解析 JSON → 调用推理机推演 → 写回图存储
// ═══════════════════════════════════════════════════════════

fn handle_post(request: &mut tiny_http::Request, state: &Arc<Mutex<AppState>>) -> (u16, String) {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        return (400, json_error("Failed to read body".into()));
    }
    let strikes: Vec<serde_json::Value> = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => return (400, json_error(format!("Invalid JSON: {}", e))),
    };

    let app = match state.lock() {
        Ok(a) => a,
        Err(e) => return (500, json_error(e.to_string())),
    };
    let mut results = Vec::new();
    let engine = TimelineEngine::new();

    let _ = std::fs::create_dir_all("logs");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let log_path = format!("logs/strike_{}.log", ts);

    for strike in &strikes {
        let code = strike.get("code").and_then(|v| v.as_str()).unwrap_or("");
        let name = strike.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let sid = strike.get("id").and_then(|v| v.as_str()).unwrap_or("");

        // ── 查找攻击方 ──
        let attacker_code = strike
            .get("attacker")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let attacker = find_entity(&app, attacker_code);
        let (a_code, a_lat, a_lon, a_alt, a_conf) = match &attacker {
            Some(n) => (
                n.property("code")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string(),
                get_float(n, "Space_abs", 0),
                get_float(n, "Space_abs", 1),
                get_float(n, "Space_abs", 3),
                prop_as_f64(n.property("confidence")).unwrap_or(0.8),
            ),
            None => (
                attacker_code.to_string(),
                strike
                    .get("attacker_lat")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0),
                strike
                    .get("attacker_lon")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0),
                strike
                    .get("attacker_alt")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0),
                strike
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.7),
            ),
        };

        // ── 查找目标 ──
        let target_code = strike.get("target").and_then(|v| v.as_str()).unwrap_or("");
        let target = find_entity(&app, target_code);
        let (t_code, t_lat, t_lon, t_depth) = match &target {
            Some(n) => (
                n.property("code")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string(),
                get_float(n, "Space_abs", 0),
                get_float(n, "Space_abs", 1),
                -get_float(n, "Space_abs", 2), // depth = -z
            ),
            None => (
                target_code.to_string(),
                strike
                    .get("target_lat")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0),
                strike
                    .get("target_lon")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0),
                strike
                    .get("target_depth")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0),
            ),
        };

        // ── 武器参数 ──
        let weapon_type = strike
            .get("weapon_type")
            .and_then(|v| v.as_str())
            .unwrap_or("鱼雷");
        let weapon_range = strike
            .get("weapon_range_m")
            .and_then(|v| v.as_f64())
            .unwrap_or(10_000.0);
        let confidence = strike
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(a_conf);

        logf!(&log_path, "🎯 打击任务: {} ({})", name, code);
        logf!(&log_path, "   攻击方: {}  target: {}", a_code, t_code);

        // ── 前置条件检查 ──
        let precondition_passed = check_preconditions(&log_path, &attacker, strike);
        if !precondition_passed.0 {
            logf!(&log_path, "   ⛔ 前置条件不满足: {}", precondition_passed.1);
            results.push(serde_json::json!({
                "code": code, "name": name,
                "attacker": a_code, "target": t_code,
                "status": "blocked", "reason": precondition_passed.1,
            }));
            continue;
        }

        // ── 调用推理机 ──
        let input = StrikeInput {
            strike_code: code.to_string(),
            strike_name: name.to_string(),
            strike_id: sid.to_string(),
            attacker_code: a_code.clone(),
            attacker_lat: a_lat,
            attacker_lon: a_lon,
            attacker_alt: a_alt,
            target_code: t_code.clone(),
            target_lat: t_lat,
            target_lon: t_lon,
            target_depth: t_depth,
            weapon_type: weapon_type.to_string(),
            weapon_range_m: weapon_range,
            confidence,
        };
        let result = engine.simulate_strike(&input);

        // ── 写回打击节点 ──
        let mut props = HashMap::new();
        props.insert("id".to_string(), PropertyValue::from(sid));
        props.insert("iri".to_string(), PropertyValue::from(code));
        props.insert("code".to_string(), PropertyValue::from(code));
        props.insert("name".to_string(), PropertyValue::from(name));
        props.insert("attacker".to_string(), PropertyValue::from(a_code.as_str()));
        props.insert("target".to_string(), PropertyValue::from(t_code.as_str()));
        props.insert("weapon_type".to_string(), PropertyValue::from(weapon_type));
        props.insert(
            "distance_m".to_string(),
            PropertyValue::Float(result.distance_m),
        );
        props.insert(
            "in_range".to_string(),
            PropertyValue::Boolean(result.in_range),
        );
        props.insert(
            "hit_probability".to_string(),
            PropertyValue::Float(result.hit_probability),
        );
        props.insert(
            "damage_level".to_string(),
            PropertyValue::from(result.damage_level.as_str()),
        );
        props.insert(
            "total_time_s".to_string(),
            PropertyValue::Float(result.total_time_s),
        );
        props.insert("duration".to_string(), PropertyValue::from(result.duration));
        props.insert(
            "precondition".to_string(),
            PropertyValue::from(result.precondition.as_str()),
        );
        props.insert(
            "effect".to_string(),
            PropertyValue::from(result.effect.as_str()),
        );
        props.insert(
            "cost".to_string(),
            PropertyValue::from(result.cost.as_str()),
        );
        props.insert(
            "composedOf".to_string(),
            PropertyValue::from(result.composed_of.as_str()),
        );
        props.insert("domain".to_string(), PropertyValue::from("StrikeWarfare"));
        props.insert("status".to_string(), PropertyValue::from("有效"));
        props.insert("update_time".to_string(), PropertyValue::from("now"));

        // 以 id 为技术锚点执行删除（id 为空则回退到 code）
        let delete_key = if sid.is_empty() { code } else { sid };
        let _ = app.repo.delete_node(delete_key);
        if let Err(e) = app.repo.insert_node(&Node::new(
            vec![unified_mapping::STRIKE_LABEL.to_string()],
            props,
        )) {
            return (500, json_error(e.to_string()));
        }

        results.push(serde_json::json!({
            "code": code, "name": name,
            "attacker": a_code, "target": t_code,
            "weapon_type": weapon_type,
            "attacker_position": { "lat": a_lat, "lon": a_lon, "alt": a_alt },
            "target_position": { "lat": t_lat, "lon": t_lon, "depth": t_depth },
            "distance_m": result.distance_m,
            "distance_km": format!("{:.2}", result.distance_m / 1000.0),
            "in_range": result.in_range,
            "hit_probability": result.hit_probability,
            "damage_level": result.damage_level,
            "total_time_s": result.total_time_s,
            "duration": result.duration,
            "precondition": result.precondition,
            "effect": result.effect,
            "composedOf": result.composed_of,
            "cost": result.cost,
            "status": "executed",
        }));
    }

    (
        201,
        serde_json::json!({ "ok": true, "count": results.len(), "strikes": results }).to_string(),
    )
}

// ═══════════════════════════════════════════════════════════
// 图操作辅助
// ═══════════════════════════════════════════════════════════

fn check_preconditions(
    log_path: &str,
    entity: &Option<Node>,
    strike: &serde_json::Value,
) -> (bool, String) {
    let entity_node = match entity {
        Some(n) => n,
        None => {
            // 没有找到实体时，不阻塞——由调用方用直接坐标
            return (true, String::new());
        }
    };

    let patrol_precondition = strike
        .get("precondition")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let entity_precondition = entity_node
        .property("precondition")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let entity_code = entity_node
        .property("code")
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    let checks = [
        ("打击任务", patrol_precondition.to_string()),
        ("实体", entity_precondition.to_string()),
    ];

    let mut failures = Vec::new();

    for (source, swrl) in &checks {
        if swrl.is_empty() {
            continue;
        }

        // greaterThanOrEqual(var, threshold)
        if let Some(pos) = swrl.find("greaterThanOrEqual(") {
            let after = &swrl[pos + "greaterThanOrEqual(".len()..];
            if let Some(end) = after.find(')') {
                let args = &after[..end];
                let parts: Vec<&str> = args.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    let field_name = parts[0].trim_start_matches("?x.").trim_start_matches('?');
                    let threshold: f64 = match parts[1].parse() {
                        Ok(v) => v,
                        Err(_) => {
                            logf!(&log_path, "   ⚠ SWRL阈值解析失败: '{}'", parts[1]);
                            continue;
                        }
                    };
                    let actual = prop_as_f64(entity_node.property(field_name)).unwrap_or(0.0);
                    if actual < threshold {
                        failures.push(format!(
                            "[{}] {}: {}={} < {} 不满足",
                            source, field_name, entity_code, actual, threshold
                        ));
                    }
                }
            }
        }

        // greaterThan(var, threshold)
        if let Some(pos) = swrl.find("greaterThan(") {
            let after = &swrl[pos + "greaterThan(".len()..];
            if let Some(end) = after.find(')') {
                let args = &after[..end];
                let parts: Vec<&str> = args.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    let field_name = parts[0].trim_start_matches("?x.").trim_start_matches('?');
                    let threshold: f64 = match parts[1].parse() {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let actual = prop_as_f64(entity_node.property(field_name)).unwrap_or(0.0);
                    if actual <= threshold {
                        failures.push(format!(
                            "[{}] {}: {}={} <= {} 不满足",
                            source, field_name, entity_code, actual, threshold
                        ));
                    }
                }
            }
        }

        // equal(var, value)
        if let Some(pos) = swrl.find("equal(") {
            let after = &swrl[pos + "equal(".len()..];
            if let Some(end) = after.find(')') {
                let args = &after[..end];
                let parts: Vec<&str> = args.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    let field_name = parts[0].trim_start_matches("?x.").trim_start_matches('?');
                    let expected = parts[1].trim_matches('\'').trim_matches('"');
                    let actual = entity_node
                        .property(field_name)
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
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

fn find_entity(app: &crate::app::AppState, code: &str) -> Option<Node> {
    if code.is_empty() {
        return None;
    }
    let all = app
        .repo
        .get_nodes_by_label(unified_mapping::ENTITY_LABEL)
        .unwrap_or_default();
    for n in &all {
        if n.property("code").and_then(|v| v.as_str()) == Some(code) {
            return Some(n.clone());
        }
    }
    None
}

// ═══════════════════════════════════════════════════════════
// GET 辅助
// ═══════════════════════════════════════════════════════════

fn node_to_strike(n: &Node) -> serde_json::Value {
    let p = |k: &str| n.property(k);
    serde_json::json!({
        "code":            p("code").and_then(|v| v.as_str()).unwrap_or(""),
        "name":            p("name").and_then(|v| v.as_str()).unwrap_or(""),
        "attacker":        p("attacker").and_then(|v| v.as_str()).unwrap_or(""),
        "target":          p("target").and_then(|v| v.as_str()).unwrap_or(""),
        "weapon_type":     p("weapon_type").and_then(|v| v.as_str()).unwrap_or(""),
        "distance_m":      prop_as_f64(p("distance_m")).unwrap_or(0.0),
        "in_range":        p("in_range").and_then(|v| v.as_bool()).unwrap_or(false),
        "hit_probability": prop_as_f64(p("hit_probability")).unwrap_or(0.0),
        "damage_level":    p("damage_level").and_then(|v| v.as_str()).unwrap_or(""),
        "total_time_s":    prop_as_f64(p("total_time_s")).unwrap_or(0.0),
        "duration":        p("duration").and_then(|v| v.as_i64()).unwrap_or(0),
        "precondition":    p("precondition").and_then(|v| v.as_str()).unwrap_or(""),
        "effect":          p("effect").and_then(|v| v.as_str()).unwrap_or(""),
        "cost":            p("cost").and_then(|v| v.as_str()).unwrap_or(""),
        "composedOf":      p("composedOf").and_then(|v| v.as_str()).unwrap_or(""),
        "status":          p("status").and_then(|v| v.as_str()).unwrap_or(""),
    })
}
