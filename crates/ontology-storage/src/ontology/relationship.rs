//! 关系创建 + 跨标签节点解析。
//!
//! 支持按 code / name / id 自动查找源节点和目标节点，
//! 可选通过 label 限定查找范围（Entity / Type / Patrol）。

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::mapper::graph::property::PropertyValue;
use crate::mapper::graph::relationship::Relationship;
use crate::mapper::unified_mapping;
use crate::repository::graph_store::GraphRepository;

use super::entity::{get_str, json_to_property};

/// 生成当前时间字符串 (ISO 8601 简化格式)
fn format_now() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // 简单格式: YYYY-MM-DD HH:MM:SS
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // 以 epoch 1970-01-01 为起点推算
    let total_days = days as i64;
    let (y, m, d) = days_to_date(total_days);

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        y, m, d, hours, minutes, seconds
    )
}

/// 简化日期转换（仅用于生成时间字符串，非精确日历计算）
fn days_to_date(total_days: i64) -> (i64, u32, u32) {
    let mut y = 1970i64;
    let mut remaining = total_days;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let months_days = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 1u32;
    for &md in &months_days {
        if remaining < md as i64 {
            break;
        }
        remaining -= md as i64;
        m += 1;
    }
    (y, m, (remaining + 1) as u32)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

// ═══════════════════════════════════════════════════════════
// 关系创建
// ═══════════════════════════════════════════════════════════

/// 创建单条关系。
///
/// 自动解析 source / target 节点标识符，校验存在性后写入。
pub fn create_relationship(
    repo: &dyn GraphRepository,
    spec: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let rel_type = get_str(spec, "rel_type").ok_or("'rel_type' is required")?;

    let source_id = resolve_node_id(repo, spec, "source")?;
    let target_id = resolve_node_id(repo, spec, "target")?;

    // 关系属性 — 先收集用户传入的属性
    let mut rel_props = HashMap::new();
    if let Some(obj) = spec.get("properties").and_then(|v| v.as_object()) {
        for (k, v) in obj {
            rel_props.insert(k.clone(), json_to_property(v));
        }
    }

    // 标准审计字段 — 自动填充默认值（用户传入时覆盖）
    let now = format_now();
    rel_props
        .entry(unified_mapping::CREATE_TIME_KEY.to_string())
        .or_insert_with(|| PropertyValue::String(now.clone()));
    rel_props
        .entry(unified_mapping::UPDATE_TIME_KEY.to_string())
        .or_insert_with(|| PropertyValue::String(now));
    rel_props
        .entry(unified_mapping::DELETE_FLAG_KEY.to_string())
        .or_insert_with(|| PropertyValue::Integer(0));
    rel_props
        .entry(unified_mapping::IS_SYSTEM_KEY.to_string())
        .or_insert_with(|| PropertyValue::String("0".to_string()));

    // create_user / update_user — 有则填入，无则空字符串
    rel_props
        .entry(unified_mapping::CREATE_USER_KEY.to_string())
        .or_insert_with(|| PropertyValue::String(String::new()));
    rel_props
        .entry(unified_mapping::UPDATE_USER_KEY.to_string())
        .or_insert_with(|| PropertyValue::String(String::new()));

    let rel = Relationship::new(&source_id, rel_type, &target_id, rel_props);
    repo.insert_relationship(&rel)
        .map_err(|e| format!("Insert relationship failed: {}", e))?;

    Ok(serde_json::json!({
        "source": source_id,
        "rel_type": rel_type,
        "target": target_id,
    }))
}

// ═══════════════════════════════════════════════════════════
// 节点解析
// ═══════════════════════════════════════════════════════════

/// 解析节点标识符，返回用于构造关系的 node_id。
///
/// 查找顺序：
/// 1. `{prefix}` — 直接值
/// 2. `{prefix}_code` — 按 code 查找
/// 3. `{prefix}_name` — 按 name 查找
/// 4. `{prefix}_id` — 直接作为内部 ID 使用
///
/// 如果指定了 `{prefix}_label`，则只在对应标签下查找；
/// 否则依次尝试 Entity → Type → Patrol。
pub fn resolve_node_id(
    repo: &dyn GraphRepository,
    spec: &serde_json::Value,
    prefix: &str,
) -> Result<String, String> {
    let value = get_str(spec, &format!("{}_{}", prefix, "id"))
        .or_else(|| get_str(spec, &format!("{}_{}", prefix, "code")))
        .or_else(|| get_str(spec, &format!("{}_{}", prefix, "name")))
        .or_else(|| get_str(spec, prefix))
        .ok_or_else(|| {
            format!(
                "'{}_{}' or '{}_{}' is required",
                prefix, "id", prefix, "code"
            )
        })?;

    let label_key = format!("{}_{}", prefix, "label");
    let specified_label = get_str(spec, &label_key);

    if let Some(label) = specified_label {
        return find_node_by_label(repo, label, value)
            .ok_or_else(|| format!("Node not found: label='{}', value='{}'", label, value));
    }

    // 自动查找：Entity → Type → Patrol
    for label in &[
        unified_mapping::ENTITY_LABEL,
        unified_mapping::TYPE_LABEL,
        unified_mapping::PATROL_LABEL,
    ] {
        if let Some(id) = find_node_by_label(repo, label, value) {
            return Ok(id);
        }
    }

    // 兜底：直接作为 node_id 使用
    Ok(value.to_string())
}

/// 在指定标签的节点中按 code / name 查找，返回节点内部标识符（优先 `id` 属性，其次 `code`）。
fn find_node_by_label(repo: &dyn GraphRepository, label: &str, value: &str) -> Option<String> {
    let nodes = repo.get_nodes_by_label(label).unwrap_or_default();

    // 优先 code 匹配
    for node in &nodes {
        if node.property("code").and_then(|v| v.as_str()) == Some(value) {
            return node
                .property("id")
                .and_then(|v| v.as_str())
                .or_else(|| node.property("code").and_then(|v| v.as_str()))
                .map(|s| s.to_string());
        }
    }

    // 其次 name 匹配
    for node in &nodes {
        if node.property("name").and_then(|v| v.as_str()) == Some(value) {
            return node
                .property("id")
                .and_then(|v| v.as_str())
                .or_else(|| node.property("code").and_then(|v| v.as_str()))
                .map(|s| s.to_string());
        }
    }

    None
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_node_id_missing_value() {
        let spec = serde_json::json!({});
        assert_eq!(get_str(&spec, "source"), None);
        assert_eq!(get_str(&spec, "source_code"), None);
    }

    #[test]
    fn resolve_with_label_specified() {
        let spec = serde_json::json!({
            "source": "P8A_001",
            "source_label": "Entity"
        });
        assert_eq!(get_str(&spec, "source"), Some("P8A_001"));
        assert_eq!(get_str(&spec, "source_label"), Some("Entity"));
    }

    #[test]
    fn resolve_with_code_and_name() {
        let spec = serde_json::json!({
            "source_code": "P8A_001",
            "source_name": "P-8A海神巡逻机",
            "target": "P8A_002"
        });
        assert_eq!(get_str(&spec, "source"), None);
        assert_eq!(get_str(&spec, "source_code"), Some("P8A_001"));
        assert_eq!(get_str(&spec, "target"), Some("P8A_002"));
    }

    #[test]
    fn relationship_with_properties() {
        let spec = serde_json::json!({
            "source": "P8A_001",
            "rel_type": "移动",
            "target": "P8A_002",
            "properties": { "distance": 120.5, "bearing": 45 }
        });
        assert_eq!(get_str(&spec, "rel_type"), Some("移动"));
        let props = spec.get("properties").and_then(|v| v.as_object());
        assert!(props.is_some());
        assert_eq!(props.unwrap().len(), 2);
    }
}
