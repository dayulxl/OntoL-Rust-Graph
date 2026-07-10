//! 结构化 Prompt 构建器。
//!
//! 从本体数据（Node、Relationship、GraphPattern 等）构建 LLM 可理解的上下文 prompt。
//! 使得每次 LLM 调用携带的上下文数据格式一致、结构清晰。

use crate::mapper::graph::node::Node;
use crate::mapper::graph::pattern::{GraphPattern, NodePattern, RelationshipPattern};
use crate::mapper::graph::property::PropertyValue;
use crate::mapper::graph::relationship::Relationship;

// ── 上下文构建 ──

/// LLM 上下文 — 一次 LLM 调用所携带的结构化数据快照
#[derive(Debug, Clone)]
pub struct LlmContext {
    /// 系统指令（角色设定、任务描述）
    pub system_prompt: String,

    /// 上下文节点
    pub nodes: Vec<Node>,

    /// 上下文关系
    pub relationships: Vec<Relationship>,

    /// 自然语言描述（从节点和关系自动生成）
    pub description: String,
}

impl LlmContext {
    /// 创建一个空的上下文
    pub fn new(system_prompt: &str) -> Self {
        LlmContext {
            system_prompt: system_prompt.to_string(),
            nodes: Vec::new(),
            relationships: Vec::new(),
            description: String::new(),
        }
    }

    /// 添加节点
    pub fn with_nodes(mut self, nodes: Vec<Node>) -> Self {
        self.nodes = nodes;
        self
    }

    /// 添加关系
    pub fn with_relationships(mut self, rels: Vec<Relationship>) -> Self {
        self.relationships = rels;
        self
    }

    /// 构建后自动生成描述
    pub fn build(mut self) -> Self {
        self.description = build_description(&self.nodes, &self.relationships);
        self
    }

    /// 渲染为完整 prompt 文本
    pub fn render(&self) -> String {
        let mut buf = String::new();

        // 系统指令
        buf.push_str(&format!("# 系统指令\n\n{}\n\n", self.system_prompt));

        // 上下文描述
        if !self.description.is_empty() {
            buf.push_str(&format!("# 数据上下文\n\n{}\n", self.description));
        }

        // 节点详情
        if !self.nodes.is_empty() {
            buf.push_str("\n## 节点列表\n\n");
            for (i, node) in self.nodes.iter().enumerate() {
                buf.push_str(&format_node(i + 1, node));
            }
        }

        // 关系详情
        if !self.relationships.is_empty() {
            buf.push_str("\n## 关系列表\n\n");
            for (i, rel) in self.relationships.iter().enumerate() {
                buf.push_str(&format_relationship(i + 1, rel));
            }
        }

        buf
    }
}

// ── 描述生成 ──

/// 从节点和关系自动生成自然语言描述
fn build_description(nodes: &[Node], rels: &[Relationship]) -> String {
    if nodes.is_empty() && rels.is_empty() {
        return String::new();
    }

    let mut desc = String::new();

    // 统计信息 — Node.labels 是 pub 字段
    let class_nodes: Vec<_> = nodes.iter().filter(|n| n.has_label("Class")).collect();
    let individual_nodes: Vec<_> = nodes.iter().filter(|n| n.has_label("Individual")).collect();

    if !class_nodes.is_empty() || !individual_nodes.is_empty() {
        desc.push_str(&format!(
            "当前本体包含 {} 个类和 {} 个个体。",
            class_nodes.len(),
            individual_nodes.len()
        ));
    }

    // 个体归属汇总 — Relationship.rel_type 是 pub 字段
    let instance_rels: Vec<_> = rels
        .iter()
        .filter(|r| r.rel_type == "INSTANCE_OF")
        .collect();

    if !instance_rels.is_empty() {
        desc.push_str(&format!(
            "已加载 {} 条 INSTANCE_OF 关系（个体→类）。",
            instance_rels.len()
        ));
    }

    desc
}

// ── 格式化工具 ──

/// 尝试从节点属性中提取 IRI
fn node_iri(node: &Node) -> &str {
    node.properties
        .get("iri")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
}

/// 将节点格式化为可读文本
fn format_node(index: usize, node: &Node) -> String {
    let labels_str = node.labels.join(", ");
    let iri = node_iri(node);
    let mut buf = format!("{}. [{}] (IRI: {})\n", index, labels_str, iri);

    for (key, value) in &node.properties {
        if key == "iri" {
            continue; // 已在上方显示
        }
        buf.push_str(&format!("   {}: {}\n", key, format_property_value(value)));
    }

    buf
}

/// 将关系格式化为可读文本
fn format_relationship(index: usize, rel: &Relationship) -> String {
    let mut buf = format!(
        "{}. ({}) -[{}]-> ({})\n",
        index, rel.start_node_id, rel.rel_type, rel.end_node_id
    );

    for (key, value) in &rel.properties {
        buf.push_str(&format!("   {}: {}\n", key, format_property_value(value)));
    }

    buf
}

/// 将 PropertyValue 格式化为可读字符串
fn format_property_value(value: &PropertyValue) -> String {
    match value {
        PropertyValue::String(s) => format!("\"{}\"", s),
        PropertyValue::Integer(i) => i.to_string(),
        PropertyValue::Float(f) => format!("{:.6}", f),
        PropertyValue::Boolean(b) => b.to_string(),
        PropertyValue::List(items) => {
            let inner: Vec<String> = items.iter().map(format_property_value).collect();
            format!("[{}]", inner.join(", "))
        }
        PropertyValue::Map(map) => {
            let inner: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{}: {}", k, format_property_value(v)))
                .collect();
            format!("{{{}}}", inner.join(", "))
        }
        PropertyValue::Null => "null".to_string(),
    }
}

// ── 图模式 → 自然语言 ──

/// 将 GraphPattern 翻译为自然语言查询描述
///
/// 用于在 prompt 中向 LLM 说明本次查询的意图。
///
/// GraphPattern 结构为 `(start)-[relationship]->(end)` 单条路径模式。
pub fn pattern_to_nl_description(pattern: &GraphPattern) -> String {
    let start_desc = describe_node_pattern(&pattern.start);
    let rel_desc = describe_relationship_pattern(&pattern.relationship);
    let end_desc = describe_node_pattern(&pattern.end);

    format!("{} {} {}", start_desc, rel_desc, end_desc)
}

fn describe_node_pattern(pat: &NodePattern) -> String {
    match (&pat.labels, &pat.variable) {
        (Some(label), Some(var)) => format!("匹配标签为 `{}` 的节点（变量 `{}`）", label, var),
        (Some(label), None) => format!("匹配所有标签为 `{}` 的节点", label),
        (None, Some(var)) => format!("匹配任意节点（变量 `{}`）", var),
        (None, None) => "匹配任意节点".to_string(),
    }
}

fn describe_relationship_pattern(pat: &RelationshipPattern) -> String {
    let dir = if pat.outgoing { "→" } else { "←" };
    match &pat.rel_type {
        Some(t) => format!("通过类型为 `{}` 的关系 {} 连接", t, dir),
        None => format!("通过任意关系 {} 连接", dir),
    }
}

// ── 测试 ──

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn make_test_node() -> Node {
        let mut props = HashMap::new();
        props.insert("iri".to_string(), PropertyValue::from("ex:Person"));
        props.insert("label".to_string(), PropertyValue::from("人"));
        Node::new(vec!["Class".to_string()], props)
    }

    #[test]
    fn build_context_and_render() {
        let node = make_test_node();
        let ctx = LlmContext::new("你是一个本体查询助手")
            .with_nodes(vec![node])
            .build();

        let rendered = ctx.render();
        assert!(rendered.contains("你是一个本体查询助手"));
        assert!(rendered.contains("ex:Person"));
        assert!(rendered.contains("Class"));
    }

    #[test]
    fn pattern_to_nl_description_works() {
        let pattern = GraphPattern::link("Class", "INSTANCE_OF", "Individual");

        let desc = pattern_to_nl_description(&pattern);
        assert!(desc.contains("Class"));
        assert!(desc.contains("INSTANCE_OF"));
        assert!(desc.contains("Individual"));
    }

    #[test]
    fn format_property_value_all_types() {
        assert_eq!(format_property_value(&PropertyValue::Null), "null");
        assert_eq!(format_property_value(&PropertyValue::Boolean(true)), "true");
        assert_eq!(format_property_value(&PropertyValue::Integer(42)), "42");
        assert_eq!(
            format_property_value(&PropertyValue::from("hello")),
            "\"hello\""
        );
    }
}
