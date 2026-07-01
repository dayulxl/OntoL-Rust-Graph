use crate::error::StoreError;
use crate::mapper::graph::node::Node;
use crate::mapper::graph::relationship::Relationship;

/// 事务抽象 — 支持 begin / commit / rollback 语义。
///
/// 对于不支持完整 ACID 的后端（如当前 in-memory 原型），
/// commit 为 no-op，rollback 丢弃缓冲。
pub trait Transaction {
    /// 在事务中插入节点
    fn insert_node(&mut self, node: &Node) -> Result<String, StoreError>;

    /// 在事务中插入关系
    fn insert_relationship(&mut self, rel: &Relationship) -> Result<(), StoreError>;

    /// 在事务中删除节点
    fn delete_node(&mut self, id: &str) -> Result<usize, StoreError>;

    /// 在事务中删除关系
    fn delete_relationship(&mut self, id: &str) -> Result<usize, StoreError>;

    /// 提交事务
    fn commit(self: Box<Self>) -> Result<(), StoreError>;

    /// 回滚事务
    fn rollback(self: Box<Self>) -> Result<(), StoreError>;
}
