//! POST /infer-on-nodes — 推理机流水线接口。
//!
//! 调用 `Reasoner::reason_on_nodes`，执行完整的推理流水线：
//!   1. 查找实体 → 2. 属性继承(OWL2/RDFS优先) → 3. 逐层副本+推理 → 4. 关系复制 → 5. SHACL校验
//!
//! 输入 JSON:
//! ```json
//! {
//!   "node_names": ["P8A_001", "DDG_101"],
//!   "expressions": ["swrl:hasEnemy(?x,?y)->alert(?x,?y)"],
//!   "cope_version": "v2.0",
//!   "max_iterations": 5
//! }
//! ```

use std::sync::{Arc, Mutex};

use ontology_reasoner::ReasonOnNodesRequest;

use super::super::server::json_error;
use crate::app::AppState;

pub fn handle(request: &mut tiny_http::Request, state: &Arc<Mutex<AppState>>) -> (u16, String) {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        return (400, json_error("Failed to read body".into()));
    }

    let parsed: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => return (400, json_error(format!("Invalid JSON: {}", e))),
    };

    let node_names: Vec<String> = match parsed.get("node_names") {
        Some(v) => match v.as_array() {
            Some(arr) => arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
            None => return (400, json_error("'node_names' must be an array".into())),
        },
        None => {
            return (
                400,
                json_error("Missing required field: 'node_names'".into()),
            );
        }
    };

    if node_names.is_empty() {
        return (400, json_error("'node_names' cannot be empty".into()));
    }

    let expressions: Vec<String> = parsed
        .get("expressions")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let cope_version = parsed
        .get("cope_version")
        .and_then(|v| v.as_str())
        .unwrap_or("default")
        .to_string();

    let max_iterations = parsed
        .get("max_iterations")
        .and_then(|v| v.as_u64())
        .unwrap_or(5)
        .clamp(1, 10) as usize;

    let request = ReasonOnNodesRequest {
        node_names,
        expressions,
        cope_version,
        max_iterations,
    };

    let mut app = match state.lock() {
        Ok(a) => a,
        Err(e) => return (500, json_error(e.to_string())),
    };

    match app.reasoner.reason_on_nodes(request) {
        Ok(report) => {
            let response = serde_json::json!({
                "ok": true,
                "cope_version": report.cope_version,
                "cloned_count": report.cloned_count,
                "iterations": report.iterations,
                "swrl_stats": {
                    "total_steps": report.swrl_stats.total_steps,
                    "total_derived": report.swrl_stats.total_derived,
                    "fuse_trips": report.swrl_stats.fuse_trips,
                },
                "dwl2_queries": report.dwl2_results.len(),
                "shacl_reports": report.shacl_reports.len(),
                "total_ms": report.total_ms,
            });
            (200, response.to_string())
        }
        Err(e) => (500, json_error(format!("reason_on_nodes failed: {}", e))),
    }
}
