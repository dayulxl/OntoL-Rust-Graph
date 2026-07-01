//! 服务配置 — 通过环境变量注入。
//!
//! 环境变量清单:
//!   ONTOLOGY_PORT            — HTTP 端口 (默认 8085)
//!   ONTOLOGY_NEO4J_URI       — Neo4j 地址 (默认 http://localhost:7474)
//!   ONTOLOGY_NEO4J_USER      — Neo4j 用户名 (默认 neo4j)
//!   ONTOLOGY_NEO4J_PASSWORD  — Neo4j 密码 (必填，无默认)
//!   ONTOLOGY_MODE            — 默认推理策略 (Exercise/WarFighting/Training)
//!   RUST_LOG                 — 日志级别 (默认 info)

use ontology_reasoner::OperationMode;

#[allow(dead_code)]
pub struct ServerConfig {
    pub port: u16,
    pub neo4j_uri: String,
    pub neo4j_user: String,
    pub neo4j_password: String,
    pub default_mode: OperationMode,
    #[allow(dead_code)] pub log_level: String,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        Self {
            port: std::env::var("ONTOLOGY_PORT")
                .ok().and_then(|s| s.parse().ok()).unwrap_or(8085),
            neo4j_uri: std::env::var("ONTOLOGY_NEO4J_URI")
                .unwrap_or_else(|_| "http://localhost:7474".into()),
            neo4j_user: std::env::var("ONTOLOGY_NEO4J_USER")
                .unwrap_or_else(|_| "neo4j".into()),
            neo4j_password: std::env::var("ONTOLOGY_NEO4J_PASSWORD")
                .unwrap_or_else(|_| "12345678".into()),
            default_mode: std::env::var("ONTOLOGY_MODE").ok()
                .and_then(|s| OperationMode::from_str(&s))
                .unwrap_or(OperationMode::Exercise),
            log_level: std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        }
    }
}
