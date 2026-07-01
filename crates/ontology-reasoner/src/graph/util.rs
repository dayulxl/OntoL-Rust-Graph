//! 图遍历通用工具函数。
//!
//! 所有函数均不依赖领域知识，可被任何业务方复用。

use std::collections::HashMap;

use ontology_storage::mapper::graph::node::Node;
use ontology_storage::mapper::graph::property::PropertyValue;
use ontology_storage::mapper::graph::relationship::Relationship;
use ontology_storage::repository::graph_store::GraphRepository;

// ═══════════════════════════════════════════════════════════
// 公开类型
// ═══════════════════════════════════════════════════════════

/// 关系分组摘要
#[derive(Debug, Clone)]
pub struct RelCount {
    pub relation: String,
    pub count: usize,
    pub example_targets: Vec<String>,
}

/// SWRL 规则匹配结果
#[derive(Debug, Clone)]
pub struct RuleMatch {
    pub rule_name: String,
    pub source_file: String,
    pub match_type: String,
}

// ═══════════════════════════════════════════════════════════
// 实体查找
// ═══════════════════════════════════════════════════════════

/// 在所有标签类型中按 ID 查找实体。
///
/// 按以下顺序尝试匹配：
/// 1. 直接 `get_node(id)`（按 iri 查找）
/// 2. 扫描各标签节点的 `code` / `iri` / `id` / `name` 属性
pub fn find_entity_any(repo: &dyn GraphRepository, id: &str) -> Option<Node> {
    // 1) 直接按 iri 查找
    if let Ok(Some(n)) = repo.get_node(id) {
        return Some(n);
    }

    // 2) 按标签 + 多字段匹配扫描
    for label in &["Entity", "Patrol", "Strike", "Type"] {
        let nodes = repo.get_nodes_by_label(label).unwrap_or_default();
        for n in &nodes {
            if n.property("code").and_then(|v| v.as_str()) == Some(id)
                || n.property("iri").and_then(|v| v.as_str()) == Some(id)
                || n.property("id").and_then(|v| v.as_str()) == Some(id)
                || n.property("name").and_then(|v| v.as_str()) == Some(id)
            {
                return Some(n.clone());
            }
        }
    }
    None
}

// ═══════════════════════════════════════════════════════════
// 关系查询
// ═══════════════════════════════════════════════════════════

/// 查找指向目标节点的传入关系。
///
/// 因为 `GraphRepository::get_relationships()` 只支持出向查询，
/// 此函数通过扫描所有节点的出向关系来反向查找。
pub fn find_incoming_relationships(
    repo: &dyn GraphRepository,
    target_id: &str,
    rel_type: Option<&str>,
) -> Vec<Relationship> {
    let mut results = Vec::new();
    for label in &["Entity", "Patrol", "Strike", "Type"] {
        let nodes = repo.get_nodes_by_label(label).unwrap_or_default();
        for n in &nodes {
            let ncode = n.property("code").and_then(|v| v.as_str()).unwrap_or("");
            if ncode == target_id {
                continue;
            }
            let rels = repo.get_relationships(ncode, rel_type).unwrap_or_default();
            for r in &rels {
                if r.end_node_id == target_id {
                    results.push(r.clone());
                }
            }
        }
    }
    results
}

/// 对关系列表做分组摘要 — 按关系类型聚合，返回计数和示例目标。
pub fn summarize_relations(rels: &[Relationship]) -> Vec<RelCount> {
    let mut counts: HashMap<String, (usize, Vec<String>)> = HashMap::new();
    for r in rels {
        let entry = counts.entry(r.rel_type.clone()).or_insert((0, Vec::new()));
        entry.0 += 1;
        if entry.1.len() < 3 {
            entry.1.push(r.end_node_id.clone());
        }
    }
    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.0.cmp(&a.1.0));
    sorted
        .iter()
        .take(15)
        .map(|(rel_type, (count, examples))| RelCount {
            relation: rel_type.clone(),
            count: *count,
            example_targets: examples.clone(),
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════
// 类型层次
// ═══════════════════════════════════════════════════════════

/// 获取节点的类型层次链 — 沿 `subClassOf` 关系向上追溯。
pub fn get_type_ancestors(repo: &dyn GraphRepository, node: Option<&Node>) -> Vec<String> {
    let type_name = match node.and_then(|n| n.property("type").and_then(|v| v.as_str())) {
        Some(t) => t.to_string(),
        None => return Vec::new(),
    };
    if type_name.is_empty() {
        return Vec::new();
    }

    let mut path = vec![type_name.clone()];
    let types = repo.get_nodes_by_label("Type").unwrap_or_default();
    let mut parents: HashMap<String, String> = HashMap::new();

    for t in &types {
        let tname = t.property("name").and_then(|v| v.as_str()).unwrap_or("");
        let rels = repo.get_relationships(tname, None).unwrap_or_default();
        for r in &rels {
            if r.rel_type == "subClassOf" {
                if let Some(pn) = repo.get_node(&r.end_node_id).ok().flatten() {
                    if let Some(pname) = pn.property("name").and_then(|v| v.as_str()) {
                        parents.insert(tname.to_string(), pname.to_string());
                    }
                }
            }
        }
    }

    let mut current = type_name;
    for _ in 0..10 {
        if let Some(p) = parents.get(&current) {
            path.push(p.clone());
            current = p.clone();
        } else {
            break;
        }
    }
    path
}

// ═══════════════════════════════════════════════════════════
// SWRL 规则匹配
// ═══════════════════════════════════════════════════════════

/// 扫描 `rules/` 目录下的 `.swrl` 文件，查找与当前跳相关的规则。
///
/// 通过关键字匹配（关系名、源/目标节点 ID、节点标签）来近似匹配。
/// 当 Reasoner 暴露 `rules()` 公开方法后，可改为基于 AST 的精确匹配。
pub fn find_matching_rules(
    _repo: &dyn GraphRepository,
    source_id: &str,
    relation: &str,
    target_id: &str,
    rules_dir: &str,
) -> Vec<RuleMatch> {
    let mut matched = Vec::new();

    let keywords = [relation, source_id, target_id];

    let entries = match std::fs::read_dir(rules_dir) {
        Ok(e) => e,
        Err(_) => return matched,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("swrl") {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // 关键字匹配
        let relevant = keywords.iter().any(|kw| content.contains(kw));
        if !relevant {
            continue;
        }

        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");

        // 提取第一条规则的显示名称（方括号内）
        let display_name = content
            .lines()
            .find(|l| l.contains('[') && l.contains(']'))
            .and_then(|l| {
                let start = l.find('[')?;
                let end = l.find(']')?;
                Some(l[start + 1..end].to_string())
            })
            .unwrap_or_else(|| file_stem.to_string());

        matched.push(RuleMatch {
            rule_name: display_name,
            source_file: file_stem.to_string(),
            match_type: "keyword".to_string(),
        });
    }

    matched
}

// ═══════════════════════════════════════════════════════════
// 下一步预测
// ═══════════════════════════════════════════════════════════

/// 分析目标节点的出向关系，按频次降序排列，用于预测可能的后续操作。
pub fn predict_next_steps(repo: &dyn GraphRepository, node_id: &str) -> Vec<RelCount> {
    let rels = repo.get_relationships(node_id, None).unwrap_or_default();
    let mut counts: HashMap<String, (usize, Vec<String>)> = HashMap::new();

    for r in &rels {
        let entry = counts.entry(r.rel_type.clone()).or_insert((0, Vec::new()));
        entry.0 += 1;
        if entry.1.len() < 3 {
            entry.1.push(r.end_node_id.clone());
        }
    }

    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.0.cmp(&a.1.0));

    sorted
        .iter()
        .take(10)
        .map(|(rel_type, (count, examples))| RelCount {
            relation: rel_type.clone(),
            count: *count,
            example_targets: examples.clone(),
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════
// 属性转换
// ═══════════════════════════════════════════════════════════

/// 将 `PropertyValue` 转为 `f64`。
pub fn prop_as_f64(val: Option<&PropertyValue>) -> Option<f64> {
    match val {
        Some(PropertyValue::Float(f)) => Some(*f),
        Some(PropertyValue::Integer(i)) => Some(*i as f64),
        _ => None,
    }
}

/// 截断字符串到指定最大长度，超出部分用 `...` 替代。
pub fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        format!("{}...", s.chars().take(max).collect::<String>())
    } else {
        s.to_string()
    }
}
