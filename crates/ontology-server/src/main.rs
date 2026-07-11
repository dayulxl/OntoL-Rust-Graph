//! Ontology Server — HTTP API 入口 (v0.3)。

mod app;
mod config;
mod routes;
mod server;

use std::sync::{Arc, Mutex};

use ontology_reasoner::ReasonerBuilder;
use ontology_storage::adapters::in_memory::executor::InMemoryAdapter;
use ontology_storage::mapper::unified_mapping;

use app::AppState;

fn main() {
    let _ = dotenvy::dotenv(); // 加载 .env 文件（失败时静默忽略）
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
        .with_policy(ontology_reasoner::ConfidencePolicy::with_mode(
            cfg.default_mode,
        ))
        .build();

    println!(
        "   推理策略: {:?}, 阈值: {:.2}",
        cfg.default_mode,
        reasoner.policy().threshold()
    );

    let state = Arc::new(Mutex::new(AppState::new(reasoner)));

    // ── 3. 热加载图数据库中的 SWRL 规则 ──
    if let Ok(app) = state.lock() {
        let rule_nodes = app
            .repo
            .get_nodes_by_label(unified_mapping::RULE_LABEL)
            .unwrap_or_default();
        if !rule_nodes.is_empty() {
            println!("   找到 {} 条 Rule 规则节点，加载中...", rule_nodes.len());
        }
    }

    // ── 4. 启动 HTTP 服务 ──
    println!("\n🚀 启动 HTTP 服务: http://0.0.0.0:{}\n", cfg.port);
    server::start(cfg, state);
}

/// 根据 feature + config 选择后端。
///
/// memgraph:// → MemgraphAdapter（主力后端）
/// 连接失败 → 回退内存后端
fn create_repo(
    cfg: &config::ServerConfig,
) -> ontology_storage::repository::graph_store::SharedRepository {
    // ── Memgraph（主力后端）──
    #[cfg(feature = "memgraph")]
    {
        println!(
            "🔌 连接 Memgraph ({} @ {})...",
            cfg.graph_user, cfg.graph_uri
        );
        match ontology_storage::adapters::memgraph::MemgraphAdapter::connect(
            &cfg.graph_uri,
            &cfg.graph_user,
            &cfg.graph_password,
        ) {
            Ok(adapter) => {
                println!("   ✅ Memgraph 连接成功");
                return Arc::new(adapter);
            }
            Err(e) => {
                eprintln!("   ⚠ Memgraph 连接失败: {}。回退到内存后端。", e);
            }
        }
    }

    // ── 内存回退 ──
    println!("💾 使用内存后端");
    Arc::new(InMemoryAdapter::new())
}
