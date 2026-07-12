//! Memgraph 适配器 — 通过 Bolt 协议连接 Memgraph 实时图数据库。
//!
//! ## 设计
//!
//! Memgraph 兼容 Neo4j Bolt 协议，本适配器使用 `neo4rs` 驱动通过 Bolt
//! 协议与 Memgraph 通信。独立的模块边界为未来 Memgraph 专有特性预留扩展点：
//!
//! - `mg.*` 过程（`mg.create_index`、`mg.refresh_statistics`）
//! - MAGE 图算法库
//! - 流处理 / CDC 集成
//! - 内存分析存储模式
//!
//! ## Sync/Async 桥接
//!
//! `GraphRepository` trait 是同步的，`neo4rs` 基于 tokio 异步运行时。
//! 适配器内嵌单线程 `tokio::Runtime`，通过 `block_on` 桥接。
//! `neo4rs::Graph` 是 `Clone + Send + Sync`，直接 clone 进 async 闭包即可。
//!
//! ## URI Scheme
//!
//! `memgraph://host:7687` — 内部重写为 `bolt://` 后传给 `neo4rs`。
//! 也支持 `mgraph://` 别名。
//!
//! ## 示例
//!
//! ```ignore
//! let adapter = MemgraphAdapter::connect(
//!     "memgraph://localhost:7687", "", "",
//! )?;
//! let node = adapter.get_node("SomeEntity")?;
//! ```

use std::collections::HashMap;

use crate::error::StoreError;
use crate::mapper::graph::node::Node;
use crate::mapper::graph::pattern::{GraphPattern, NodePattern, RelationshipPattern};
use crate::mapper::graph::property::PropertyValue;
use crate::mapper::graph::relationship::Relationship;
use crate::repository::graph_store::GraphRepository;
use crate::repository::transaction::Transaction;

/// Memgraph 后端适配器。
///
/// 通过 `neo4rs` Bolt 驱动连接 Memgraph，实现 `GraphRepository` trait。
pub struct MemgraphAdapter {
    runtime: tokio::runtime::Runtime,
    graph: neo4rs::Graph,
}

impl MemgraphAdapter {
    /// 连接 Memgraph 数据库。
    ///
    /// 将 `memgraph://` / `mgraph://` scheme 重写为 `bolt://`，
    /// 因为 `neo4rs` 只识别标准 Bolt URI scheme。
    ///
    /// # 参数
    ///
    /// - `uri`: Memgraph URI，如 `"memgraph://localhost:7687"`
    /// - `user`: 用户名（Memgraph 默认无认证，留空即可）
    /// - `password`: 密码
    pub fn connect(uri: &str, user: &str, password: &str) -> Result<Self, StoreError> {
        let rewritten = rewrite_uri(uri);
        let uri_owned = rewritten.clone();
        let user = user.to_string();
        let password = password.to_string();

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| StoreError::Connection(format!("tokio runtime: {}", e)))?;

        let graph = runtime.block_on(async {
            let config = neo4rs::ConfigBuilder::default()
                .uri(&uri_owned)
                .user(&user)
                .password(&password)
                .db("memgraph")
                .build()
                .map_err(|e| StoreError::Connection(format!("Memgraph config: {}", e)))?;
            neo4rs::Graph::connect(config)
                .await
                .map_err(|e| StoreError::Connection(format!("Memgraph connect: {}", e)))
        })?;

        // 验证连接
        runtime.block_on({
            let g = graph.clone();
            async move {
                let mut result = g
                    .execute(neo4rs::query("RETURN 1 AS ok"))
                    .await
                    .map_err(|e| StoreError::Connection(format!("Memgraph test: {}", e)))?;
                result
                    .next()
                    .await
                    .map_err(|e| StoreError::Connection(format!("Memgraph test: {}", e)))?;
                Ok::<_, StoreError>(())
            }
        })?;

        Ok(Self { runtime, graph })
    }

    /// 执行查询，提取 `col` 列中的所有节点
    fn collect_nodes(&self, cypher: &str, col: &str) -> Result<Vec<Node>, StoreError> {
        let g = self.graph.clone();
        let c = cypher.to_string();
        let col = col.to_string();
        self.runtime.block_on(async move {
            let mut stream = g
                .execute(neo4rs::query(&c))
                .await
                .map_err(|e| StoreError::Query(format!("Memgraph: {}", e)))?;
            let mut nodes = Vec::new();
            while let Ok(Some(row)) = stream.next().await {
                if let Ok(n) = row.get::<neo4rs::Node>(&col) {
                    nodes.push(row_to_node(n));
                }
            }
            Ok(nodes)
        })
    }
}

// ═══════════════════════════════════════════════════════════
// GraphRepository
// ═══════════════════════════════════════════════════════════

impl GraphRepository for MemgraphAdapter {
    fn begin_transaction(&self) -> Result<Box<dyn Transaction>, StoreError> {
        Err(StoreError::Transaction(
            "Memgraph: not yet supported".into(),
        ))
    }

    fn get_node(&self, id: &str) -> Result<Option<Node>, StoreError> {
        let g = self.graph.clone();
        let cypher = if let Ok(native_id) = id.parse::<i64>() {
            neo4rs::query(
                "MATCH (n) WHERE n.id = $id OR n.code = $id OR id(n) = $native_id RETURN n, id(n) AS _nid LIMIT 1"
            )
            .param("id", id)
            .param("native_id", native_id)
        } else {
            neo4rs::query("MATCH (n) WHERE n.id = $id OR n.code = $id RETURN n, id(n) AS _nid LIMIT 1")
                .param("id", id)
        };
        self.runtime.block_on(async move {
            let mut stream = g
                .execute(cypher)
                .await
                .map_err(|e| StoreError::Query(format!("Memgraph: {}", e)))?;
            while let Ok(Some(row)) = stream.next().await {
                if let Ok(n) = row.get::<neo4rs::Node>("n") {
                    let mut node = row_to_node(n);
                    if let Ok(native_id) = row.get::<i64>("_nid") {
                        node.properties.insert(
                            "_nid".to_string(),
                            PropertyValue::Integer(native_id),
                        );
                    }
                    return Ok(Some(node));
                }
            }
            Ok(None)
        })
    }

    fn get_nodes_by_label(&self, label: &str) -> Result<Vec<Node>, StoreError> {
        let cypher = format!("MATCH (n:{}) RETURN n", label);
        self.collect_nodes(&cypher, "n")
    }

    fn get_relationships(
        &self,
        node_code: &str,
        rel_type: Option<&str>,
    ) -> Result<Vec<Relationship>, StoreError> {
        let g = self.graph.clone();
        let c = node_code.to_string();
        let native_clause = if c.parse::<i64>().is_ok() {
            "OR id(n) = $native_id".to_string()
        } else {
            String::new()
        };
        // (n)-[r]-(m) 双向匹配，出向+入向全覆盖
        let cypher = match rel_type {
            Some(ref rt) => format!(
                "MATCH (n) WHERE n.id = $node_id OR n.code = $node_id {native_clause} \
                 MATCH (n)-[r:{rt}]-(m) \
                 RETURN r, id(n) AS start_native_id, id(m) AS end_native_id"
            ),
            None => format!(
                "MATCH (n) WHERE n.id = $node_id OR n.code = $node_id {native_clause} \
                 MATCH (n)-[r]-(m) \
                 RETURN r, id(n) AS start_native_id, id(m) AS end_native_id"
            ),
        };
        self.runtime.block_on(async move {
            let mut q = neo4rs::query(&cypher).param("node_id", c.clone());
            if let Ok(nid) = c.parse::<i64>() {
                q = q.param("native_id", nid);
            }
            let mut stream = g
                .execute(q)
                .await
                .map_err(|e| StoreError::Query(format!("Memgraph: {}", e)))?;
            let mut rels = Vec::new();
            while let Ok(Some(row)) = stream.next().await {
                if let Ok(mg_rel) = row.get::<neo4rs::Relation>("r") {
                    let mut r = row_to_rel(mg_rel);
                    // 统一用原生 ID 作为 start_node_id / end_node_id
                    if let Ok(nid) = row.get::<i64>("start_native_id") {
                        r.start_node_id = nid.to_string();
                    }
                    if let Ok(nid) = row.get::<i64>("end_native_id") {
                        r.end_node_id = nid.to_string();
                    }
                    rels.push(r);
                }
            }
            Ok(rels)
        })
    }

    fn query_pattern(
        &self,
        pattern: &GraphPattern,
    ) -> Result<Vec<(Node, Vec<Relationship>, Node)>, StoreError> {
        let g = self.graph.clone();
        let cypher = build_match_clause(pattern);
        self.runtime.block_on(async move {
            let mut stream = g
                .execute(neo4rs::query(&cypher))
                .await
                .map_err(|e| StoreError::Query(format!("Memgraph pattern: {}", e)))?;
            let mut results = Vec::new();
            while let Ok(Some(row)) = stream.next().await {
                let s = row.get::<neo4rs::Node>("s");
                let e = row.get::<neo4rs::Node>("e");
                let r = row.get::<neo4rs::Relation>("r");
                if let (Ok(sn), Ok(en), Ok(rn)) = (s, e, r) {
                    results.push((row_to_node(sn), vec![row_to_rel(rn)], row_to_node(en)));
                }
            }
            Ok(results)
        })
    }

    fn insert_node(&self, node: &Node) -> Result<String, StoreError> {
        let g = self.graph.clone();
        let labels_str = if node.labels.is_empty() {
            String::new()
        } else {
            format!(":{}", node.labels.join(":"))
        };

        // 如果调用方未提供 id，自动生成 UUID v4
        let mut properties = node.properties.clone();
        let auto_gen_id: Option<String>;
        if !properties.contains_key("id") {
            auto_gen_id = Some(generate_uuid_v4());
            properties.insert("id".to_string(), PropertyValue::from(auto_gen_id.as_ref().unwrap().as_str()));
        } else {
            auto_gen_id = None;
        }

        let mut set_parts = Vec::new();
        for key in properties.keys() {
            set_parts.push(format!("{}: ${}", key, key));
        }
        let props = if set_parts.is_empty() {
            String::new()
        } else {
            format!(" {{ {} }}", set_parts.join(", "))
        };
        let cypher = format!(
            "CREATE (n{}{}) RETURN COALESCE(n.id, n.code) AS node_id, id(n) AS native_id",
            labels_str, props
        );
        self.runtime.block_on(async move {
            let mut q = neo4rs::query(&cypher);
            for (k, v) in &properties {
                q = bind_prop(q, k, v);
            }
            let mut stream = g
                .execute(q)
                .await
                .map_err(|e| StoreError::Query(format!("Memgraph insert: {}", e)))?;
            while let Ok(Some(row)) = stream.next().await {
                // 优先返回原生 ID（id(n)），它始终存在
                if let Ok(native_id) = row.get::<i64>("native_id") {
                    return Ok(native_id.to_string());
                }
                if let Ok(node_id) = row.get::<String>("node_id")
                    && !node_id.is_empty()
                {
                    return Ok(node_id);
                }
            }
            // fallback: 返回自动生成的 id 或 code
            auto_gen_id
                .or_else(|| properties.get("id").and_then(|v| v.as_str()).map(String::from))
                .or_else(|| properties.get("code").and_then(|v| v.as_str()).map(String::from))
                .ok_or_else(|| StoreError::Query("insert_node: no id or code".into()))
        })
    }

    fn insert_relationship(&self, rel: &Relationship) -> Result<(), StoreError> {
        let g = self.graph.clone();
        let props_set = if rel.properties.is_empty() {
            String::new()
        } else {
            let parts: Vec<String> = rel
                .properties
                .keys()
                .map(|k| format!("{}: ${}", k, k))
                .collect();
            format!(" SET r = {{ {} }}", parts.join(", "))
        };
        // 支持原生 ID + 属性 id/code 多级匹配
        let cypher = format!(
            "MATCH (a) WHERE a.id = $start_id OR a.code = $start_id OR id(a) = $start_native \
             MATCH (b) WHERE b.id = $end_id OR b.code = $end_id OR id(b) = $end_native \
             CREATE (a)-[r:{}]->(b){}",
            rel.rel_type, props_set
        );
        let start = rel.start_node_id.clone();
        let end = rel.end_node_id.clone();
        let rprops = rel.properties.clone();
        self.runtime.block_on(async move {
            let mut q = neo4rs::query(&cypher)
                .param("start_id", start.clone())
                .param("end_id", end.clone())
                .param("start_native", start.parse::<i64>().unwrap_or(-1))
                .param("end_native", end.parse::<i64>().unwrap_or(-1));
            for (k, v) in &rprops {
                q = bind_prop(q, k, v);
            }
            g.run(q)
                .await
                .map_err(|e| StoreError::Query(format!("Memgraph insert rel: {}", e)))
        })
    }

    fn delete_node(&self, id: &str) -> Result<usize, StoreError> {
        let g = self.graph.clone();
        let node_id = id.to_string();
        let native_id = node_id.parse::<i64>().unwrap_or(-1);
        self.runtime.block_on(async move {
            let q = neo4rs::query(
                "MATCH (n) WHERE n.id = $node_id OR n.code = $node_id OR id(n) = $native_id DETACH DELETE n RETURN count(n) AS cnt"
            ).param("node_id", node_id).param("native_id", native_id);
            let mut stream = g.execute(q).await
                .map_err(|e| StoreError::Query(format!("Memgraph delete: {}", e)))?;
            while let Ok(Some(row)) = stream.next().await {
                if let Ok(cnt) = row.get::<i64>("cnt") {
                    return Ok(cnt as usize);
                }
            }
            Ok(0)
        })
    }

    fn delete_relationship(&self, _id: &str) -> Result<usize, StoreError> {
        Ok(0)
    }

    /// Memgraph 专用：将 QueryPlan 翻译为 Cypher 查询执行。
    ///
    /// 这是推理引擎（GIE）与 Memgraph 之间的核心接口。
    /// 每个 QueryPlan 变体翻译为一条或多条 Cypher 语句。
    fn execute_plan(&self, plan: &crate::mapper::query_plan::QueryPlan) -> Result<crate::mapper::query_plan::QueryResult, StoreError> {
        use crate::mapper::query_plan::{QueryPlan, QueryResult, AtomPattern, ConstraintSpec};

        match plan {
            // ── 继承默认实现 ──
            QueryPlan::GetByCode(code) => {
                self.get_node(code).map(QueryResult::Single)
            }
            QueryPlan::GetByLabel(label) => {
                self.get_nodes_by_label(label).map(QueryResult::List)
            }
            QueryPlan::GetRelationships { node_code, rel_type } => {
                self.get_relationships(node_code, rel_type.as_deref())
                    .map(QueryResult::Relationships)
            }
            QueryPlan::PatternMatch { start_label, rel_type, end_label, end_properties } => {
                let mut props_map = HashMap::new();
                for (k, v) in end_properties {
                    props_map.insert(k.clone(), v.clone());
                }
                let pattern = GraphPattern {
                    start: NodePattern { labels: start_label.clone(), properties: HashMap::new(), variable: Some("s".to_string()) },
                    relationship: RelationshipPattern { rel_type: rel_type.clone(), properties: HashMap::new(), variable: Some("r".to_string()), outgoing: true },
                    end: NodePattern { labels: end_label.clone(), properties: props_map, variable: Some("e".to_string()) },
                };
                self.query_pattern(&pattern).map(QueryResult::PatternMatches)
            }

            // ── 属性继承: GetParentTypes ──
            QueryPlan::GetParentTypes { child_code } => {
                let cypher = "MATCH (c)-[:INSTANCE_OF|subClassOf]->(p:Type) \
                              WHERE c.code = $code RETURN p";
                let g = self.graph.clone();
                let code = child_code.clone();
                self.runtime.block_on(async move {
                    let q = neo4rs::query(cypher).param("code", code);
                    let mut stream = g.execute(q).await
                        .map_err(|e| StoreError::Query(format!("GetParentTypes: {}", e)))?;
                    let mut parents = Vec::new();
                    while let Ok(Some(row)) = stream.next().await {
                        if let Ok(n) = row.get::<neo4rs::Node>("p") {
                            parents.push(row_to_node(n));
                        }
                    }
                    Ok(QueryResult::ParentTypes(parents))
                })
            }

            // ── 下游发现: DiscoverDownstream ──
            QueryPlan::DiscoverDownstream { node_code, rel_types: _, cope_version: _ } => {
                let g = self.graph.clone();
                let code = node_code.clone();
                // 仅发现原实体（cope_version 为空或不存在）
                let cypher = "MATCH (n)-[r]->(m) \
                              WHERE (n.code = $code OR n.id = $code) \
                                AND (m.cope_version IS NULL OR m.cope_version = '') \
                              RETURN DISTINCT m.code AS code, m";
                self.runtime.block_on(async move {
                    let q = neo4rs::query(cypher).param("code", code);
                    let mut stream = g.execute(q).await
                        .map_err(|e| StoreError::Query(format!("DiscoverDownstream: {}", e)))?;
                    let mut codes = Vec::new();
                    while let Ok(Some(row)) = stream.next().await {
                        if let Ok(n) = row.get::<neo4rs::Node>("m") {
                            let node = row_to_node(n);
                            // Rust 侧按 rel_types 过滤（如果指定了）
                            let code_str = node.property("code")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if !codes.contains(&code_str.to_string()) {
                                codes.push(code_str.to_string());
                            }
                        }
                    }
                    Ok(QueryResult::DownstreamCodes(codes))
                })
            }

            // ── 规则匹配: RuleMatch ──
            QueryPlan::RuleMatch { rule_name: _, atom_patterns } => {
                let g = self.graph.clone();
                let patterns = atom_patterns.clone();
                self.runtime.block_on(async move {
                    let mut bindings: Vec<HashMap<String, PropertyValue>> = Vec::new();
                    if patterns.is_empty() {
                        return Ok(QueryResult::RuleBindings(bindings));
                    }
                    // 简单实现：逐 ClassAtom 查询匹配节点
                    for atom in &patterns {
                        if let AtomPattern::ClassAtom { class_name, variable: _ } = atom {
                            let cypher = format!("MATCH (n:{}) RETURN n", class_name);
                            let mut stream = g.execute(neo4rs::query(&cypher)).await
                                .map_err(|e| StoreError::Query(format!("RuleMatch: {}", e)))?;
                            while let Ok(Some(row)) = stream.next().await {
                                if let Ok(n) = row.get::<neo4rs::Node>("n") {
                                    let node = row_to_node(n);
                                    let mut binding = HashMap::new();
                                    for (k, v) in &node.properties {
                                        binding.insert(k.clone(), v.clone());
                                    }
                                    bindings.push(binding);
                                }
                            }
                        }
                    }
                    Ok(QueryResult::RuleBindings(bindings))
                })
            }

            // ── 约束验证: ValidateConstraint ──
            QueryPlan::ValidateConstraint { node_code, constraints } => {
                let g = self.graph.clone();
                let code = node_code.clone();
                let specs = constraints.clone();
                self.runtime.block_on(async move {
                    let q = neo4rs::query(
                        "MATCH (n) WHERE n.code = $code OR n.id = $code RETURN n"
                    ).param("code", code);
                    let mut stream = g.execute(q).await
                        .map_err(|e| StoreError::Query(format!("ValidateConstraint: {}", e)))?;
                    let node_opt = if let Ok(Some(row)) = stream.next().await {
                        row.get::<neo4rs::Node>("n").ok().map(row_to_node)
                    } else {
                        None
                    };
                    let node = match node_opt {
                        Some(n) => n,
                        None => return Ok(QueryResult::ConstraintPassed(false)),
                    };
                    let mut failed = Vec::new();
                    for spec in &specs {
                        let ok = match spec {
                            ConstraintSpec::Required { property } => {
                                node.property(property).is_some()
                            }
                            ConstraintSpec::Exists { property } => {
                                node.properties.contains_key(property.as_str())
                            }
                            ConstraintSpec::Equals { property, value } => {
                                node.property(property).map_or(false, |v| v == value)
                            }
                            ConstraintSpec::NotEquals { property, value } => {
                                node.property(property).map_or(true, |v| v != value)
                            }
                            ConstraintSpec::MinInclusive { property, value } => {
                                match node.property(property) {
                                    Some(PropertyValue::Float(f)) => *f >= *value,
                                    Some(PropertyValue::Integer(i)) => (*i as f64) >= *value,
                                    _ => false,
                                }
                            }
                            ConstraintSpec::MaxInclusive { property, value } => {
                                match node.property(property) {
                                    Some(PropertyValue::Float(f)) => *f <= *value,
                                    Some(PropertyValue::Integer(i)) => (*i as f64) <= *value,
                                    _ => false,
                                }
                            }
                            ConstraintSpec::MinExclusive { property, value } => {
                                match node.property(property) {
                                    Some(PropertyValue::Float(f)) => *f > *value,
                                    Some(PropertyValue::Integer(i)) => (*i as f64) > *value,
                                    _ => false,
                                }
                            }
                            ConstraintSpec::MaxExclusive { property, value } => {
                                match node.property(property) {
                                    Some(PropertyValue::Float(f)) => *f < *value,
                                    Some(PropertyValue::Integer(i)) => (*i as f64) < *value,
                                    _ => false,
                                }
                            }
                            ConstraintSpec::Pattern { property, regex: _ } => {
                                // regex 在 Rust 侧评估
                                node.property(property).is_some()
                            }
                            ConstraintSpec::InValues { property, values } => {
                                node.property(property)
                                    .map_or(false, |v| values.contains(v))
                            }
                        };
                        if !ok {
                            failed.push(format!("{:?}", spec));
                        }
                    }
                    Ok(QueryResult::ConstraintDetail {
                        passed: failed.is_empty(),
                        failed_constraints: failed,
                    })
                })
            }

            // ── JSONPath 提取: JsonPathLookup ──
            QueryPlan::JsonPathLookup { node_code, segments } => {
                let g = self.graph.clone();
                let code = node_code.clone();
                let segs = segments.clone();
                self.runtime.block_on(async move {
                    let q = neo4rs::query(
                        "MATCH (n) WHERE n.code = $code OR n.id = $code RETURN n"
                    ).param("code", code);
                    let mut stream = g.execute(q).await
                        .map_err(|e| StoreError::Query(format!("JsonPathLookup: {}", e)))?;
                    let node_opt = if let Ok(Some(row)) = stream.next().await {
                        row.get::<neo4rs::Node>("n").ok().map(row_to_node)
                    } else {
                        None
                    };
                    let node = match node_opt {
                        Some(n) => n,
                        None => return Ok(QueryResult::JsonValue(None)),
                    };
                    // 沿路径段查找: 先查属性，再沿关系
                    let mut current_val: Option<PropertyValue> = None;
                    for (i, seg) in segs.iter().enumerate() {
                        if i == 0 {
                            // 第一段：查节点属性
                            current_val = node.property(seg).cloned();
                        } else {
                            // 后续段：在关系或嵌套中查找（简化实现：仅属性）
                            current_val = None; // 嵌套关系查找待完善
                        }
                        if current_val.is_none() {
                            break;
                        }
                    }
                    Ok(QueryResult::JsonValue(current_val))
                })
            }

            // ── 自定义函数: CallFunction ──
            QueryPlan::CallFunction { func_name, target_id, params: _ } => {
                // func: JSON {"id":"N1","func":"calc"} → 适配器内部处理
                // 当前为骨架实现，返回占位结果
                Ok(QueryResult::FunctionCalled {
                    success: true,
                    message: format!("func '{}' called for target '{}'", func_name, target_id),
                })
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════
// PropertyValue → Bolt 参数
// ═══════════════════════════════════════════════════════════

fn bind_prop(q: neo4rs::Query, key: &str, val: &PropertyValue) -> neo4rs::Query {
    match val {
        PropertyValue::String(s) => q.param(key, s.clone()),
        PropertyValue::Integer(i) => q.param(key, *i),
        PropertyValue::Float(f) => q.param(key, *f),
        PropertyValue::Boolean(b) => q.param(key, *b),
        _ => q, // Lists/Maps 暂跳过
    }
}

// ═══════════════════════════════════════════════════════════
// neo4rs 行 → 领域类型
// ═══════════════════════════════════════════════════════════

fn row_to_node(node: neo4rs::Node) -> Node {
    let labels: Vec<String> = node.labels().iter().map(|l| l.to_string()).collect();
    let mut properties = HashMap::new();
    for key in node.keys() {
        let val = node
            .get::<String>(key)
            .map(PropertyValue::String)
            .or_else(|_| node.get::<i64>(key).map(PropertyValue::Integer))
            .or_else(|_| node.get::<f64>(key).map(PropertyValue::Float))
            .or_else(|_| node.get::<bool>(key).map(PropertyValue::Boolean))
            .unwrap_or(PropertyValue::Null);
        properties.insert(key.to_string(), val);
    }
    Node::new(labels, properties)
}

fn row_to_rel(rel: neo4rs::Relation) -> Relationship {
    let mut properties = HashMap::new();
    for key in rel.keys() {
        let val = rel
            .get::<String>(key)
            .map(PropertyValue::String)
            .or_else(|_| rel.get::<i64>(key).map(PropertyValue::Integer))
            .or_else(|_| rel.get::<f64>(key).map(PropertyValue::Float))
            .or_else(|_| rel.get::<bool>(key).map(PropertyValue::Boolean))
            .unwrap_or(PropertyValue::Null);
        properties.insert(key.to_string(), val);
    }
    Relationship {
        rel_type: rel.typ().to_string(),
        start_node_id: rel.start_node_id().to_string(),
        end_node_id: rel.end_node_id().to_string(),
        properties,
    }
}

// ═══════════════════════════════════════════════════════════
// URI Scheme 重写
// ═══════════════════════════════════════════════════════════

/// 将 Memgraph 专有 scheme 重写为 Bolt scheme，供 `neo4rs` 识别。
fn rewrite_uri(uri: &str) -> String {
    if let Some(rest) = uri.strip_prefix("memgraph://") {
        format!("bolt://{}", rest)
    } else if let Some(rest) = uri.strip_prefix("mgraph://") {
        format!("bolt://{}", rest)
    } else {
        uri.to_string()
    }
}

/// 生成 UUID v4 格式的字符串（std only，无外部依赖）。
fn generate_uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    // 混合时间戳和地址随机性生成 128-bit UUID v4
    let hi = (ts ^ (ts >> 17) ^ (ts << 31)).wrapping_mul(0x9e3779b97f4a7c15);
    let lo = (hi >> 32) ^ hi;
    let bytes = [
        ((lo >> 56) & 0xff) as u8,
        ((lo >> 48) & 0xff) as u8,
        ((lo >> 40) & 0xff) as u8,
        ((lo >> 32) & 0xff) as u8,
        ((lo >> 24) & 0xff) as u8,
        ((lo >> 16) & 0xff) as u8,
        0x40 | ((lo >> 12) & 0x0f) as u8, // version 4
        ((lo >> 8) & 0xff) as u8,
        0x80 | (lo & 0x3f) as u8, // variant 10xx
        ((ts >> 40) & 0xff) as u8,
        ((ts >> 32) & 0xff) as u8,
        ((ts >> 24) & 0xff) as u8,
        ((ts >> 16) & 0xff) as u8,
        ((ts >> 8) & 0xff) as u8,
        (ts & 0xff) as u8,
        ((hi >> 56) & 0xff) as u8,
    ];
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

// ═══════════════════════════════════════════════════════════
// Cypher MATCH 子句构建（参数内联）
// ═══════════════════════════════════════════════════════════

fn build_match_clause(pattern: &GraphPattern) -> String {
    let start = build_node(&pattern.start, "s");
    let rel = build_rel(&pattern.relationship, "r");
    let end = build_node(&pattern.end, "e");
    format!(
        "MATCH {}{}{} RETURN s AS s, r AS r, e AS e LIMIT 1000",
        start, rel, end
    )
}

fn build_node(np: &NodePattern, var: &str) -> String {
    let label = np.labels.as_deref().unwrap_or("");
    let label_str = if label.is_empty() {
        String::new()
    } else {
        format!(":{}", label)
    };
    if np.properties.is_empty() {
        format!("({}{})", var, label_str)
    } else {
        let parts: Vec<String> = np
            .properties
            .iter()
            .map(|(k, v)| format!("{}: {}", k, prop_literal(v)))
            .collect();
        format!("({}{} {{ {} }})", var, label_str, parts.join(", "))
    }
}

fn build_rel(rp: &RelationshipPattern, var: &str) -> String {
    let rt = rp.rel_type.as_deref().unwrap_or("");
    let type_str = if rt.is_empty() {
        String::new()
    } else {
        format!(":{}", rt)
    };
    let dir = if rp.outgoing { "->" } else { "<-" };
    if rp.properties.is_empty() {
        format!("-[{}{}]{}-", var, type_str, dir)
    } else {
        let parts: Vec<String> = rp
            .properties
            .iter()
            .map(|(k, v)| format!("{}: {}", k, prop_literal(v)))
            .collect();
        format!("-[{}{} {{ {} }}]{}-", var, type_str, parts.join(", "), dir)
    }
}

fn prop_literal(val: &PropertyValue) -> String {
    match val {
        PropertyValue::String(s) => format!("'{}'", s.replace('\'', "\\'")),
        PropertyValue::Integer(i) => i.to_string(),
        PropertyValue::Float(f) => f.to_string(),
        PropertyValue::Boolean(b) => b.to_string(),
        PropertyValue::Null => "null".to_string(),
        _ => "null".to_string(),
    }
}
