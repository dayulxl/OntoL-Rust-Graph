use std::sync::Arc;

use crate::error::StoreError;
use crate::mapper::graph::node::Node;
use crate::mapper::graph::pattern::GraphPattern;
use crate::mapper::graph::relationship::Relationship;
use crate::mapper::query_plan::{QueryPlan, QueryResult};
use crate::repository::transaction::Transaction;

/// 属性图存储的核心抽象 Trait。
///
/// 所有适配器（Memgraph 等）都必须实现此接口。
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

    /// 执行推理查询计划（默认实现委托到现有方法）。
    fn execute_plan(&self, plan: &QueryPlan) -> Result<QueryResult, StoreError> {
        use std::collections::HashMap;
        use crate::mapper::graph::pattern::{GraphPattern, NodePattern, RelationshipPattern};
        match plan {
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
                    start: NodePattern {
                        labels: start_label.clone(),
                        properties: HashMap::new(),
                        variable: Some("s".to_string()),
                    },
                    relationship: RelationshipPattern {
                        rel_type: rel_type.clone(),
                        properties: HashMap::new(),
                        variable: Some("r".to_string()),
                        outgoing: true,
                    },
                    end: NodePattern {
                        labels: end_label.clone(),
                        properties: props_map,
                        variable: Some("e".to_string()),
                    },
                };
                self.query_pattern(&pattern)
                    .map(QueryResult::PatternMatches)
            }
            _ => Err(StoreError::Query(
                format!("execute_plan: unsupported plan variant {:?}", plan)
            )),
        }
    }
}

/// 类型别名：线程安全的多态仓库句柄
pub type SharedRepository = Arc<dyn GraphRepository>;
