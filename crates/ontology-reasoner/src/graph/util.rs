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

/// 按 id + code 组合查找实体。
///
/// - 仅 id：按 id/iri 检索
/// - 仅 code：按 code 检索
/// - 两者都有：两者都匹配才命中
pub fn find_entity_by_id_code(
    repo: &dyn GraphRepository,
    id: &str,
    code: Option<&str>,
) -> Option<Node> {
    if id.is_empty() && code.is_none() {
        return None;
    }

    for label in &["Entity", "Event", "Patrol", "Strike", "Type", "Behavior"] {
        let nodes = repo.get_nodes_by_label(label).unwrap_or_default();
        for n in &nodes {
            let matches_id = if id.is_empty() {
                true
            } else {
                n.property("id").and_then(|v| v.as_str()) == Some(id)
                    || n.property("iri").and_then(|v| v.as_str()) == Some(id)
                    || repo.get_node(id).ok().flatten().is_some()
            };
            let matches_code = match code {
                Some(c) => n.property("code").and_then(|v| v.as_str()) == Some(c),
                None => true,
            };
            if matches_id && matches_code {
                return Some(n.clone());
            }
        }
    }

    // 回退：id 非空时用 find_entity_any
    if !id.is_empty() && code.is_none() {
        return find_entity_any(repo, id);
    }
    None
}

/// 在所有标签类型中按 ID 查找实体。
///
/// 分层匹配（优先级递减）：
/// 1. 直接 `get_node(id)`（按 iri）
/// 2. 精确匹配 `code` / `iri` / `id` / `name`
/// 3. 模糊匹配 `name` / `desc`（子串包含，用于自然语言查询）
pub fn find_entity_any(repo: &dyn GraphRepository, id: &str) -> Option<Node> {
    // 1) 直接按 iri 查找
    if let Ok(Some(n)) = repo.get_node(id) {
        return Some(n);
    }

    // 2) 精确匹配
    for label in &["Entity", "Event", "Patrol", "Strike", "Type", "Behavior"] {
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

    // 3) 模糊回退 — name / desc 包含查询字符串
    // 调用方可能用自然语言描述（如 "雷达故障" 匹配 "雷达" 或 desc 中的 "雷达故障"）
    for label in &["Entity", "Event", "Patrol", "Strike", "Type", "Behavior"] {
        let nodes = repo.get_nodes_by_label(label).unwrap_or_default();
        for n in &nodes {
            let name_match = n.property("name")
                .and_then(|v| v.as_str().map(|s| s.contains(id)))
                .unwrap_or(false);
            let desc_match = n.property("desc")
                .and_then(|v| v.as_str().map(|s| s.contains(id)))
                .unwrap_or(false);
            if name_match || desc_match {
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
    for label in &["Entity", "Event", "Patrol", "Strike", "Type", "Behavior"] {
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

/// 克隆单个实体到指定版本（只克隆节点，不复制关系）。
///
/// **只用副本做推理，不使用原实体。**
///
/// - 如果实体的 code 已含 `_v{target_version}` 后缀且 cope_version 匹配 → 已是副本，直接返回
/// - 如果副本已存在（之前克隆过）→ 直接返回已有副本，不重复创建
/// - 否则，克隆节点：所有属性 + 标签一致，`cope_version` 设为 `target_version`，
///   `code` 追加 `_v{target_version}` 后缀
///
/// 关系的复制由调用方（`clone_all_for_version`）统一处理。
///
/// 返回副本的新 code。
pub fn ensure_cope_version(
    repo: &dyn GraphRepository,
    entity: &Node,
    target_version: &str,
) -> Result<String, String> {
    let original_code = entity
        .property("code")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let new_code = format!("{}_v{}", original_code, target_version);

    // 已经是副本 → 直接返回
    let current_version = entity
        .property("cope_version")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if original_code.ends_with(&format!("_v{}", target_version)) && current_version == target_version {
        return Ok(original_code.to_string());
    }

    // 副本已存在 → 直接返回
    if let Ok(Some(_existing)) = repo.get_node(&new_code) {
        return Ok(new_code);
    }

    // 构造副本节点
    let mut new_props = entity.properties.clone();
    new_props.insert("cope_version".to_string(), PropertyValue::from(target_version));
    new_props.insert("code".to_string(), PropertyValue::from(new_code.as_str()));

    let cloned = Node::new(entity.labels.clone(), new_props);
    repo.insert_node(&cloned)
        .map_err(|e| format!("clone insert '{}': {}", new_code, e))?;

    Ok(new_code)
}

/// 克隆图中所有本体对象及其关系到指定版本。
///
/// **不修改任何原数据（scenario_version 为空的原始实体）。**
///
/// 步骤：
/// 1. 全量扫描：收集所有标签的实体节点，筛选出原实体（cope_version 为空或非当前版本）
/// 2. 全量克隆：对每个原实体创建版本副本（code 追加 `_v{version}` 后缀）
/// 3. 关系复制：复制原实体之间的所有关系到副本之间（原 A→B 变为 副本A→副本B）
///
/// 返回 (旧code→新code 映射, 克隆后的源实体code)
pub fn clone_all_for_version(
    repo: &dyn GraphRepository,
    start_id: &str,
    start_code: Option<&str>,
    target_version: &str,
) -> Result<(std::collections::HashMap<String, String>, String), String> {
    use std::collections::HashMap;

    // ── 1. 全量扫描：收集所有原实体（cope_version 为空） ──
    let mut all_ids: Vec<String> = Vec::new();
    for label in &["Entity", "Event", "Patrol", "Strike", "Type", "Behavior"] {
        let nodes = repo.get_nodes_by_label(label).unwrap_or_default();
        for n in &nodes {
            let code = n.property("code").and_then(|v| v.as_str()).unwrap_or("");
            if code.is_empty() { continue; }
            let ver = n.property("cope_version")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            // 只克隆原实体（cope_version 为空），跳过已有副本
            if ver.is_empty() {
                all_ids.push(code.to_string());
            }
        }
    }

    // ── 2. 解析源实体 ──
    let source = find_entity_by_id_code(repo, start_id, start_code)
        .or_else(|| find_entity_any(repo, start_id))
        .ok_or_else(|| format!("实体 '{}' 未找到", start_id))?;
    let source_code = source.property("code").and_then(|v| v.as_str()).unwrap_or(start_id).to_string();

    // 如果源实体是已有副本，直接用
    if let Some(ref sc) = start_code {
        if let Ok(Some(n)) = repo.get_node(sc) {
            let ver = n.property("cope_version").and_then(|v| v.as_str()).unwrap_or("");
            if ver == target_version {
                // 源已是此版本副本，不需要克隆
                let mut empty_map = HashMap::new();
                empty_map.insert(sc.to_string(), sc.to_string());
                return Ok((empty_map, sc.to_string()));
            }
        }
    }

    // ── 3. 全量克隆：每个原实体创建一个版本副本 ──
    let mut code_map: HashMap<String, String> = HashMap::new();
    for id in &all_ids {
        if let Ok(Some(node)) = repo.get_node(id) {
            match ensure_cope_version(repo, &node, target_version) {
                Ok(new_code) => {
                    code_map.insert(id.clone(), new_code);
                }
                Err(e) => return Err(format!("克隆 '{}' 失败: {}", id, e)),
            }
        }
    }

    // ── 4. 关系复制：副本之间建立与原实体相同的关系 ──
    // 遍历所有原实体，复制其出向关系到副本之间
    for (old_code, new_code) in &code_map {
        let out_rels = repo.get_relationships(old_code, None).unwrap_or_default();
        for r in &out_rels {
            // 目标如果在克隆映射中 → 指向副本；否则跳过（不指向原实体）
            if let Some(new_tgt) = code_map.get(&r.end_node_id) {
                let new_rel = Relationship {
                    rel_type: r.rel_type.clone(),
                    start_node_id: new_code.clone(),
                    end_node_id: new_tgt.clone(),
                    properties: r.properties.clone(),
                };
                let _ = repo.insert_relationship(&new_rel);
            }
        }
    }

    // 源实体对应的副本 code
    let new_source = if source_code.is_empty() {
        code_map.values().next().cloned()
            .ok_or_else(|| "克隆失败：无实体".to_string())?
    } else {
        code_map.get(&source_code).cloned()
            .ok_or_else(|| format!("克隆源实体 '{}' 失败", source_code))?
    };

    Ok((code_map, new_source))
}

/// 按副本版本号删除所有副本实体及其关联关系。
///
/// 步骤：
/// 1. 扫描所有 `Entity` 标签的节点，找到 `cope_version` 匹配的副本
/// 2. 对每个副本，DETACH DELETE（删除节点 + 所有关联关系）
///
/// 返回删除的节点数。
pub fn delete_by_cope_version(
    repo: &dyn GraphRepository,
    target_version: &str,
) -> usize {
    let mut deleted = 0usize;

    for label in &["Entity", "Event", "Patrol", "Strike", "Type", "Behavior"] {
        let nodes = repo.get_nodes_by_label(label).unwrap_or_default();
        for n in &nodes {
            let ver = n.property("cope_version")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if ver == target_version {
                let code = n.property("code")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if let Ok(cnt) = repo.delete_node(code) {
                    deleted += cnt;
                }
            }
        }
    }

    deleted
}

/// 修改图数据库中指定实体的属性值。
///
/// 根据 ID（code）查找实体，用传入的键值对覆盖其属性。
///
/// **副本保护**：如果实体的 `cope_version` 为空（原实体），则先克隆一份副本
/// （调用 `ensure_cope_version`），在副本上修改，不污染原实体。
///
/// 返回更新后的实体。
pub fn update_entity_properties(
    repo: &dyn GraphRepository,
    id: &str,
    updates: HashMap<String, PropertyValue>,
    cope_version: Option<&str>,
) -> Result<Node, String> {
    let entity = find_entity_any(repo, id)
        .ok_or_else(|| format!("实体 '{}' 未找到", id))?;

    // 副本保护：原实体（cope_version 为空）先克隆再修改
    let target_code = if let Some(ver) = cope_version {
        let current_ver = entity
            .property("cope_version")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if current_ver.is_empty() {
            // 原实体 → 克隆副本，在副本上修改
            ensure_cope_version(repo, &entity, ver)?
        } else {
            // 已是副本 → 直接修改
            entity
                .property("code")
                .and_then(|v| v.as_str())
                .unwrap_or(id)
                .to_string()
        }
    } else {
        entity
            .property("code")
            .and_then(|v| v.as_str())
            .unwrap_or(id)
            .to_string()
    };

    // 重新读取目标实体（可能是刚克隆的副本）
    let target = find_entity_any(repo, &target_code)
        .ok_or_else(|| format!("目标实体 '{}' 未找到", target_code))?;

    let code = target
        .property("code")
        .and_then(|v| v.as_str())
        .unwrap_or(&target_code);

    let mut new_props = target.properties.clone();
    for (key, val) in &updates {
        new_props.insert(key.clone(), val.clone());
    }

    // 删除旧节点 → 写入更新后的节点
    repo.delete_node(code)
        .map_err(|e| format!("删除原实体失败: {}", e))?;
    let updated = Node::new(target.labels, new_props);
    repo.insert_node(&updated)
        .map_err(|e| format!("写入更新实体失败: {}", e))?;

    Ok(updated)
}

/// 截断字符串到指定最大长度，超出部分用 `...` 替代。
pub fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        format!("{}...", s.chars().take(max).collect::<String>())
    } else {
        s.to_string()
    }
}
