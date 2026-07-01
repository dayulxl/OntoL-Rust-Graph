//! 内存属性图执行器。
//!
//! 直接操作 `InMemoryGraph`，实现 `GraphRepository` trait。

use std::sync::{Arc, Mutex};

use crate::error::StoreError;
use crate::mapper::graph::node::Node;
use crate::mapper::graph::relationship::Relationship;
use crate::mapper::graph::pattern::GraphPattern;
use crate::repository::graph_store::GraphRepository;
use crate::repository::transaction::Transaction;

use super::graph::InMemoryGraph;

/// 内存后端适配器
pub struct InMemoryAdapter {
    graph: Arc<Mutex<InMemoryGraph>>,
}

impl InMemoryAdapter {
    pub fn new() -> Self {
        Self {
            graph: Arc::new(Mutex::new(InMemoryGraph::new())),
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, InMemoryGraph>, StoreError> {
        self.graph.lock()
            .map_err(|e| StoreError::Transaction(format!("Mutex poisoned: {}", e)))
    }
}

impl Default for InMemoryAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphRepository for InMemoryAdapter {
    fn begin_transaction(&self) -> Result<Box<dyn Transaction>, StoreError> {
        Err(StoreError::Transaction(
            "InMemory adapter does not yet support transactions".into(),
        ))
    }

    fn get_node(&self, id: &str) -> Result<Option<Node>, StoreError> {
        let g = self.lock()?;
        Ok(g.get_node(id).cloned())
    }

    fn get_nodes_by_label(&self, label: &str) -> Result<Vec<Node>, StoreError> {
        let g = self.lock()?;
        let nodes: Vec<Node> = g
            .get_nodes_by_label(label)
            .into_iter()
            .cloned()
            .collect();
        Ok(nodes)
    }

    fn get_relationships(
        &self,
        node_id: &str,
        rel_type: Option<&str>,
    ) -> Result<Vec<Relationship>, StoreError> {
        let g = self.lock()?;
        let edges = g.outgoing_edges(node_id);
        let filtered: Vec<Relationship> = edges
            .into_iter()
            .filter(|r| rel_type.map_or(true, |rt| r.rel_type == rt))
            .cloned()
            .collect();
        Ok(filtered)
    }

    fn query_pattern(
        &self,
        pattern: &GraphPattern,
    ) -> Result<Vec<(Node, Vec<Relationship>, Node)>, StoreError> {
        let g = self.lock()?;

        let start_nodes: Vec<Node> = if let Some(ref label) = pattern.start.labels {
            g.get_nodes_by_label(label).into_iter().cloned().collect()
        } else {
            g.nodes.values().cloned().collect()
        };

        let mut results = Vec::new();
        for start_node in &start_nodes {
            if !pattern.start.properties.is_empty() {
                let matches = pattern.start.properties.iter().all(|(k, v)| {
                    start_node.property(k) == Some(v)
                });
                if !matches { continue; }
            }

            let start_iri = start_node.property("iri").and_then(|v| v.as_str());
            let Some(start_iri) = start_iri else { continue };

            let edges = g.outgoing_edges(start_iri);

            for edge in &edges {
                if let Some(ref rt) = pattern.relationship.rel_type {
                    if edge.rel_type != *rt { continue; }
                }

                if let Some(end_node) = g.get_node(&edge.end_node_id) {
                    if let Some(ref end_label) = pattern.end.labels {
                        if !end_node.has_label(end_label) { continue; }
                    }
                    if !pattern.end.properties.is_empty() {
                        let matches = pattern.end.properties.iter().all(|(k, v)| {
                            end_node.property(k) == Some(v)
                        });
                        if !matches { continue; }
                    }
                    results.push((
                        start_node.clone(),
                        vec![(*edge).clone()],
                        end_node.clone(),
                    ));
                }
            }
        }

        Ok(results)
    }

    fn insert_node(&self, node: &Node) -> Result<String, StoreError> {
        let mut g = self.lock()?;
        g.insert_node(node.clone())
    }

    fn insert_relationship(&self, rel: &Relationship) -> Result<(), StoreError> {
        let mut g = self.lock()?;
        g.insert_relationship(rel)
    }

    fn delete_node(&self, id: &str) -> Result<usize, StoreError> {
        let mut g = self.lock()?;
        Ok(g.remove_node(id).map(|_| 1).unwrap_or(0))
    }

    fn delete_relationship(&self, _id: &str) -> Result<usize, StoreError> {
        Ok(0)
    }
}
