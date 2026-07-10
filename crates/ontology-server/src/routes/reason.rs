//! POST /reason — 触发 SWRL 规则推理。

use super::super::server::json_error;
use crate::app::AppState;
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

    let mut app = match state.lock() {
        Ok(a) => a,
        Err(e) => return (500, json_error(format!("Lock: {}", e))),
    };

    // 可选规则
    if let Some(rules) = q.get("rules").and_then(|v| v.as_array()) {
        for rule in rules {
            if let Some(text) = rule.as_str()
                && let Err(e) = app.reasoner.load_swrl_rule(text)
            {
                return (400, json_error(format!("Rule parse: {}", e)));
            }
        }
    }

    let incremental = q
        .get("incremental")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let result = if incremental {
        app.reasoner.reason_incremental()
    } else {
        app.reasoner.reason()
    };

    match result {
        Ok(report) => {
            let resp = serde_json::json!({
                "ok": true,
                "rules_loaded": report.rules_loaded,
                "total_steps": report.stats.total_steps,
                "derived_facts": report.stats.total_derived,
                "fuse_trips": report.stats.fuse_trips,
                "elapsed_ms": report.total_ms,
                "low_confidence": report.results.iter()
                    .filter(|r| r.confidence < 0.5)
                    .map(|r| serde_json::json!({
                        "rule": r.rule_name, "confidence": r.confidence
                    }))
                    .collect::<Vec<_>>(),
            });
            (200, resp.to_string())
        }
        Err(e) => {
            if let ontology_reasoner::ReasonerError::ConfidenceFuse {
                confidence,
                threshold,
                rule_name,
            } = &e
            {
                let resp = serde_json::json!({
                    "error": "confidence_fuse_tripped",
                    "confidence": confidence, "threshold": threshold, "rule_name": rule_name,
                    "detail": format!("{}", e),
                });
                return (422, resp.to_string());
            }
            (500, json_error(format!("{}", e)))
        }
    }
}
