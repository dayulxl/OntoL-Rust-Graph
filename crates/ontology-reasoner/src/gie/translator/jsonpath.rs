//! # JSONPath 翻译器 — JSONPath → QueryPlan
//!
//! 将 JSONPath 表达式（RFC 9535，`$.` 前缀）翻译为 [`QueryPlan`]。
//!
//! ## 路径解析
//!
//! ```text
//! $.position.lat → segments = ["position", "lat"]
//! $.node1.node1_1.field → segments = ["node1", "node1_1", "field"]
//! ```
//!
//! 每段可以是属性查找或关系遍历，由存储适配器决定。

use ontology_storage::mapper::query_plan::QueryPlan;

/// 解析 JSONPath 表达式为路径段列表。
///
/// # 示例
///
/// ```
/// let segments = parse_jsonpath("$.position.lat");
/// // → ["position", "lat"]
/// ```
pub fn parse_jsonpath(expr: &str) -> Result<Vec<String>, String> {
    let expr = expr.trim();

    // 必须以 $. 开头
    let body = expr
        .strip_prefix("$.")
        .or_else(|| expr.strip_prefix('$'))
        .ok_or_else(|| format!("JSONPath 必须以 '$.' 开头: '{}'", expr))?;

    if body.is_empty() {
        return Ok(Vec::new());
    }

    // 简单实现：按 '.' 拆分
    // 后续可扩展支持 [index]、[*]、[?(@.prop==val)] 等
    let segments: Vec<String> = body
        .split('.')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    if segments.is_empty() {
        return Err(format!("JSONPath 路径为空: '{}'", expr));
    }

    Ok(segments)
}

/// 将 JSONPath 表达式翻译为 JsonPathLookup QueryPlan。
pub fn translate_jsonpath(node_code: &str, path_expr: &str) -> Result<QueryPlan, String> {
    let segments = parse_jsonpath(path_expr)?;
    Ok(QueryPlan::JsonPathLookup {
        node_code: node_code.to_string(),
        segments,
    })
}
