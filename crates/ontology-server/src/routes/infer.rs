//! POST /infer-forward — 向前推理接口。
//!
//! # 产品层 / 业务层隔离
//!
//! ```
//! ┌──────────────────────────────────────────────────┐
//! │  ontology-server (业务层)                         │
//! │  infer.rs                                        │
//! │  ├─ HTTP 请求/响应处理                            │
//! │  ├─ MilitaryStateChangeDetector (领域知识)         │
//! │  └─ JSON 序列化                                  │
//! └──────────────┬───────────────────────────────────┘
//!                │ 调用
//! ┌──────────────▼───────────────────────────────────┐
//! │  ontology-reasoner::graph (产品层 / 框架)          │
//! │  ├─ GraphExplorer (通用 BFS 图遍历)               │
//! │  ├─ StateChangeDetector trait (可插拔检测器)       │
//! │  └─ util (实体查找/关系汇总/类型层次/规则匹配)      │
//! └──────────────────────────────────────────────────┘
//! ```
//!
//! 输入 JSON: `{ "id": "实体编号", "name": "任务名", "relation": "关系名" }`
//! 可选: `depth` (默认 3, 最大 5), `direction` ("outgoing"|"incoming"|"both")

use std::sync::{Arc, Mutex};

use ontology_reasoner::graph::{
    Direction, ExploreConfig, ExploreHop, ExploreResult, GraphExplorer, StateChangeDetector,
    prop_as_f64, truncate_str,
};
use ontology_reasoner::spatial::haversine_m;
use ontology_storage::mapper::graph::node::Node;
use ontology_storage::mapper::graph::property::PropertyValue;

use super::super::server::json_error;
use crate::app::AppState;

// ═══════════════════════════════════════════════════════════
// HTTP 入口
// ═══════════════════════════════════════════════════════════

pub fn handle(request: &mut tiny_http::Request, state: &Arc<Mutex<AppState>>) -> (u16, String) {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        return (400, json_error("Failed to read body".into()));
    }

    let parsed: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => return (400, json_error(format!("Invalid JSON: {}", e))),
    };

    let requests: Vec<&serde_json::Value> = if parsed.is_array() {
        parsed.as_array().unwrap().iter().collect()
    } else {
        vec![&parsed]
    };

    let app = match state.lock() {
        Ok(a) => a,
        Err(e) => return (500, json_error(e.to_string())),
    };

    let repo = app.repo.clone();
    let explorer = GraphExplorer::new(repo);

    // 业务层检测器
    let detector = MilitaryStateChangeDetector;

    let mut results = Vec::new();

    for req in &requests {
        let id = req.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let code = req.get("code").and_then(|v| v.as_str());
        let name = req.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let relation = req.get("relation").and_then(|v| v.as_str()).unwrap_or("");

        if id.is_empty() && code.is_none() {
            results.push(serde_json::json!({"error": "至少需要 'id' 或 'code' 之一"}));
            continue;
        }
        if relation.is_empty() {
            results.push(serde_json::json!({"error": "Missing required field: relation"}));
            continue;
        }

        let cope_version = match req.get("cope_version").and_then(|v| v.as_str()) {
            Some(cv) => cv.to_string(),
            None => {
                results.push(serde_json::json!({"error": "Missing required field: cope_version"}));
                continue;
            }
        };

        let name = if name.is_empty() {
            "推理任务"
        } else {
            name
        };
        let depth = req
            .get("depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(3)
            .min(5) as usize;
        let direction = req
            .get("direction")
            .and_then(|v| v.as_str())
            .unwrap_or("outgoing");
        let confidence_threshold = req.get("confidence_threshold").and_then(|v| v.as_f64());

        let config = ExploreConfig {
            start_id: id.to_string(),
            start_code: code.map(|s| s.to_string()),
            relation: relation.to_string(),
            max_depth: depth,
            direction: direction.parse::<Direction>().unwrap_or_default(),
            confidence_threshold,
            cope_version: Some(cope_version),
        };

        let threshold = config.confidence_threshold;

        match explorer.explore(&config) {
            Ok(result) => {
                let json = build_response(&result, &detector, name, threshold);
                results.push(json);
            }
            Err(e) => {
                results.push(serde_json::json!({"error": e, "id": id}));
            }
        }
    }

    (
        200,
        serde_json::json!({ "ok": true, "count": results.len(), "results": results }).to_string(),
    )
}

// ═══════════════════════════════════════════════════════════
// 响应构建 — 将产品层 ExploreResult + StateChangeDetector → JSON
// ═══════════════════════════════════════════════════════════

fn build_response(
    result: &ExploreResult,
    detector: &dyn StateChangeDetector,
    name: &str,
    confidence_threshold: Option<f64>,
) -> serde_json::Value {
    let chain_json: Vec<serde_json::Value> = result
        .chain
        .iter()
        .map(|hop| hop_to_json(hop, detector))
        .collect();

    let state_change_count: usize = chain_json
        .iter()
        .map(|h| {
            h["inferred"]["state_changes"]
                .as_array()
                .map(|a| a.len())
                .unwrap_or(0)
        })
        .sum();
    let rule_match_count: usize = chain_json
        .iter()
        .map(|h| {
            h["inferred"]["matching_rules"]
                .as_array()
                .map(|a| a.len())
                .unwrap_or(0)
        })
        .sum();

    // 置信度统计
    let stopped_count = result.chain.iter().filter(|h| h.stop_propagation).count();
    let with_conf: Vec<f64> = result.chain.iter().filter_map(|h| h.confidence).collect();
    let avg_confidence = if with_conf.is_empty() {
        None
    } else {
        Some(with_conf.iter().sum::<f64>() / with_conf.len() as f64)
    };

    // 源实体的出/入关系摘要
    let source_outgoing: Vec<serde_json::Value> = result
        .source_outgoing
        .iter()
        .map(|rc| {
            serde_json::json!({
                "relation": rc.relation,
                "count": rc.count,
                "example_targets": rc.example_targets,
            })
        })
        .collect();
    let source_incoming: Vec<serde_json::Value> = result
        .source_incoming
        .iter()
        .map(|rc| {
            serde_json::json!({
                "relation": rc.relation,
                "count": rc.count,
                "example_targets": rc.example_targets,
            })
        })
        .collect();

    let entity_code = result
        .source
        .property("code")
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    let avg_str = match avg_confidence {
        Some(v) => format!("{:.2}", v),
        None => "N/A".into(),
    };

    let summary = format!(
        "{}: 实体 '{}' 沿 '{}' 关系 {} 方向遍历 {} 跳, 访问 {} 个新节点, 发现 {} 条状态变化, {} 条匹配规则。有置信度数据的节点平均: {}, 因低于阈值停止传播: {} 处。检测器: {}。",
        name,
        entity_code,
        result.relation,
        result.direction.as_str(),
        chain_json.len(),
        result.chain.len(),
        state_change_count,
        rule_match_count,
        avg_str,
        stopped_count,
        detector.name(),
    );

    serde_json::json!({
        "source": node_to_json(&result.source),
        "relation": result.relation,
        "direction": result.direction.as_str(),
        "name": name,
        "source_context": {
            "outgoing_relations": source_outgoing,
            "incoming_relations": source_incoming,
        },
        "chain": chain_json,
        "confidence_threshold": confidence_threshold,
        "stats": {
            "hops": result.chain.len(),
            "nodes_visited": result.chain.len(),
            "state_changes": state_change_count,
            "matching_rules": rule_match_count,
            "avg_confidence": avg_confidence.map(|v| format!("{:.2}", v)).unwrap_or("N/A".into()),
            "stop_propagation_count": stopped_count,
            "nodes_with_confidence": with_conf.len(),
        },
        "summary": summary,
    })
}

fn hop_to_json(hop: &ExploreHop, detector: &dyn StateChangeDetector) -> serde_json::Value {
    let state_changes = detector.detect_changes(
        hop.source_node.as_ref(),
        hop.target_node.as_ref(),
        &hop.rel_type,
    );

    let matching_rules: Vec<serde_json::Value> = hop
        .matching_rules
        .iter()
        .map(|rm| {
            serde_json::json!({
                "rule_name": rm.rule_name,
                "source_file": rm.source_file,
                "match_type": rm.match_type,
            })
        })
        .collect();

    let next_relations: Vec<serde_json::Value> = hop
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

    serde_json::json!({
        "hop": hop.hop,
        "direction": hop.direction,
        "source_id": hop.source_id,
        "target_id": hop.target_id,
        "rel_type": hop.rel_type,
        "target": hop.target_node.as_ref().map(node_to_json).unwrap_or(serde_json::json!({"id": hop.target_id})),
        "type_hierarchy": hop.target_type_chain,
        "confidence": hop.confidence.map(|v| serde_json::json!(v)).unwrap_or(serde_json::Value::Null),
        "stop_propagation": hop.stop_propagation,
        "inferred": {
            "state_changes": state_changes,
            "matching_rules": matching_rules,
            "next_relations": next_relations,
        },
    })
}

// ═══════════════════════════════════════════════════════════
// 业务层：军事 ASW 领域状态变化检测器
// ═══════════════════════════════════════════════════════════

/// 军事反潜战（ASW）领域的状态变化检测器。
///
/// 识别以下领域特定变化：
/// - `Space_abs` [lat, lon, ?, alt] → 位置/高度变化 + haversine 距离
/// - `status` → 状态转变
/// - `speed` → 速度变化
/// - `power` → 功率变化
/// - `confidence` → 置信度变化
/// - `composedOf` → 组合动作链传递
/// - 中文关系语义（移动/打击/子动作）
/// - 前置条件/效果解析
struct MilitaryStateChangeDetector;

impl StateChangeDetector for MilitaryStateChangeDetector {
    fn detect_changes(
        &self,
        source: Option<&Node>,
        target: Option<&Node>,
        relation: &str,
    ) -> Vec<String> {
        let mut changes = Vec::new();
        let (src, tgt) = match (source, target) {
            (Some(s), Some(t)) => (s, t),
            _ => return changes,
        };

        // ── 位置变化 (Space_abs: [lat, lon, ?, alt]) ──
        let src_pos = src.property("Space_abs").and_then(|v| v.as_list());
        let tgt_pos = tgt.property("Space_abs").and_then(|v| v.as_list());
        if let (Some(sp), Some(tp)) = (src_pos, tgt_pos) {
            let get_f = |v: &PropertyValue| -> f64 {
                match v {
                    PropertyValue::Float(f) => *f,
                    PropertyValue::Integer(i) => *i as f64,
                    _ => 0.0,
                }
            };
            if sp.len() >= 2 && tp.len() >= 2 {
                let dist_km =
                    haversine_m(get_f(&sp[0]), get_f(&sp[1]), get_f(&tp[0]), get_f(&tp[1]))
                        / 1000.0;
                if dist_km > 0.01 {
                    changes.push(format!(
                        "📍 位置移动: ({:.4}, {:.4}) → ({:.4}, {:.4}) 距离={:.2} km",
                        get_f(&sp[0]),
                        get_f(&sp[1]),
                        get_f(&tp[0]),
                        get_f(&tp[1]),
                        dist_km
                    ));
                }
            }
            if sp.len() >= 4 && tp.len() >= 4 {
                let src_alt = get_f(&sp[3]);
                let tgt_alt = get_f(&tp[3]);
                if (src_alt - tgt_alt).abs() > 1.0 {
                    changes.push(format!(
                        "↕ 高度变化: {:.0}m → {:.0}m (Δ{:.0}m)",
                        src_alt,
                        tgt_alt,
                        tgt_alt - src_alt
                    ));
                }
            }
        }

        // ── 状态变化 ──
        let src_status = src
            .property("status")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let tgt_status = tgt
            .property("status")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !src_status.is_empty() && !tgt_status.is_empty() && src_status != tgt_status {
            changes.push(format!("🔀 状态转变: '{}' → '{}'", src_status, tgt_status));
        }

        // ── 速度变化 ──
        if let (Some(ss), Some(ts)) = (
            prop_as_f64(src.property("speed")),
            prop_as_f64(tgt.property("speed")),
        ) && (ss - ts).abs() > 0.01
        {
            changes.push(format!(
                "🏃 速度变化: {:.1} → {:.1} m/s (Δ{:.1})",
                ss,
                ts,
                ts - ss
            ));
        }

        // ── 功率变化 ──
        if let (Some(sp), Some(tp)) = (
            prop_as_f64(src.property("power")),
            prop_as_f64(tgt.property("power")),
        ) && (sp - tp).abs() > 0.01
        {
            changes.push(format!(
                "⚡ 功率变化: {:.1} → {:.1} (Δ{:.1})",
                sp,
                tp,
                tp - sp
            ));
        }

        // ── 置信度变化 ──
        if let (Some(sc), Some(tc)) = (
            prop_as_f64(src.property("confidence")),
            prop_as_f64(tgt.property("confidence")),
        ) && (sc - tc).abs() > 0.001
        {
            changes.push(format!("🎯 置信度变化: {:.2} → {:.2}", sc, tc));
        }

        // ── composedOf 传递 ──
        let src_comp = src
            .property("composedOf")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let tgt_comp = tgt
            .property("composedOf")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !src_comp.is_empty() && !tgt_comp.is_empty() && src_comp != tgt_comp {
            changes.push(format!(
                "🔗 组合动作链传递: {} → {}",
                truncate_str(src_comp, 40),
                truncate_str(tgt_comp, 40)
            ));
        }

        // ── 关系语义推断 ──
        let src_code = src.property("code").and_then(|v| v.as_str()).unwrap_or("");
        let tgt_code = tgt.property("code").and_then(|v| v.as_str()).unwrap_or("");

        match relation {
            "移动" | "movement" | "movementTo" => {
                if !src_code.is_empty() && !tgt_code.is_empty() {
                    changes.push(format!(
                        "🧭 实体 {} 沿移动关系到达节点 {}",
                        src_code, tgt_code
                    ));
                }
            }
            "打击" | "strike" | "strikes" => {
                changes.push(format!("💥 实体 {} 对 {} 发起打击关系", src_code, tgt_code));
            }
            "子动作" | "subAction" | "composedOf" => {
                changes.push(format!(
                    "📋 组合动作链: {} 包含子动作 {}",
                    src_code, tgt_code
                ));
            }
            _ => {
                if !src_code.is_empty() && !tgt_code.is_empty() {
                    changes.push(format!("🔗 {} -[{}]-> {}", src_code, relation, tgt_code));
                }
            }
        }

        // ── 目标前置条件/效果 ──
        let tgt_pre = tgt
            .property("precondition")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let tgt_eff = tgt
            .property("effect")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !tgt_pre.is_empty() {
            changes.push(format!("📌 目标前置条件: {}", truncate_str(tgt_pre, 80)));
        }
        if !tgt_eff.is_empty() {
            changes.push(format!("✅ 目标预期效果: {}", truncate_str(tgt_eff, 80)));
        }

        changes
    }

    fn name(&self) -> &str {
        "军事ASW"
    }
}

// ═══════════════════════════════════════════════════════════
// JSON 序列化辅助（业务层特有 — HTTP 响应格式）
// ═══════════════════════════════════════════════════════════

fn node_to_json(n: &Node) -> serde_json::Value {
    let mut props = serde_json::Map::new();
    for (k, v) in &n.properties {
        props.insert(k.clone(), p2j(v));
    }
    let id = n
        .property("code")
        .and_then(|v| v.as_str())
        .or_else(|| n.property("iri").and_then(|v| v.as_str()))
        .unwrap_or("?");
    let name = n.property("name").and_then(|v| v.as_str()).unwrap_or(id);
    serde_json::json!({
        "id": id,
        "name": name,
        "labels": n.labels,
        "properties": props,
    })
}

fn p2j(v: &PropertyValue) -> serde_json::Value {
    match v {
        PropertyValue::String(s) => serde_json::Value::String(s.clone()),
        PropertyValue::Integer(i) => serde_json::Number::from(*i).into(),
        PropertyValue::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        PropertyValue::Boolean(b) => serde_json::Value::Bool(*b),
        PropertyValue::List(arr) => serde_json::Value::Array(arr.iter().map(p2j).collect()),
        PropertyValue::Map(m) => {
            serde_json::Value::Object(m.iter().map(|(k, v)| (k.clone(), p2j(v))).collect())
        }
        PropertyValue::Null => serde_json::Value::Null,
    }
}
