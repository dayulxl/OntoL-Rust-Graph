//! 运行时工厂 — 根据配置返回具体适配器实例。
//!
//! 业务层只需调用 `StorageConfig::build()` 即可获得
//! `Arc<dyn GraphRepository>`，不感知后端类型。

use crate::error::StoreError;
use crate::repository::graph_store::SharedRepository;
use std::sync::Arc;

/// 存储后端配置
#[derive(Debug, Clone)]
pub enum StorageConfig {
    /// Memgraph 后端（Bolt 协议，主力）
    #[cfg(feature = "memgraph")]
    Memgraph {
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
            #[cfg(feature = "memgraph")]
            StorageConfig::Memgraph {
                uri,
                user,
                password,
            } => {
                let adapter =
                    crate::adapters::memgraph::MemgraphAdapter::connect(&uri, &user, &password)?;
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
