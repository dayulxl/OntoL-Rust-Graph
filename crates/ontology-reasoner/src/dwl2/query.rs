//! DWL2 DL 查询引擎 — AST → 图模式匹配。
//! 将 `ClassExpression` 编译为 `GraphPattern`，
//! 在 `GraphRepository` 上执行查询返回个体集合。

use std::collections::HashSet;
use std::time::Instant;

use ontology_storage::mapper::graph::pattern::{
    GraphPattern, NodePattern, RelationshipPattern,
};
use ontology_storage::mapper::graph::property::PropertyValue;
use ontology_storage::repository::graph_store::SharedRepository;

use crate::dwl2::ast::{ClassExpression, Dwl2Query, Dwl2Result, QueryType};
use crate::error::ReasonerError;

/// DWL2 DL 查询引擎
pub struct Dwl2QueryEngine {
    repo: SharedRepository,
}

impl Dwl2QueryEngine {
    pub fn new(repo: SharedRepository) -> Self {
        Self { repo }
    }

    /// 执行 DWL2 DL 查询
    pub fn execute(&self, query: &Dwl2Query) -> Result<Dwl2Result, ReasonerError> {
        let start = Instant::now();
        match &query.query_type {
            QueryType::RetrieveInstances => {
                let individuals = self.retrieve_instances(&query.expression)?;
                Ok(Dwl2Result {
                    individuals,
                    subsumption_holds: None,
                    elapsed_ms: start.elapsed().as_millis() as u64,
                })
            }
            QueryType::IsSubClassOf { sub_class, super_class } => {
                let sub = self.retrieve_instances(&ClassExpression::ClassName(sub_class.clone()))?;
                let sup = self.retrieve_instances(super_class)?;
                let holds = sub.iter().all(|i| sup.contains(i));
                Ok(Dwl2Result {
                    individuals: sub,
                    subsumption_holds: Some(holds),
                    elapsed_ms: start.elapsed().as_millis() as u64,
                })
            }
            QueryType::IsInstanceOf { individual_iri } => {
                let instances = self.retrieve_instances(&query.expression)?;
                let holds = instances.contains(individual_iri);
                Ok(Dwl2Result {
                    individuals: if holds { vec![individual_iri.clone()] } else { vec![] },
                    subsumption_holds: Some(holds),
                    elapsed_ms: start.elapsed().as_millis() as u64,
                })
            }
        }
    }

    /// 递归遍历 ClassExpression AST，返回满足条件的个体 IRI 集合
    pub fn retrieve_instances(&self, expr: &ClassExpression) -> Result<Vec<String>, ReasonerError> {
        match expr {
            ClassExpression::Top => self.get_all_individuals(),
            ClassExpression::Bottom => Ok(vec![]),
            ClassExpression::ClassName(name) => self.get_instances_of_class(name),

            ClassExpression::Intersection(left, right) => {
                let left_set: HashSet<String> = self.retrieve_instances(left)?.into_iter().collect();
                let right_set: HashSet<String> = self.retrieve_instances(right)?.into_iter().collect();
                Ok(left_set.intersection(&right_set).cloned().collect())
            }
            ClassExpression::Union(left, right) => {
                let left = self.retrieve_instances(left)?;
                let right = self.retrieve_instances(right)?;
                let union: HashSet<String> = left.into_iter().chain(right).collect();
                Ok(union.into_iter().collect())
            }
            ClassExpression::Complement(inner) => {
                let all = self.get_all_individuals()?;
                let inner_set: HashSet<String> = self.retrieve_instances(inner)?.into_iter().collect();
                Ok(all.into_iter().filter(|i| !inner_set.contains(i)).collect())
            }
            ClassExpression::SomeValuesFrom { property, filler } => {
                self.some_values_from(property, filler)
            }
            ClassExpression::AllValuesFrom { property, filler } => {
                self.all_values_from(property, filler)
            }
            ClassExpression::MinCardinality { n, property, filler } => {
                self.cardinality_filter(*n, property, filler, |cnt, n| cnt >= n)
            }
            ClassExpression::MaxCardinality { n, property, filler } => {
                self.cardinality_filter(*n, property, filler, |cnt, n| cnt <= n)
            }
            ClassExpression::ExactCardinality { n, property, filler } => {
                self.cardinality_filter(*n, property, filler, |cnt, n| cnt == n)
            }
            ClassExpression::OneOf(iris) => Ok(iris.clone()),
            ClassExpression::SelfRestriction(property) => self.self_restriction(property),
        }
    }

    // ── 图查询原语 ──

    fn get_all_individuals(&self) -> Result<Vec<String>, ReasonerError> {
        let nodes = self.repo.get_nodes_by_label("Individual")?;
        Ok(nodes.iter().filter_map(|n| n.property("iri").and_then(|v| v.as_str()).map(String::from)).collect())
    }

    fn get_instances_of_class(&self, class_iri: &str) -> Result<Vec<String>, ReasonerError> {
        let pattern = GraphPattern::new(
            NodePattern::with_label("Individual").with_variable("ind"),
            RelationshipPattern::with_type("INSTANCE_OF").with_variable("r"),
            NodePattern::with_label("Class")
                .with_variable("c")
                .with_property("iri", PropertyValue::from(class_iri)),
        );
        let results = self.repo.query_pattern(&pattern)?;
        Ok(results.iter().filter_map(|(ind, _, _)| {
            ind.property("iri").and_then(|v| v.as_str()).map(String::from)
        }).collect())
    }

    fn some_values_from(&self, property: &str, filler: &ClassExpression) -> Result<Vec<String>, ReasonerError> {
        let filler_set: HashSet<String> = self.retrieve_instances(filler)?.into_iter().collect();
        let pattern = GraphPattern::new(
            NodePattern::with_label("Individual").with_variable("ind"),
            RelationshipPattern::with_type(property).with_variable("r"),
            NodePattern::with_label("Individual").with_variable("target"),
        );
        let results = self.repo.query_pattern(&pattern)?;
        Ok(results.iter().filter_map(|(ind, _, target)| {
            let tiri = target.property("iri").and_then(|v| v.as_str())?;
            if filler_set.contains(tiri) {
                ind.property("iri").and_then(|v| v.as_str()).map(String::from)
            } else { None }
        }).collect())
    }

    fn all_values_from(&self, property: &str, filler: &ClassExpression) -> Result<Vec<String>, ReasonerError> {
        let filler_set: HashSet<String> = self.retrieve_instances(filler)?.into_iter().collect();
        let all = self.get_all_individuals()?;
        let mut result = Vec::new();
        for ind_iri in &all {
            let rels = self.repo.get_relationships(ind_iri, Some(property))?;
            if rels.iter().all(|r| filler_set.contains(r.end_node_id.as_str())) {
                result.push(ind_iri.clone());
            }
        }
        Ok(result)
    }

    fn cardinality_filter<F>(&self, n: u32, property: &str, filler: &ClassExpression, pred: F) -> Result<Vec<String>, ReasonerError>
    where F: Fn(usize, usize) -> bool {
        let filler_set: HashSet<String> = self.retrieve_instances(filler)?.into_iter().collect();
        let all = self.get_all_individuals()?;
        let mut result = Vec::new();
        for ind_iri in &all {
            let rels = self.repo.get_relationships(ind_iri, Some(property))?;
            let cnt = rels.iter().filter(|r| filler_set.contains(r.end_node_id.as_str())).count();
            if pred(cnt, n as usize) { result.push(ind_iri.clone()); }
        }
        Ok(result)
    }

    fn self_restriction(&self, property: &str) -> Result<Vec<String>, ReasonerError> {
        let all = self.get_all_individuals()?;
        let mut result = Vec::new();
        for ind_iri in &all {
            let rels = self.repo.get_relationships(ind_iri, Some(property))?;
            if rels.iter().any(|r| r.end_node_id == *ind_iri) { result.push(ind_iri.clone()); }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use ontology_storage::adapters::in_memory::executor::InMemoryAdapter;
    use ontology_storage::mapper::graph::node::Node;
    use ontology_storage::mapper::graph::property::PropertyValue;
    use ontology_storage::mapper::graph::relationship::Relationship;
    use ontology_storage::repository::graph_store::GraphRepository;

    use ontology_storage::repository::graph_store::SharedRepository;

    use crate::dwl2::ast::{ClassExpression, Dwl2Query, QueryType};
    use super::Dwl2QueryEngine;

    fn setup_test_repo() -> SharedRepository {
        let adapter = Arc::new(InMemoryAdapter::new());
        for iri in &["http://ex#Person", "http://ex#Animal"] {
            let mut props = std::collections::HashMap::new();
            props.insert("iri".to_string(), PropertyValue::from(*iri));
            adapter.insert_node(&Node::new(vec!["Class".to_string()], props)).unwrap();
        }
        for (iri, label) in &[("http://ex#Alice", "Alice"), ("http://ex#Bob", "Bob")] {
            let mut props = std::collections::HashMap::new();
            props.insert("iri".to_string(), PropertyValue::from(*iri));
            props.insert("label".to_string(), PropertyValue::from(*label));
            adapter.insert_node(&Node::new(vec!["Individual".to_string()], props)).unwrap();
        }
        adapter.insert_relationship(&Relationship::simple("http://ex#Alice", "INSTANCE_OF", "http://ex#Person")).unwrap();
        adapter.insert_relationship(&Relationship::simple("http://ex#Bob", "INSTANCE_OF", "http://ex#Animal")).unwrap();
        adapter
    }

    #[test]
    fn test_class_name_retrieval() {
        let engine = Dwl2QueryEngine::new(setup_test_repo());
        let result = engine.execute(&Dwl2Query {
            expression: ClassExpression::class("http://ex#Person"),
            query_type: QueryType::RetrieveInstances,
        }).unwrap();
        assert!(result.individuals.contains(&"http://ex#Alice".to_string()));
        assert!(!result.individuals.contains(&"http://ex#Bob".to_string()));
    }

    #[test]
    fn test_union_retrieval() {
        let engine = Dwl2QueryEngine::new(setup_test_repo());
        let result = engine.execute(&Dwl2Query {
            expression: ClassExpression::class("http://ex#Person").or(ClassExpression::class("http://ex#Animal")),
            query_type: QueryType::RetrieveInstances,
        }).unwrap();
        assert_eq!(result.individuals.len(), 2);
    }

    #[test]
    fn test_subclass_of_holds() {
        let engine = Dwl2QueryEngine::new(setup_test_repo());
        let result = engine.execute(&Dwl2Query {
            expression: ClassExpression::class("http://ex#Person"),
            query_type: QueryType::IsSubClassOf {
                sub_class: "http://ex#Person".into(),
                super_class: ClassExpression::class("http://ex#Person"),
            },
        }).unwrap();
        assert_eq!(result.subsumption_holds, Some(true));
    }
}
