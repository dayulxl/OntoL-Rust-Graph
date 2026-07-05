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
                .db("memgraph") // Memgraph 默认数据库名为 "memgraph"，不是 Neo4j 的 "neo4j"
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
                result.next().await
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
            let mut stream = g.execute(neo4rs::query(&c)).await
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
        Err(StoreError::Transaction("Memgraph: not yet supported".into()))
    }

    fn get_node(&self, id: &str) -> Result<Option<Node>, StoreError> {
        let g = self.graph.clone();
        self.runtime.block_on(async move {
            let q = neo4rs::query(
                "MATCH (n) WHERE n.id = $id OR n.code = $id RETURN n LIMIT 1"
            ).param("id", id);
            let mut stream = g.execute(q).await
                .map_err(|e| StoreError::Query(format!("Memgraph: {}", e)))?;
            while let Ok(Some(row)) = stream.next().await {
                if let Ok(n) = row.get::<neo4rs::Node>("n") {
                    return Ok(Some(row_to_node(n)));
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
        &self, node_code: &str, rel_type: Option<&str>,
    ) -> Result<Vec<Relationship>, StoreError> {
        let g = self.graph.clone();
        let c = node_code.to_string();
        let cypher = match rel_type {
            Some(ref rt) => format!(
                "MATCH (n) WHERE n.id = $node_id OR n.code = $node_id \
                 MATCH (n)-[r:{}]->(m) RETURN r, m.id AS target", rt
            ),
            None => "MATCH (n) WHERE n.id = $node_id OR n.code = $node_id \
                     MATCH (n)-[r]->(m) RETURN r, m.id AS target".to_string(),
        };
        self.runtime.block_on(async move {
            let q = neo4rs::query(&cypher).param("node_id", c);
            let mut stream = g.execute(q).await
                .map_err(|e| StoreError::Query(format!("Memgraph: {}", e)))?;
            let mut rels = Vec::new();
            while let Ok(Some(row)) = stream.next().await {
                if let Ok(mg_rel) = row.get::<neo4rs::Relation>("r") {
                    let mut r = row_to_rel(mg_rel);
                    if let Ok(tgt) = row.get::<String>("target") {
                        r.end_node_id = tgt;
                    }
                    rels.push(r);
                }
            }
            Ok(rels)
        })
    }

    fn query_pattern(
        &self, pattern: &GraphPattern,
    ) -> Result<Vec<(Node, Vec<Relationship>, Node)>, StoreError> {
        let g = self.graph.clone();
        let cypher = build_match_clause(pattern);
        self.runtime.block_on(async move {
            let mut stream = g.execute(neo4rs::query(&cypher)).await
                .map_err(|e| StoreError::Query(format!("Memgraph pattern: {}", e)))?;
            let mut results = Vec::new();
            while let Ok(Some(row)) = stream.next().await {
                let s = row.get::<neo4rs::Node>("s");
                let e = row.get::<neo4rs::Node>("e");
                let r = row.get::<neo4rs::Relation>("r");
                if let (Ok(sn), Ok(en), Ok(rn)) = (s, e, r) {
                    results.push((
                        row_to_node(sn),
                        vec![row_to_rel(rn)],
                        row_to_node(en),
                    ));
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
        let mut set_parts = Vec::new();
        for key in node.properties.keys() {
            set_parts.push(format!("{}: ${}", key, key));
        }
        let props = if set_parts.is_empty() {
            String::new()
        } else {
            format!(" {{ {} }}", set_parts.join(", "))
        };
        let cypher = format!("CREATE (n{}{}) RETURN COALESCE(n.id, n.code) AS node_id", labels_str, props);
        let properties = node.properties.clone();
        self.runtime.block_on(async move {
            let mut q = neo4rs::query(&cypher);
            for (k, v) in &properties {
                q = bind_prop(q, k, v);
            }
            let mut stream = g.execute(q).await
                .map_err(|e| StoreError::Query(format!("Memgraph insert: {}", e)))?;
            while let Ok(Some(row)) = stream.next().await {
                if let Ok(node_id) = row.get::<String>("node_id") {
                    if !node_id.is_empty() {
                        return Ok(node_id);
                    }
                }
            }
            node.property("id")
                .or_else(|| node.property("code"))
                .and_then(|v| v.as_str()).map(String::from)
                .ok_or_else(|| StoreError::Query("insert_node: no id or code".into()))
        })
    }

    fn insert_relationship(&self, rel: &Relationship) -> Result<(), StoreError> {
        let g = self.graph.clone();
        let props_set = if rel.properties.is_empty() {
            String::new()
        } else {
            let parts: Vec<String> = rel.properties.keys()
                .map(|k| format!("{}: ${}", k, k))
                .collect();
            format!(" SET r = {{ {} }}", parts.join(", "))
        };
        let cypher = format!(
            "MATCH (a) WHERE a.id = $start_id OR a.code = $start_id \
             MATCH (b) WHERE b.id = $end_id OR b.code = $end_id \
             CREATE (a)-[r:{}]->(b){}",
            rel.rel_type, props_set
        );
        let start = rel.start_node_id.clone();
        let end = rel.end_node_id.clone();
        let rprops = rel.properties.clone();
        self.runtime.block_on(async move {
            let mut q = neo4rs::query(&cypher)
                .param("start_id", start)
                .param("end_id", end);
            for (k, v) in &rprops {
                q = bind_prop(q, k, v);
            }
            g.run(q).await.map_err(|e| StoreError::Query(format!("Memgraph insert rel: {}", e)))
        })
    }

    fn delete_node(&self, id: &str) -> Result<usize, StoreError> {
        let g = self.graph.clone();
        let node_id = id.to_string();
        self.runtime.block_on(async move {
            let q = neo4rs::query(
                "MATCH (n) WHERE n.id = $node_id OR n.code = $node_id DETACH DELETE n RETURN count(n) AS cnt"
            ).param("node_id", node_id);
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
        let val = node.get::<String>(key).map(PropertyValue::String)
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
        let val = rel.get::<String>(key).map(PropertyValue::String)
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

// ═══════════════════════════════════════════════════════════
// Cypher MATCH 子句构建（参数内联）
// ═══════════════════════════════════════════════════════════

fn build_match_clause(pattern: &GraphPattern) -> String {
    let start = build_node(&pattern.start, "s");
    let rel = build_rel(&pattern.relationship, "r");
    let end = build_node(&pattern.end, "e");
    format!("MATCH {}{}{} RETURN s AS s, r AS r, e AS e LIMIT 1000", start, rel, end)
}

fn build_node(np: &NodePattern, var: &str) -> String {
    let label = np.labels.as_deref().unwrap_or("");
    let label_str = if label.is_empty() { String::new() } else { format!(":{}", label) };
    if np.properties.is_empty() {
        format!("({}{})", var, label_str)
    } else {
        let parts: Vec<String> = np.properties.iter()
            .map(|(k, v)| format!("{}: {}", k, prop_literal(v)))
            .collect();
        format!("({}{} {{ {} }})", var, label_str, parts.join(", "))
    }
}

fn build_rel(rp: &RelationshipPattern, var: &str) -> String {
    let rt = rp.rel_type.as_deref().unwrap_or("");
    let type_str = if rt.is_empty() { String::new() } else { format!(":{}", rt) };
    let dir = if rp.outgoing { "->" } else { "<-" };
    if rp.properties.is_empty() {
        format!("-[{}{}]{}-", var, type_str, dir)
    } else {
        let parts: Vec<String> = rp.properties.iter()
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
