//! 内存属性图存储（std-only 实现）。
//!
//! 使用 `HashMap` 存储节点（IRI → Node）和邻接表存储关系。
//! 外部通过 `Mutex` 包裹以保证线程安全。

use std::collections::HashMap;

use crate::error::StoreError;
use crate::mapper::graph::node::Node;
use crate::mapper::graph::relationship::Relationship;

/// 内存属性图的核心数据结构
#[derive(Debug, Default)]
pub struct InMemoryGraph {
    /// 节点映射：IRI → Node
    pub nodes: HashMap<String, Node>,

    /// 邻接表：起始节点 IRI → 目标节点 IRI → 关系列表
    pub adj_out: HashMap<String, HashMap<String, Vec<Relationship>>>,

    /// 逆邻接表：目标节点 IRI → 起始节点 IRI → 关系列表（支持入向查询）
    pub adj_in: HashMap<String, HashMap<String, Vec<Relationship>>>,
}

impl InMemoryGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            adj_out: HashMap::new(),
            adj_in: HashMap::new(),
        }
    }

    /// 插入节点
    pub fn insert_node(&mut self, node: Node) -> Result<String, StoreError> {
        let iri = node
            .property("iri")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(generate_id);

        self.nodes.insert(iri.clone(), node);
        Ok(iri)
    }

    /// 根据标识符获取节点（多级匹配，对标 Memgraph `WHERE n.id=$id OR n.code=$id`）。
    ///
    /// 1. 直接 HashMap 键查找（IRI / 内部 ID）
    /// 2. 扫描匹配 `id` / `iri` / `code` 属性
    pub fn get_node(&self, id: &str) -> Option<&Node> {
        // 直接键查找
        if let Some(n) = self.nodes.get(id) {
            return Some(n);
        }
        // 属性扫描（id/iri/code 三字段模糊匹配）
        self.nodes.values().find(|n| {
            n.property("id")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s == id)
                || n.property("iri")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| s == id)
                || n.property("code")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| s == id)
        })
    }

    /// 根据标签获取所有匹配节点
    pub fn get_nodes_by_label(&self, label: &str) -> Vec<&Node> {
        self.nodes.values().filter(|n| n.has_label(label)).collect()
    }

    /// 插入关系
    pub fn insert_relationship(&mut self, rel: &Relationship) -> Result<(), StoreError> {
        // 更新出向邻接表
        self.adj_out
            .entry(rel.start_node_id.clone())
            .or_default()
            .entry(rel.end_node_id.clone())
            .or_default()
            .push(rel.clone());

        // 更新入向邻接表
        self.adj_in
            .entry(rel.end_node_id.clone())
            .or_default()
            .entry(rel.start_node_id.clone())
            .or_default()
            .push(rel.clone());

        Ok(())
    }

    /// 获取从指定节点出发的所有出向关系
    pub fn outgoing_edges(&self, node_iri: &str) -> Vec<&Relationship> {
        self.adj_out
            .get(node_iri)
            .map(|targets| targets.values().flat_map(|v| v.iter()).collect())
            .unwrap_or_default()
    }

    /// 获取指向指定节点的所有入向关系
    pub fn incoming_edges(&self, node_iri: &str) -> Vec<&Relationship> {
        self.adj_in
            .get(node_iri)
            .map(|sources| sources.values().flat_map(|v| v.iter()).collect())
            .unwrap_or_default()
    }

    /// 根据关系类型过滤
    pub fn edges_by_type<'a>(edges: &[&'a Relationship], rel_type: &str) -> Vec<&'a Relationship> {
        edges
            .iter()
            .filter(|r| r.rel_type == rel_type)
            .copied()
            .collect()
    }

    /// 删除节点及其所有关联关系（多级标识符匹配）。
    pub fn remove_node(&mut self, id: &str) -> Option<Node> {
        // 先尝试直接键删除
        let removed = self.nodes.remove(id);
        if removed.is_some() {
            // 清理邻接表
            self.cleanup_edges(id);
            return removed;
        }
        // 属性扫描 — 找到匹配的节点后以其实际 IRI 键删除
        let actual_key = self
            .nodes
            .iter()
            .find(|(_, n)| {
                n.property("id")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| s == id)
                    || n.property("iri")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| s == id)
                    || n.property("code")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| s == id)
            })
            .map(|(k, _)| k.clone());
        if let Some(ref key) = actual_key {
            let removed = self.nodes.remove(key);
            if removed.is_some() {
                self.cleanup_edges(key);
            }
            return removed;
        }
        None
    }

    /// 清理与指定节点键关联的所有邻接表条目
    fn cleanup_edges(&mut self, node_key: &str) {
        self.adj_out.remove(node_key);
        self.adj_in.remove(node_key);
        for targets in self.adj_out.values_mut() {
            targets.remove(node_key);
        }
        for sources in self.adj_in.values_mut() {
            sources.remove(node_key);
        }
    }
}

/// 生成简易唯一 ID（时间戳 + 随机）
fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let ctr = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("urn:local:{:x}-{:x}", ts, ctr)
}
