//! 本体推理引擎 — 端到端演示。
//!
//! 默认连接 Memgraph 后端（通过环境变量配置），可切换为内存后端。
//!
//! 运行：
//!   cargo run                                                            # Memgraph 后端（默认）
//!   cargo run --no-default-features --features in-memory                 # 内存后端
//!
//! 环境变量：ONTOLOGY_GRAPH_URI / ONTOLOGY_GRAPH_USER / ONTOLOGY_GRAPH_PASSWORD

use std::collections::HashMap;
use std::sync::Arc;

use ontology_storage::mapper::graph::node::Node;
use ontology_storage::mapper::graph::property::PropertyValue;
use ontology_storage::mapper::graph::relationship::Relationship;
use ontology_storage::repository::graph_store::SharedRepository;

use ontology_reasoner::ReasonerBuilder;

/// 选择后端并返回 Arc<dyn GraphRepository>
///
/// Memgraph 连接参数从环境变量读取（与 ontology-server 一致）：
///   ONTOLOGY_GRAPH_URI      默认 memgraph://localhost:7687
///   ONTOLOGY_GRAPH_USER     默认空
///   ONTOLOGY_GRAPH_PASSWORD 默认空
fn create_repo() -> Result<SharedRepository, Box<dyn std::error::Error>> {
    #[cfg(feature = "memgraph")]
    {
        let uri = std::env::var("ONTOLOGY_GRAPH_URI")
            .unwrap_or_else(|_| "memgraph://localhost:7687".into());
        let user = std::env::var("ONTOLOGY_GRAPH_USER").unwrap_or_default();
        let password = std::env::var("ONTOLOGY_GRAPH_PASSWORD").unwrap_or_default();

        println!("🔌 连接 Memgraph ({} @ {})...", user, uri);
        let adapter =
            ontology_storage::adapters::memgraph::MemgraphAdapter::connect(&uri, &user, &password)?;
        println!("   ✅ Memgraph 连接成功\n");
        Ok(Arc::new(adapter))
    }

    #[cfg(not(feature = "memgraph"))]
    {
        println!("💾 使用内存后端");
        let adapter = ontology_storage::adapters::in_memory::executor::InMemoryAdapter::new();
        return Ok(Arc::new(adapter));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ontology_reasoner::logger::init();
    println!("╔══════════════════════════════════════════╗");
    println!("║  本体推理引擎 — SWRL 规则推理演示        ║");
    println!("╚══════════════════════════════════════════╝\n");

    let repo = create_repo()?;

    // ═══════════════════════════════════════════════════════
    // 1. 构建知识图谱
    // ═══════════════════════════════════════════════════════
    println!("📦 构建知识图谱...");

    // 本体类
    for (iri, label) in &[
        ("http://ex#Person", "Person"),
        ("http://ex#Parent", "Parent"),
        ("http://ex#Uncle", "Uncle"),
        ("http://ex#Brother", "Brother"),
        ("http://ex#Adult", "Adult"),
    ] {
        let mut props = HashMap::new();
        props.insert("iri".to_string(), PropertyValue::from(*iri));
        props.insert("label".to_string(), PropertyValue::from(*label));
        repo.insert_node(&Node::new(vec!["Class".to_string()], props))?;
    }

    // 个体
    for (iri, label) in &[
        ("http://ex#Alice", "Alice"),
        ("http://ex#Bob", "Bob"),
        ("http://ex#Charlie", "Charlie"),
        ("http://ex#Diana", "Diana"),
        ("http://ex#Eve", "Eve"),
    ] {
        let mut props = HashMap::new();
        props.insert("iri".to_string(), PropertyValue::from(*iri));
        props.insert("label".to_string(), PropertyValue::from(*label));
        repo.insert_node(&Node::new(vec!["Individual".to_string()], props))?;
    }

    // 类型断言
    for (ind, cls) in &[
        ("http://ex#Alice", "http://ex#Person"),
        ("http://ex#Bob", "http://ex#Person"),
        ("http://ex#Charlie", "http://ex#Person"),
        ("http://ex#Diana", "http://ex#Person"),
        ("http://ex#Eve", "http://ex#Person"),
    ] {
        repo.insert_relationship(&Relationship::simple(*ind, "INSTANCE_OF", *cls))?;
    }

    // 家族关系
    let family_edges = vec![
        ("http://ex#Alice", "hasParent", "http://ex#Bob"),
        ("http://ex#Bob", "hasBrother", "http://ex#Charlie"),
        ("http://ex#Eve", "hasParent", "http://ex#Diana"),
        ("http://ex#Diana", "hasBrother", "http://ex#Bob"),
    ];
    for (src, rel_type, dst) in &family_edges {
        repo.insert_relationship(&Relationship::simple(*src, *rel_type, *dst))?;
    }

    // 年龄属性
    let ages = vec![("http://ex#Alice", "25"), ("http://ex#Bob", "45")];
    for (ind, age) in &ages {
        let mut props = HashMap::new();
        props.insert("iri".to_string(), PropertyValue::from("http://ex#hasAge"));
        let age_node_iri = repo.insert_node(&Node::new(vec!["Property".to_string()], props))?;
        let mut rel_props = HashMap::new();
        rel_props.insert("value".to_string(), PropertyValue::from(*age));
        repo.insert_relationship(&Relationship::new(
            *ind,
            "HAS_VALUE",
            &age_node_iri,
            rel_props,
        ))?;
    }

    println!("   ✅ {} 实体已写入\n", 5 + 5 + 2);

    // ═══════════════════════════════════════════════════════
    // 2. 加载 SWRL 规则 + 执行推理
    // ═══════════════════════════════════════════════════════
    println!("🔧 加载 SWRL 推理规则...");

    let mut reasoner = ReasonerBuilder::new(repo)
        .with_max_iterations(10)
        .with_fuse_threshold(0.3)
        .with_verbose(true)
        .build();

    // hasParent(?y, ?x) ^ hasBrother(?x, ?z) → hasUncle(?y, ?z)
    reasoner.load_swrl_rule(
        "[uncleRule: hasParent(?y, ?x) ^ hasBrother(?x, ?z) -> hasUncle(?y, ?z)]",
    )?;

    // 互逆
    reasoner.load_swrl_rule("[brotherInverse: hasBrother(?x, ?y) -> hasBrother(?y, ?x)]")?;

    println!("   ✅ {} 条规则已加载\n", reasoner.rule_count());

    // ═══════════════════════════════════════════════════════
    // 3. 执行推理
    // ═══════════════════════════════════════════════════════
    println!("⚡ 执行推理...\n");
    let report = reasoner.reason()?;
    println!("{}", report);

    // ═══════════════════════════════════════════════════════
    // 4. 验证结果
    // ═══════════════════════════════════════════════════════
    println!("🔍 验证推理结果:");

    for (name, iri) in &[("Alice", "http://ex#Alice"), ("Eve", "http://ex#Eve")] {
        let expr = ontology_reasoner::ClassExpression::some(
            "hasUncle",
            ontology_reasoner::ClassExpression::class("http://ex#Person"),
        );
        let dl_result = reasoner.query_instances(expr)?;
        let has_uncle = dl_result.individuals.contains(&iri.to_string());
        if has_uncle {
            println!("   ✅ {} 推导出了 Uncle 关系", name);
        } else {
            println!("   ❌ {} 未推导出 Uncle 关系", name);
        }
    }

    println!("\n🎉 演示完成");
    Ok(())
}
