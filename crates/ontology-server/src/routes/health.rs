//! GET /health — 健康检查。

use crate::app::AppState;
use ontology_storage::mapper::unified_mapping;
use std::sync::{Arc, Mutex};

pub fn handle(state: &Arc<Mutex<AppState>>) -> (u16, String) {
    let app = match state.lock() {
        Ok(a) => a,
        Err(e) => return (500, json_err("Lock error", &e.to_string())),
    };

    let entity_count = app
        .repo
        .get_nodes_by_label(unified_mapping::ENTITY_LABEL)
        .map(|v| v.len())
        .unwrap_or(0);
    let type_count = app
        .repo
        .get_nodes_by_label(unified_mapping::TYPE_LABEL)
        .map(|v| v.len())
        .unwrap_or(0);

    let body = serde_json::json!({
        "status": "ok",
        "service": "ontology-server",
        "version": env!("CARGO_PKG_VERSION"),
        "backend": if cfg!(feature = "memgraph") { "memgraph" } else { "in-memory" },
        "counts": {
            "entities": entity_count,
            "types": type_count,
        },
    });
    (200, body.to_string())
}

fn json_err(kind: &str, msg: &str) -> String {
    format!(r#"{{"error": "{}: {}"}}"#, kind, msg.replace('"', "\\\""))
}
