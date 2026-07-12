//! HTTP 服务器 — 精简路由。
//!
//! 端点:
//!   GET  /health              健康检查
//!   GET  /tools               LLM Function Calling 工具定义
//!   POST /infer-on-nodes-id-fc   图内置ID向前推理链（流式 NDJSON）

use std::sync::{Arc, Mutex};

use tiny_http::{Header, Server};

use crate::app::AppState;
use crate::config::ServerConfig;
use crate::routes;

pub fn start(config: ServerConfig, state: Arc<Mutex<AppState>>) {
    let addr = format!("0.0.0.0:{}", config.port);
    let server = match Server::http(&addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Cannot start server: {}", e);
            return;
        }
    };

    println!("可用端点:");
    println!("   GET  http://localhost:{}/health", config.port);
    println!("   GET  http://localhost:{}/tools", config.port);
    println!("   POST http://localhost:{}/infer-on-nodes-id-fc  (图内置ID向前推理链)", config.port);
    println!();

    for mut request in server.incoming_requests() {
        let state = Arc::clone(&state);
        let url = request.url().to_string();
        let method_str = request.method().to_string();

        std::thread::spawn(move || {
            // CORS 预检
            if method_str == "OPTIONS" {
                let resp = tiny_http::Response::from_string("")
                    .with_status_code(204)
                    .with_header(
                        Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
                    );
                let _ = request.respond(resp);
                return;
            }

            // 流式端点
            if url == "/infer-on-nodes-id-fc" && method_str == "POST" {
                routes::infer_on_nodes::handle_stream(request, &state);
                return;
            }

            // 普通端点
            let (status, body) = match (method_str.as_str(), url.as_str()) {
                ("GET", "/health") => routes::health::handle(&state),
                ("GET", "/tools") => routes::tools::handle(),
                _ => (404, json_error(format!("Not found: {} {}", method_str, url))),
            };

            let content_type = if body.trim_start().starts_with('{') || body.trim_start().starts_with('[') {
                "application/json; charset=utf-8"
            } else {
                "text/plain; charset=utf-8"
            };

            let response = tiny_http::Response::from_string(&body)
                .with_status_code(status)
                .with_header(Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap())
                .with_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap());

            if let Err(e) = request.respond(response) {
                eprintln!("Response error: {}", e);
            }
        });
    }
}

fn json_error(msg: String) -> String {
    format!(r#"{{"error": "{}"}}"#, msg.replace('"', "\\\""))
}
