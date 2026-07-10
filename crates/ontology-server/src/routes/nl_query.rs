//! POST /nl-query — 自然语言关键词 → DWL2 查询。
//!
//! 请求: `{ "query": "find all P-8A in the area" }`
//! 响应: DWL2 检索结果

use super::super::server::json_error;
use crate::app::AppState;
use ontology_reasoner::ClassExpression;
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

    let query_text = q.get("query").and_then(|v| v.as_str()).unwrap_or("");
    if query_text.is_empty() {
        return (400, json_error("Missing 'query' field".into()));
    }

    let lower = query_text.to_lowercase();
    let app = match state.lock() {
        Ok(a) => a,
        Err(e) => return (500, json_error(e.to_string())),
    };

    // 关键词 → ClassExpression 映射
    let mut expression = ClassExpression::Top;
    let type_keywords: Vec<(&str, &str)> = vec![
        ("p-8a", "P-8A海神巡逻机"),
        ("海神", "P-8A海神巡逻机"),
        ("巡逻机", "P-8A海神巡逻机"),
        ("mh-60r", "MH-60R海鹰反潜直升机"),
        ("海鹰", "MH-60R海鹰反潜直升机"),
        ("直升机", "MH-60R海鹰反潜直升机"),
        ("mq-4c", "MQ-4C人鱼海神无人机"),
        ("无人机", "MQ-4C人鱼海神无人机"),
        ("尼米兹", "尼米兹级"),
        ("航母", "尼米兹级"),
        ("福特", "福特级"),
        ("驱逐舰", "阿利伯克级"),
        ("阿利伯克", "阿利伯克级"),
        ("巡洋舰", "提康德罗加级"),
        ("提康德罗加", "提康德罗加级"),
        ("补给舰", "供应级"),
        ("鱼雷", "Mk_54_轻型反潜鱼雷"),
        ("声纳", "主动声纳浮标"),
        ("雷达", "雷达"),
    ];

    let mut matched_types = Vec::new();
    for (keyword, typ) in &type_keywords {
        if lower.contains(keyword) {
            matched_types.push(typ.to_string());
        }
    }
    matched_types.sort();
    matched_types.dedup();

    if !matched_types.is_empty() {
        let mut types: Vec<ClassExpression> = matched_types
            .iter()
            .map(|t| ClassExpression::class(t.as_str()))
            .collect();
        let first = types.remove(0);
        expression = types.into_iter().fold(first, |acc, e| acc.or(e));
    }

    // 红蓝方过滤
    let command_side: Option<i64> = if lower.contains("蓝方") {
        Some(1)
    } else if lower.contains("红方") {
        Some(0)
    } else if lower.contains("中立") {
        Some(2)
    } else {
        None
    };

    // 执行查询
    match app.reasoner.query_instances(expression) {
        Ok(result) => {
            let mut entities: Vec<serde_json::Value> = Vec::new();
            for iri in &result.individuals {
                let all = app
                    .repo
                    .get_nodes_by_label(unified_mapping::ENTITY_LABEL)
                    .unwrap_or_default();
                for n in &all {
                    if n.property("code").and_then(|v| v.as_str()) == Some(iri.as_str()) {
                        let mut props = serde_json::Map::new();
                        props.insert(
                            "code".into(),
                            serde_json::Value::String(
                                n.property("code")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .into(),
                            ),
                        );
                        props.insert(
                            "name".into(),
                            serde_json::Value::String(
                                n.property("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .into(),
                            ),
                        );
                        props.insert(
                            "type".into(),
                            serde_json::Value::String(
                                n.property("type")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .into(),
                            ),
                        );
                        if let Some(cs) = n.property("command_side").and_then(|v| v.as_i64()) {
                            props.insert(
                                "command_side".into(),
                                serde_json::Value::Number(cs.into()),
                            );
                        }
                        entities.push(serde_json::Value::Object(props));
                        break;
                    }
                }
            }

            // 应用后处理过滤
            let filtered: Vec<_> = entities
                .into_iter()
                .filter(|e| {
                    let mut ok = true;
                    if let Some(side) = command_side
                        && e["command_side"].as_i64() != Some(side)
                    {
                        ok = false;
                    }
                    ok
                })
                .collect();

            let resp = serde_json::json!({
                "query": query_text,
                "expression_key": result.individuals.is_empty(),
                "count": filtered.len(),
                "entities": filtered,
                "elapsed_ms": result.elapsed_ms,
            });
            (200, resp.to_string())
        }
        Err(e) => (500, json_error(format!("Query error: {}", e))),
    }
}
