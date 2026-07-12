//! POST /infer-on-nodes-id-fc — 推理机流水线（流式长连接，按 id 查询）。
//!
//! 每层/每阶段推送 NDJSON 事件到客户端。
//!
//! 输入 JSON:
//! ```json
//! {
//!   "node_ids": ["550e8400-e29b-41d4-a716-446655440000", "..."],
//!   "confidence": 0.8,
//!   "cope_version": "550e8400-e29b-41d4-a716-446655440001"
//! }
//! ```

use std::io::{self, Read};
use std::sync::{Arc, Mutex, mpsc};

use ontology_reasoner::{InferenceEngine, InferenceRequest};

use crate::app::AppState;

/// mpsc::Receiver → Read 适配器，供 tiny_http::Response::new() 流式推送。
struct ChannelReader {
    rx: mpsc::Receiver<String>,
    buf: Vec<u8>,
    pos: usize,
}

impl Read for ChannelReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        // 当前缓冲区已读完 → 从 channel 读取下一行
        if self.pos >= self.buf.len() {
            match self.rx.recv() {
                Ok(line) => {
                    self.buf = line.into_bytes();
                    self.pos = 0;
                }
                Err(_) => return Ok(0), // channel 关闭 → EOF
            }
        }
        let remaining = &self.buf[self.pos..];
        let n = remaining.len().min(out.len());
        out[..n].copy_from_slice(&remaining[..n]);
        self.pos += n;
        Ok(n)
    }
}

pub fn handle_stream(
    mut request: tiny_http::Request,
    state: &Arc<Mutex<AppState>>,
) {
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        let resp = tiny_http::Response::from_string("{\"error\":\"Failed to read body\"}")
            .with_status_code(400);
        let _ = request.respond(resp);
        return;
    }

    let parsed: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            let resp = tiny_http::Response::from_string(
                format!("{{\"error\":\"Invalid JSON: {}\"}}", e)
            ).with_status_code(400);
            let _ = request.respond(resp);
            return;
        }
    };

    // ── 解析参数 ──
    let node_ids: Vec<String> = match parsed.get("node_ids").and_then(|v| v.as_array()) {
        Some(arr) => arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect(),
        None => {
            let resp = tiny_http::Response::from_string(
                "{\"error\":\"Missing required field: node_ids\"}"
            ).with_status_code(400);
            let _ = request.respond(resp);
            return;
        }
    };
    if node_ids.is_empty() {
        let resp = tiny_http::Response::from_string(
            "{\"error\":\"node_ids cannot be empty\"}"
        ).with_status_code(400);
        let _ = request.respond(resp);
        return;
    }

    let confidence = parsed.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.8).clamp(0.0, 1.0);
    let cope_version = parsed.get("cope_version").and_then(|v| v.as_str()).unwrap_or("default").to_string();

    // ── 创建事件通道 ──
    let (tx, rx) = mpsc::channel::<String>();

    // ── 在独立线程中执行推理 ──
    let repo = {
        let app = state.lock().unwrap();
        Arc::clone(app.reasoner.repo())
    };
    let policy = {
        let app = state.lock().unwrap();
        app.reasoner.policy().clone()
    };

    std::thread::spawn(move || {
        let mut engine = InferenceEngine::new(repo)
            .with_policy(policy)
            .with_event_channel(tx);

        let gie_request = InferenceRequest { node_ids, confidence, cope_version };
        if let Err(e) = engine.reason_on_nodes(gie_request) {
            eprintln!("GIE error: {}", e);
        }
        // tx 在此 drop → rx 收到 EOF
    });

    // ── 流式响应：ChannelReader 实现 Read，tiny_http 边读边发 ──
    let reader = ChannelReader { rx, buf: Vec::new(), pos: 0 };

    let resp = tiny_http::Response::new(
        200.into(),
        vec![
            "Content-Type: application/x-ndjson; charset=utf-8".parse().unwrap(),
            "Access-Control-Allow-Origin: *".parse().unwrap(),
        ],
        reader,
        None, // data_length = None → 流式
        None,
    );

    if let Err(e) = request.respond(resp) {
        eprintln!("Stream response error: {}", e);
    }
}
