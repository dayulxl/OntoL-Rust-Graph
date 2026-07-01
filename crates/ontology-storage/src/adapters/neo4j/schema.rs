//! Neo4j 索引与约束管理。
//!
//! 在初始化时创建必要索引，加速 `(:Class { iri })` 等常用查询。

use crate::error::StoreError;
use super::executor::Neo4jExecutor;

/// 初始化本体存储所需的索引
pub fn initialize_schema(executor: &Neo4jExecutor) -> Result<(), StoreError> {
    let index_queries = [
        // Class 节点 iri 索引
        "CREATE INDEX class_iri IF NOT EXISTS FOR (n:Class) ON (n.iri)",
        // Individual 节点 iri 索引
        "CREATE INDEX individual_iri IF NOT EXISTS FOR (n:Individual) ON (n.iri)",
        // Property 节点 iri 索引
        "CREATE INDEX property_iri IF NOT EXISTS FOR (n:Property) ON (n.iri)",
    ];

    for cypher in &index_queries {
        match executor.execute_cypher_raw(cypher, &serde_json::json!({})) {
            Ok(_) => {}
            // 索引已存在时报错忽略
            Err(e) => {
                log::warn!("Schema index warning (may already exist): {}", e);
            }
        }
    }

    Ok(())
}
