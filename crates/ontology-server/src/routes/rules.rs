//! POST /rules/reload — 从 Neo4j 热加载 SWRL 规则。
//!
//! GET /rules — 列出已加载的规则。

use super::super::server::json_error;
use crate::app::AppState;
use ontology_storage::mapper::unified_mapping;
use std::sync::{Arc, Mutex};

/// GET /rules — 列出已加载规则
pub fn handle_get(state: &Arc<Mutex<AppState>>) -> (u16, String) {
    let app = match state.lock() {
        Ok(a) => a,
        Err(e) => return (500, json_error(e.to_string())),
    };
    let rules: Vec<serde_json::Value> = app
        .reasoner
        .rules()
        .iter()
        .map(|r| {
            serde_json::json!({
                "name": r.name.as_deref().unwrap_or("<anonymous>"),
                "antecedent_count": r.antecedent.len(),
                "consequent_count": r.consequent.len(),
                "is_safe": r.is_safe(),
            })
        })
        .collect();

    (
        200,
        serde_json::json!({ "count": rules.len(), "rules": rules }).to_string(),
    )
}

/// POST /rules/reload — 从 Neo4j 重新加载 (:Rule) 节点中的 SWRL 规则
pub fn handle_post(
    _request: &mut tiny_http::Request,
    state: &Arc<Mutex<AppState>>,
) -> (u16, String) {
    let mut app = match state.lock() {
        Ok(a) => a,
        Err(e) => return (500, json_error(e.to_string())),
    };

    // 从 Neo4j 查询所有启用的 Rule 节点
    let rule_nodes = app
        .repo
        .get_nodes_by_label(unified_mapping::RULE_LABEL)
        .unwrap_or_default();
    let mut loaded = 0usize;

    for node in &rule_nodes {
        let text = node.property("text").and_then(|v| v.as_str()).unwrap_or("");
        let enabled = node
            .property("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if !enabled || text.is_empty() {
            continue;
        }

        match app.reasoner.load_swrl_rule(text) {
            Ok(_) => loaded += 1,
            Err(e) => eprintln!("   ⚠ Rule load error: {}", e),
        }
    }

    // 如果 Neo4j 中没有 Rule 节点，尝试从 rules/ 目录加载
    if loaded == 0
        && let Ok(entries) = std::fs::read_dir("rules")
    {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "swrl").unwrap_or(false)
                && let Ok(content) = std::fs::read_to_string(&path)
            {
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    match app.reasoner.load_swrl_rule(line) {
                        Ok(_) => loaded += 1,
                        Err(e) => eprintln!("   ⚠ File rule error ({}): {}", path.display(), e),
                    }
                }
            }
        }
    }

    let resp = serde_json::json!({
        "ok": true,
        "loaded": loaded,
        "total_rules": app.reasoner.rule_count(),
    });
    (200, resp.to_string())
}
