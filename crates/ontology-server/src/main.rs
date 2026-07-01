//! Ontology Server — HTTP API 入口 (v0.2)。

mod app;
mod config;
mod routes;
mod server;

use std::sync::{Arc, Mutex};

use ontology_reasoner::ReasonerBuilder;
use ontology_storage::adapters::in_memory::executor::InMemoryAdapter;

use app::AppState;

fn main() {
    ontology_reasoner::logger::init();
    let cfg = config::ServerConfig::from_env();

    println!("╔══════════════════════════════════════════╗");
    println!("║  Ontology Server — LLM API Gateway       ║");
    println!("╚══════════════════════════════════════════╝");
    println!();

    // ── 1. 初始化后端 ──
    let repo = create_repo(&cfg);
    println!();

    // ── 2. 构建推理器 ──
    let reasoner = ReasonerBuilder::new(repo)
        .with_max_iterations(50)
        .with_verbose(true)
        .with_policy(ontology_reasoner::ConfidencePolicy::with_mode(cfg.default_mode))
        .build();

    println!("   默认策略: {:?}, 阈值: {:.2}", cfg.default_mode, reasoner.policy().threshold());

    let state = Arc::new(Mutex::new(AppState::new(reasoner)));

    // ── 3. 热加载 Neo4j 中的 SWRL 规则 ──
    if let Ok(app) = state.lock() {
        let rule_nodes = app.repo.get_nodes_by_label("Rule").unwrap_or_default();
        if !rule_nodes.is_empty() {
            println!("   找到 {} 条 Neo4j 规则节点，加载中...", rule_nodes.len());
        }
    }
    // 延迟到首次 reason() 调用时加载

    // ── 4. 启动 HTTP 服务 ──
    println!("\n🚀 启动 HTTP 服务: http://0.0.0.0:{}\n", cfg.port);
    server::start(cfg, state);
}

/// 根据 feature + config 选择后端
fn create_repo(cfg: &config::ServerConfig) -> ontology_storage::repository::graph_store::SharedRepository {
    #[cfg(feature = "neo4j")]
    {
        println!("🔌 连接 Neo4j ({} @ {})...", cfg.neo4j_user, cfg.neo4j_uri);
        match ontology_storage::adapters::neo4j::Neo4jAdapter::connect(
            &cfg.neo4j_uri,
            &cfg.neo4j_user,
            &cfg.neo4j_password,
        ) {
            Ok(adapter) => {
                println!("   ✅ Neo4j 连接成功");
                return Arc::new(adapter);
            }
            Err(e) => {
                eprintln!("   ⚠ Neo4j 连接失败: {}。回退到内存后端。", e);
            }
        }
    }

    // 非 neo4j feature 时用内存后端
    #[cfg(not(feature = "neo4j"))]
    let _ = cfg;  // suppress unused warning

    println!("💾 使用内存后端");
    Arc::new(InMemoryAdapter::new())
}
