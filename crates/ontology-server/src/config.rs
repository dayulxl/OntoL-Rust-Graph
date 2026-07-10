//! 服务配置 — 通过环境变量注入。
//!
//! 环境变量清单:
//!   ONTOLOGY_PORT               — HTTP 端口 (默认 8085)
//!   ONTOLOGY_GRAPH_URI          — 图数据库地址 (默认 memgraph://localhost:7687)
//!   ONTOLOGY_GRAPH_USER         — 图数据库用户名 (默认空)
//!   ONTOLOGY_GRAPH_PASSWORD     — 图数据库密码 (默认空)
//!   ONTOLOGY_MODE               — 默认推理策略 (Exercise/WarFighting/Training)
//!   RUST_LOG                    — 日志级别 (默认 info)

use ontology_reasoner::OperationMode;
use std::str::FromStr;

#[allow(dead_code)]
pub struct ServerConfig {
    pub port: u16,
    pub graph_uri: String,
    pub graph_user: String,
    pub graph_password: String,
    pub default_mode: OperationMode,
    #[allow(dead_code)]
    pub log_level: String,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        Self {
            port: std::env::var("ONTOLOGY_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8085),
            graph_uri: std::env::var("ONTOLOGY_GRAPH_URI")
                .unwrap_or_else(|_| "memgraph://localhost:7687".into()),
            graph_user: std::env::var("ONTOLOGY_GRAPH_USER").unwrap_or_default(),
            graph_password: std::env::var("ONTOLOGY_GRAPH_PASSWORD").unwrap_or_default(),
            default_mode: std::env::var("ONTOLOGY_MODE")
                .ok()
                .and_then(|s| OperationMode::from_str(&s).ok())
                .unwrap_or(OperationMode::Exercise),
            log_level: std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        }
    }
}
