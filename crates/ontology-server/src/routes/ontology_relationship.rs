//! POST /relationships/create — 大模型创建关系入口（HTTP 适配层）。
//!
//! 请求格式见 `ontology_storage::ontology::relationship` 模块文档。
//! 本文件仅做 HTTP ↔ 存储层调用转换，不包含业务逻辑。

use std::sync::{Arc, Mutex};

use ontology_storage::ontology::relationship;

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

    let rels = match payload.get("relationships").and_then(|v| v.as_array()) {
        Some(r) if !r.is_empty() => r,
        _ => return (400, json_error("Missing or empty 'relationships' array".into())),
    };

    let app = match state.lock() {
        Ok(a) => a,
        Err(e) => return (500, json_error(format!("Lock: {}", e))),
    };

    let repo = app.repo.as_ref();
    let mut results: Vec<serde_json::Value> = Vec::new();

    for (i, rel_spec) in rels.iter().enumerate() {
        match relationship::create_relationship(repo, rel_spec) {
            Ok(info) => results.push(info),
            Err(e) => {
                return (400, json_error(format!("Relationship[{}]: {}", i, e)));
            }
        }
    }

    let resp = serde_json::json!({
        "created": results.len(),
        "relationships": results,
    });
    (200, resp.to_string())
}
