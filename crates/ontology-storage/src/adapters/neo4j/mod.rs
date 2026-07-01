//! Neo4j 适配器 — 通过 HTTP API 连接 Neo4j 数据库，
//! 实现 `GraphRepository` trait。
//!
//! ## 同步设计
//!
//! `GraphRepository` trait 是同步的。本适配器使用 `ureq`（同步 HTTP 客户端）
//! 直接调用 Neo4j HTTP API，无需 tokio runtime 或 async 桥接。

pub mod executor;
pub mod schema;

use crate::error::StoreError;
use crate::mapper::graph::node::Node;
use crate::mapper::graph::relationship::Relationship;
use crate::mapper::graph::pattern::GraphPattern;
use crate::repository::graph_store::GraphRepository;
use crate::repository::transaction::Transaction;

use executor::Neo4jExecutor;

/// Neo4j 后端适配器。
///
/// 通过 HTTP API (`POST /db/neo4j/tx/commit`) 执行 Cypher 查询。
/// 同步接口，内部使用 ureq 阻塞式 HTTP 客户端。
pub struct Neo4jAdapter {
    executor: Neo4jExecutor,
}

impl Neo4jAdapter {
    /// 连接 Neo4j 并初始化 schema。
    ///
    /// # 参数
    ///
    /// - `uri`: Neo4j HTTP URI，如 `"http://localhost:7474"`
    ///   注意：这是 **HTTP** 端口，不是 Bolt 端口 7687
    /// - `user`: 用户名，如 `"neo4j"`
    /// - `password`: 密码
    pub fn connect(uri: &str, user: &str, password: &str) -> Result<Self, StoreError> {
        let executor = Neo4jExecutor::new(uri, user, password);

        // 验证连接
        let rows = executor.execute_cypher_raw("RETURN 1 AS ok", &serde_json::json!({}))?;
        if rows.is_empty() || rows[0].get("ok").and_then(|v| v.as_i64()) != Some(1) {
            return Err(StoreError::Connection(
                "Neo4j connection test failed".into(),
            ));
        }

        // 初始化索引
        schema::initialize_schema(&executor)?;

        Ok(Self { executor })
    }

    /// 获取内部 executor 引用（供高级用户直接执行 Cypher）
    pub fn executor(&self) -> &Neo4jExecutor {
        &self.executor
    }
}

impl GraphRepository for Neo4jAdapter {
    fn begin_transaction(&self) -> Result<Box<dyn Transaction>, StoreError> {
        Err(StoreError::Transaction(
            "Neo4j adapter uses auto-commit for each operation".into(),
        ))
    }

    fn get_node(&self, id: &str) -> Result<Option<Node>, StoreError> {
        self.executor.get_node(id)
    }

    fn get_nodes_by_label(&self, label: &str) -> Result<Vec<Node>, StoreError> {
        self.executor.get_nodes_by_label(label)
    }

    fn get_relationships(
        &self,
        node_id: &str,
        rel_type: Option<&str>,
    ) -> Result<Vec<Relationship>, StoreError> {
        self.executor.get_relationships(node_id, rel_type)
    }

    fn query_pattern(
        &self,
        pattern: &GraphPattern,
    ) -> Result<Vec<(Node, Vec<Relationship>, Node)>, StoreError> {
        self.executor.query_pattern(pattern)
    }

    fn insert_node(&self, node: &Node) -> Result<String, StoreError> {
        self.executor.create_node(node)
    }

    fn insert_relationship(&self, rel: &Relationship) -> Result<(), StoreError> {
        self.executor.create_relationship(rel)
    }

    fn delete_node(&self, id: &str) -> Result<usize, StoreError> {
        self.executor.delete_node(id)
    }

    fn delete_relationship(&self, _id: &str) -> Result<usize, StoreError> {
        // 关系删除需要端点和类型，仅 ID 无法精确定位
        log::warn!("delete_relationship by ID not supported; use endpoint-aware deletion");
        Ok(0)
    }
}
