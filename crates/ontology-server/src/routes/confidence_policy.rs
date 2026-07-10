//! POST /confidence/policy — 切换置信度策略。
//!
//! ```json
//! { "mode": "WarFighting" }
//! ```
//!
//! 可选: `threshold` 自定义阈值覆盖

use super::super::server::json_error;
use crate::app::AppState;
use ontology_reasoner::{ConfidencePolicy, OperationMode};
use std::str::FromStr;
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
        Err(e) => return (500, json_error(e.to_string())),
    };

    // 切换模式
    if let Some(mode_str) = q.get("mode").and_then(|v| v.as_str()) {
        match OperationMode::from_str(mode_str) {
            Ok(mode) => app.reasoner.switch_policy_mode(mode),
            Err(_) => {
                return (
                    400,
                    json_error(format!(
                        "Unknown mode: {}. Use: WarFighting, Training, Exercise",
                        mode_str
                    )),
                );
            }
        }
    }

    let policy = app.reasoner.policy();
    let resp = serde_json::json!({
        "ok": true,
        "policy": {
            "mode": policy_mode_to_str(policy),
            "threshold": policy.threshold(),
        }
    });
    (200, resp.to_string())
}

fn policy_mode_to_str(policy: &ConfidencePolicy) -> &str {
    match policy.mode {
        OperationMode::WarFighting => "WarFighting",
        OperationMode::Training => "Training",
        OperationMode::Exercise => "Exercise",
    }
}
