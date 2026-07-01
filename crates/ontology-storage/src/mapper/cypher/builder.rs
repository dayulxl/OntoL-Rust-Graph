//! Cypher 查询构建器。
//!
//! 将 `GraphPattern` / `NodePattern` / `RelationshipPattern` 编译为
//! 参数化的 Cypher 字符串，配合 `params` 模块防止注入。

use crate::mapper::graph::pattern::{GraphPattern, NodePattern, RelationshipPattern};

/// 将 NodePattern 编译为 Cypher 节点匹配片段
///
/// 示例：`(n:Class { iri: $iri_0 })`
pub fn build_node_match(pattern: &NodePattern) -> String {
    let var = pattern.variable.as_deref().unwrap_or("n");
    let label = pattern
        .labels
        .as_ref()
        .map(|l| format!(":{}", l))
        .unwrap_or_default();

    if pattern.properties.is_empty() {
        format!("({}{})", var, label)
    } else {
        let props: Vec<String> = pattern
            .properties
            .keys()
            .enumerate()
            .map(|(i, key)| format!("{}: ${}_{}", key, var, i))
            .collect();
        format!("({}{} {{ {} }})", var, label, props.join(", "))
    }
}

/// 将 RelationshipPattern 编译为 Cypher 关系片段
///
/// 示例：`-[r:HAS_PROPERTY { transitive: $r_0 }]->`
pub fn build_rel_match(pattern: &RelationshipPattern) -> String {
    let var = pattern.variable.as_deref().unwrap_or("r");
    let rel_type = pattern
        .rel_type
        .as_ref()
        .map(|t| format!(":{}", t))
        .unwrap_or_default();

    let props_str = if pattern.properties.is_empty() {
        String::new()
    } else {
        let props: Vec<String> = pattern
            .properties
            .keys()
            .enumerate()
            .map(|(i, key)| format!("{}: ${}_{}", key, var, i))
            .collect();
        format!(" {{ {} }}", props.join(", "))
    };

    if pattern.outgoing {
        format!("-[{}{}{}]->", var, rel_type, props_str)
    } else {
        format!("<-[{}{}{}]-", var, rel_type, props_str)
    }
}

/// 将完整的 GraphPattern 编译为 Cypher MATCH 子句
pub fn build_match(pattern: &GraphPattern) -> String {
    let start = build_node_match(&pattern.start);
    let rel = build_rel_match(&pattern.relationship);
    let end = build_node_match(&pattern.end);
    format!("MATCH {}{}{}", start, rel, end)
}

/// 构建 RETURN 子句
pub fn build_return(vars: &[&str]) -> String {
    format!("RETURN {}", vars.join(", "))
}

/// 构建完整的 MATCH ... RETURN 查询
pub fn build_query(pattern: &GraphPattern, return_vars: &[&str]) -> String {
    let match_clause = build_match(pattern);
    let return_clause = build_return(return_vars);
    format!("{} {}", match_clause, return_clause)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapper::graph::pattern::{GraphPattern, NodePattern, RelationshipPattern};
    use crate::mapper::graph::property::PropertyValue;

    #[test]
    fn node_no_props() {
        let p = NodePattern::with_label("Class").with_variable("c");
        assert_eq!(build_node_match(&p), "(c:Class)");
    }

    #[test]
    fn node_with_props() {
        let p = NodePattern::with_label("Class")
            .with_variable("c")
            .with_property("iri", PropertyValue::from("http://example.org#Person"));
        let result = build_node_match(&p);
        assert!(result.starts_with("(c:Class {"));
        assert!(result.contains("iri: $c_0"));
    }

    #[test]
    fn rel_outgoing() {
        let p = RelationshipPattern::with_type("KNOWS").with_variable("r");
        assert_eq!(build_rel_match(&p), "-[r:KNOWS]->");
    }

    #[test]
    fn rel_incoming() {
        let p = RelationshipPattern::with_type("KNOWS").with_variable("r").incoming();
        assert_eq!(build_rel_match(&p), "<-[r:KNOWS]-");
    }

    #[test]
    fn full_query() {
        let pattern = GraphPattern::link("Person", "KNOWS", "Person");
        let query = build_query(&pattern, &["s", "e"]);
        assert_eq!(query, "MATCH (s:Person)-[r:KNOWS]->(e:Person) RETURN s, e");
    }
}
