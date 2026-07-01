//! 运行时工厂 — 根据配置返回具体适配器实例。
//!
//! 业务层只需调用 `StorageConfig::build()` 即可获得
//! `Arc<dyn GraphRepository>`，不感知后端类型。

use std::sync::Arc;
use crate::error::StoreError;
use crate::repository::graph_store::SharedRepository;

/// 存储后端配置
#[derive(Debug, Clone)]
pub enum StorageConfig {
    /// Neo4j 后端
    #[cfg(feature = "neo4j")]
    Neo4j {
        uri: String,
        user: String,
        password: String,
    },

    /// 内存后端（开发 / 测试用）
    #[cfg(feature = "in-memory")]
    InMemory,
}

impl StorageConfig {
    /// 根据配置构建对应的 `SharedRepository`
    pub fn build(self) -> Result<SharedRepository, StoreError> {
        match self {
            #[cfg(feature = "neo4j")]
            StorageConfig::Neo4j { uri, user, password } => {
                let adapter = crate::adapters::neo4j::Neo4jAdapter::connect(
                    &uri, &user, &password,
                )?;
                Ok(Arc::new(adapter))
            }

            #[cfg(feature = "in-memory")]
            StorageConfig::InMemory => {
                let adapter = crate::adapters::in_memory::executor::InMemoryAdapter::new();
                Ok(Arc::new(adapter))
            }
        }
    }
}
