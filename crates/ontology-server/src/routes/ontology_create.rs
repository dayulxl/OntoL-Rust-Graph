//! POST /ontology/create — 大模型创建本体入口（HTTP 适配层）。
//!
//! 请求格式见 `ontology_storage::ontology::entity` 模块文档。
//! 本文件仅做 HTTP ↔ 存储层调用转换，不包含业务逻辑。

use std::sync::{Arc, Mutex};

use ontology_storage::ontology::entity;

use crate::app::AppState;
use super::super::server::json_error;

pub fn handle(
    request: &mut tiny_http::Request,
    state: &Arc<Mutex<AppState>>,
) -> (u16, String) {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        return (400, json_error("Failed to read body".into()));
    }

    let payload: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => return (400, json_error(format!("Invalid JSON: {}", e))),
    };

    let operations = match payload.get("operations").and_then(|v| v.as_array()) {
        Some(ops) if !ops.is_empty() => ops,
        _ => return (400, json_error("Missing or empty 'operations' array".into())),
    };

    let app = match state.lock() {
        Ok(a) => a,
        Err(e) => return (500, json_error(format!("Lock: {}", e))),
    };

    let repo = app.repo.as_ref();
    let mut results: Vec<serde_json::Value> = Vec::new();

    for (i, op) in operations.iter().enumerate() {
        let action = op.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let params = op.get("parameters").unwrap_or(&serde_json::Value::Null);

        let result = match action {
            "create_entity" => entity::create_entity(repo, params),
            "create_type" => entity::create_type(repo, params),
            "create_patrol" => entity::create_patrol(repo, params),
            "create_relationship" => ontology_storage::ontology::relationship::create_relationship(repo, params),
            other => Err(format!("Unknown action '{}' at index {}", other, i)),
        };

        match result {
            Ok(info) => results.push(info),
            Err(e) => {
                return (400, json_error(format!("Operation[{}] {}: {}", i, action, e)));
            }
        }
    }

    let resp = serde_json::json!({
        "created": results.len(),
        "results": results,
    });
    (200, resp.to_string())
}
