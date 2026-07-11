//! HTTP 服务器 — 路由分发。
//!
//! 使用 `tiny_http` 作为 HTTP 层，每个请求在独立线程中处理，
//! 避免 AuraDB Bolt 慢查询阻塞其他请求。

use std::sync::{Arc, Mutex};

use tiny_http::{Header, Server};

use crate::app::AppState;
use crate::config::ServerConfig;
use crate::routes;

/// 启动 HTTP 服务器（每个请求独立线程，避免慢查询阻塞）
pub fn start(config: ServerConfig, state: Arc<Mutex<AppState>>) {
    let addr = format!("0.0.0.0:{}", config.port);
    let server = match Server::http(&addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Cannot start server: {}", e);
            return;
        }
    };

    println!("Available endpoints:");
    println!("   GET  http://localhost:{}/health", config.port);
    println!("   GET  http://localhost:{}/tools", config.port);
    println!("   GET  http://localhost:{}/schema", config.port);
    println!("   POST http://localhost:{}/query", config.port);
    println!("   POST http://localhost:{}/reason", config.port);
    println!("   POST http://localhost:{}/context", config.port);
    println!("   GET|POST http://localhost:{}/patrol", config.port);
    println!("   GET|POST http://localhost:{}/strike", config.port);
    println!("   POST http://localhost:{}/confidence/policy", config.port);
    println!("   POST http://localhost:{}/nl-query", config.port);
    println!("   POST http://localhost:{}/ontology/create", config.port);
    println!(
        "   POST http://localhost:{}/relationships/create",
        config.port
    );
    println!("   POST http://localhost:{}/tools/call", config.port);
    println!("   GET|POST http://localhost:{}/rules", config.port);
    println!("   POST http://localhost:{}/infer-forward", config.port);
    println!("   POST http://localhost:{}/infer-on-nodes", config.port);
    println!("   POST http://localhost:{}/entity/update", config.port);
    println!();

    // 每个请求独立线程处理，Bolt 慢查询不会阻塞健康检查等其他请求
    for mut request in server.incoming_requests() {
        let state = Arc::clone(&state);
        let url = request.url().to_string();
        let method_str = request.method().to_string();

        std::thread::spawn(move || {
            let (status, body) = dispatch(&method_str, &url, &mut request, &state);

            let content_type =
                if body.trim_start().starts_with('{') || body.trim_start().starts_with('[') {
                    "application/json; charset=utf-8"
                } else {
                    "text/plain; charset=utf-8"
                };

            let response = tiny_http::Response::from_string(&body)
                .with_status_code(status)
                .with_header(
                    Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap(),
                )
                .with_header(
                    Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
                );

            if let Err(e) = request.respond(response) {
                eprintln!("Response error: {}", e);
            }
        });
    }
}

/// 路由分发
fn dispatch(
    method: &str,
    path: &str,
    request: &mut tiny_http::Request,
    state: &Arc<Mutex<AppState>>,
) -> (u16, String) {
    if method == "OPTIONS" {
        return (204, String::new());
    }

    if path == "/patrol" || path.starts_with("/patrol?") {
        return routes::patrol::handle(request, state, method);
    }

    if path == "/strike" || path.starts_with("/strike?") {
        return routes::strike::handle(request, state, method);
    }

    if path == "/rules" || path.starts_with("/rules") {
        if method == "GET" {
            return routes::rules::handle_get(state);
        }
        if method == "POST" {
            return routes::rules::handle_post(request, state);
        }
    }

    if method == "GET" {
        match path {
            "/health" => return routes::health::handle(state),
            "/tools" => return routes::tools::handle(),
            "/schema" => return routes::schema::handle(state),
            _ => {}
        }
    }

    if method == "POST" {
        match path {
            "/query" => return routes::query::handle(request, state),
            "/reason" => return routes::reason::handle(request, state),
            "/context" => return routes::context::handle(request, state),
            "/confidence/policy" => return routes::confidence_policy::handle(request, state),
            "/nl-query" => return routes::nl_query::handle(request, state),
            "/ontology/create" => return routes::ontology_create::handle(request, state),
            "/relationships/create" => {
                return routes::ontology_relationship::handle(request, state);
            }
            "/infer-forward" => return routes::infer::handle(request, state),
            "/infer-on-nodes" => return routes::infer_on_nodes::handle(request, state),
            "/entity/update" => return routes::entity_update::handle(request, state),
            "/tools/call" => return routes::tools_call::handle(request, state),
            _ => {}
        }
    }

    let known = [
        "/health",
        "/tools",
        "/schema",
        "/query",
        "/reason",
        "/context",
        "/patrol",
        "/strike",
        "/confidence/policy",
        "/nl-query",
        "/ontology/create",
        "/relationships/create",
        "/rules",
        "/tools/call",
        "/infer-forward",
        "/infer-on-nodes",
        "/entity/update",
    ];
    if known.contains(&path) {
        let allowed = if matches!(path, "/health" | "/tools" | "/schema" | "/rules") {
            "GET"
        } else {
            "POST"
        };
        return (
            405,
            json_error(format!("Method not allowed. Use {}.", allowed)),
        );
    }

    (404, json_error(format!("Not found: {} {}", method, path)))
}

pub fn json_error(msg: String) -> String {
    format!(r#"{{"error": "{}"}}"#, msg.replace('"', "\\\""))
}
