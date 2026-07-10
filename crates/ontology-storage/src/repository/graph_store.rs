use std::sync::Arc;

use crate::error::StoreError;
use crate::mapper::graph::node::Node;
use crate::mapper::graph::pattern::GraphPattern;
use crate::mapper::graph::relationship::Relationship;
use crate::repository::transaction::Transaction;

/// 属性图存储的核心抽象 Trait。
///
/// 所有适配器（Neo4j、InMemory 等）都必须实现此接口。
/// 上层业务仅依赖此 trait，不感知底层存储类型。
pub trait GraphRepository: Send + Sync {
    /// 开启一个事务
    fn begin_transaction(&self) -> Result<Box<dyn Transaction>, StoreError>;

    /// 根据 ID 获取节点
    fn get_node(&self, id: &str) -> Result<Option<Node>, StoreError>;

    /// 根据标签查询所有匹配的节点
    fn get_nodes_by_label(&self, label: &str) -> Result<Vec<Node>, StoreError>;

    /// 获取与节点相关的关系（出方向，可指定关系类型）
    fn get_relationships(
        &self,
        node_id: &str,
        rel_type: Option<&str>,
    ) -> Result<Vec<Relationship>, StoreError>;

    /// 执行图模式匹配查询
    fn query_pattern(
        &self,
        pattern: &GraphPattern,
    ) -> Result<Vec<(Node, Vec<Relationship>, Node)>, StoreError>;

    /// 插入节点，返回内部 ID
    fn insert_node(&self, node: &Node) -> Result<String, StoreError>;

    /// 插入关系
    fn insert_relationship(&self, rel: &Relationship) -> Result<(), StoreError>;

    /// 删除节点及所有关联关系
    fn delete_node(&self, id: &str) -> Result<usize, StoreError>;

    /// 删除指定关系
    fn delete_relationship(&self, id: &str) -> Result<usize, StoreError>;
}

/// 类型别名：线程安全的多态仓库句柄
pub type SharedRepository = Arc<dyn GraphRepository>;
