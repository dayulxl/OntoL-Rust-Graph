//! Neo4j HTTP 查询执行器。
//!
//! 通过 Neo4j HTTP API (`/db/{db}/tx/commit`) 执行 Cypher 查询，
//! 将 JSON 响应反序列化为领域 `Node` / `Relationship`。
//!
//! ## 为什么不用 Bolt (neo4rs)
//!
//! HTTP API 依赖极简（`ureq` + `serde_json`），纯 Rust，零 native 依赖，
//! 在任何 Rust 工具链上都能编译，无需 VS Build Tools 或 tokio runtime。
//!
//! ## API 格式
//!
//! 请求: `POST /db/neo4j/tx/commit`
//! ```json
//! {"statements": [{"statement": "MATCH (n) RETURN n", "parameters": {}}]}
//! ```
//!
//! 响应中节点格式:
//! ```json
//! {"identity": 1, "labels": ["Class"], "properties": {"iri": "..."}}
//! ```
//!
//! 响应中关系格式:
//! ```json
//! {"identity": 1, "start": 0, "end": 2, "type": "KNOWS", "properties": {}}
//! ```

use std::collections::HashMap;

use serde_json::Value as JsonValue;

use crate::error::StoreError;
use crate::mapper::graph::node::Node;
use crate::mapper::graph::pattern::{GraphPattern, NodePattern, RelationshipPattern};
use crate::mapper::graph::property::PropertyValue;
use crate::mapper::graph::relationship::Relationship;
use crate::mapper::cypher::builder;

/// Neo4j HTTP 执行器
pub struct Neo4jExecutor {
    /// Neo4j HTTP endpoint，如 `http://localhost:7474`
    endpoint: String,
    /// 数据库名，默认 `neo4j`
    database: String,
    /// Basic Auth 凭证
    auth_header: String,
}

impl Neo4jExecutor {
    /// 创建执行器实例
    pub fn new(uri: &str, user: &str, password: &str) -> Self {
        let auth = format!("{}:{}", user, password);
        let encoded = base64_encode(&auth);
        Self {
            endpoint: uri.trim_end_matches('/').to_string(),
            database: "neo4j".to_string(),
            auth_header: format!("Basic {}", encoded),
        }
    }

    /// 设置数据库名
    pub fn with_database(mut self, db: &str) -> Self {
        self.database = db.to_string();
        self
    }

    // ═══════════════════════════════════════════════════════
    // 查询：节点
    // ═══════════════════════════════════════════════════════

    /// 按 IRI 获取节点
    /// `MATCH (n { iri: $iri }) RETURN n LIMIT 1`
    pub fn get_node(&self, iri: &str) -> Result<Option<Node>, StoreError> {
        let params = serde_json::json!({ "iri": iri });
        let rows = self.execute_cypher(
            "MATCH (n { iri: $iri }) RETURN n LIMIT 1",
            &params,
        )?;

        extract_node(&rows, "n")
    }

    /// 按标签获取所有节点
    /// `MATCH (n:Label) RETURN n`
    pub fn get_nodes_by_label(&self, label: &str) -> Result<Vec<Node>, StoreError> {
        let cypher = format!("MATCH (n:{}) RETURN n", label);
        let rows = self.execute_cypher(&cypher, &serde_json::json!({}))?;

        let mut nodes = Vec::new();
        for row in &rows {
            if let Some(node) = parse_node_from_row(row, "n") {
                nodes.push(node);
            }
        }
        Ok(nodes)
    }

    // ═══════════════════════════════════════════════════════
    // 查询：关系
    // ═══════════════════════════════════════════════════════

    /// 获取节点的出向关系
    /// `MATCH (n { iri: $iri })-[r:REL_TYPE]->(m) RETURN r, m.iri AS target`
    pub fn get_relationships(
        &self,
        node_iri: &str,
        rel_type: Option<&str>,
    ) -> Result<Vec<Relationship>, StoreError> {
        let params = serde_json::json!({ "iri": node_iri });

        let cypher = match rel_type {
            Some(rt) => format!(
                "MATCH (n {{ iri: $iri }})-[r:{}]->(m) RETURN r, m.iri AS target",
                rt
            ),
            None => "MATCH (n { iri: $iri })-[r]->(m) RETURN r, m.iri AS target".to_string(),
        };

        let rows = self.execute_cypher(&cypher, &params)?;

        let mut rels = Vec::new();
        for row in &rows {
            if let Some(r) = parse_relationship_from_row(row, "r") {
                let target_iri = row.get("target")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let mut rel = r;
                // 确保 end_node_id 使用 IRI 而非内部 ID
                if !target_iri.is_empty() {
                    rel.end_node_id = target_iri;
                }
                rels.push(rel);
            }
        }
        Ok(rels)
    }

    // ═══════════════════════════════════════════════════════
    // 查询：图模式匹配
    // ═══════════════════════════════════════════════════════

    /// 执行图模式匹配: (start)-[rel]->(end)
    /// 返回 (Node, Vec<Relationship>, Node) 三元组列表
    pub fn query_pattern(
        &self,
        pattern: &GraphPattern,
    ) -> Result<Vec<(Node, Vec<Relationship>, Node)>, StoreError> {
        let match_clause = build_match_clause(pattern);
        // 保持 MATCH 中的原始变量名，但 RETURN 统一别名为 s, r, e
        let start_v = pattern.start.variable.as_deref().unwrap_or("s");
        let rel_v = pattern.relationship.variable.as_deref().unwrap_or("r");
        let end_v = pattern.end.variable.as_deref().unwrap_or("e");
        let cypher = format!(
            "{} RETURN {} AS s, {} AS r, {} AS e LIMIT 1000",
            match_clause, start_v, rel_v, end_v
        );

        let params = build_pattern_params(pattern);
        let rows = self.execute_cypher(&cypher, &params)?;

        let mut results = Vec::new();
        for row in &rows {
            let s = parse_node_from_row(row, "s");
            let e = parse_node_from_row(row, "e");
            let r = parse_relationship_from_row(row, "r");

            if let (Some(start), Some(end), Some(rel)) = (s, e, r) {
                results.push((start, vec![rel], end));
            }
        }
        Ok(results)
    }

    // ═══════════════════════════════════════════════════════
    // 写入：节点
    // ═══════════════════════════════════════════════════════

    /// 创建节点: `CREATE (n:Label1:Label2 { ... }) RETURN n.iri AS iri`
    pub fn create_node(&self, node: &Node) -> Result<String, StoreError> {
        let labels_str = if node.labels.is_empty() {
            String::new()
        } else {
            format!(":{}", node.labels.join(":"))
        };

        // 收集属性参数
        let mut params = serde_json::Map::new();
        let mut set_parts = Vec::new();
        for (i, (key, value)) in node.properties.iter().enumerate() {
            let pname = format!("p{}", i);
            set_parts.push(format!("{}: ${}", key, pname));
            params.insert(pname, property_to_json(value));
        }

        let cypher = if set_parts.is_empty() {
            format!("CREATE (n{}) RETURN COALESCE(n.iri, '') AS iri", labels_str)
        } else {
            format!(
                "CREATE (n{} {{ {} }}) RETURN COALESCE(n.iri, '') AS iri",
                labels_str,
                set_parts.join(", ")
            )
        };

        let rows = self.execute_cypher(&cypher, &JsonValue::Object(params))?;

        // 尝试从返回值获取 iri
        for row in &rows {
            if let Some(iri) = row.get("iri").and_then(|v| v.as_str()) {
                if !iri.is_empty() {
                    return Ok(iri.to_string());
                }
            }
        }

        // 回退：从输入参数中取 iri
        node.property("iri")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| StoreError::Query("Node creation failed — no iri returned".into()))
    }

    // ═══════════════════════════════════════════════════════
    // 写入：关系
    // ═══════════════════════════════════════════════════════

    /// 创建关系: `MATCH (a { iri: $a }), (b { iri: $b }) CREATE (a)-[r:TYPE]->(b)`
    pub fn create_relationship(&self, rel: &Relationship) -> Result<(), StoreError> {
        let mut params = serde_json::json!({
            "start_iri": rel.start_node_id,
            "end_iri": rel.end_node_id,
        });

        // 关系属性
        let props_set = if rel.properties.is_empty() {
            String::new()
        } else {
            let parts: Vec<String> = rel.properties.iter().enumerate()
                .map(|(i, (k, _))| format!("{}: $rp{}", k, i))
                .collect();
            for (i, (_k, v)) in rel.properties.iter().enumerate() {
                if let JsonValue::Object(ref mut m) = params {
                    m.insert(format!("rp{}", i), property_to_json(v));
                }
            }
            format!(" SET r = {{ {} }}", parts.join(", "))
        };

        let cypher = format!(
            "MATCH (a {{ iri: $start_iri }}), (b {{ iri: $end_iri }}) CREATE (a)-[r:{}]->(b){} RETURN r",
            rel.rel_type, props_set
        );

        self.execute_cypher(&cypher, &params)?;
        Ok(())
    }

    // ═══════════════════════════════════════════════════════
    // 删除
    // ═══════════════════════════════════════════════════════

    /// 删除节点及其所有关系: `MATCH (n { iri: $iri }) DETACH DELETE n`
    pub fn delete_node(&self, iri: &str) -> Result<usize, StoreError> {
        let params = serde_json::json!({ "iri": iri });
        let rows = self.execute_cypher(
            "MATCH (n { iri: $iri }) DETACH DELETE n RETURN count(n) AS cnt",
            &params,
        )?;
        for row in &rows {
            if let Some(cnt) = row.get("cnt").and_then(|v| v.as_i64()) {
                return Ok(cnt as usize);
            }
        }
        Ok(0)
    }

    /// 删除指定关系: `MATCH (a {iri:$a})-[r:TYPE]->(b {iri:$b}) DELETE r`
    pub fn delete_relationship_by_endpoints(
        &self,
        start_iri: &str,
        end_iri: &str,
        rel_type: &str,
    ) -> Result<usize, StoreError> {
        let params = serde_json::json!({
            "start": start_iri,
            "end": end_iri,
        });
        let cypher = format!(
            "MATCH (a {{ iri: $start }})-[r:{}]->(b {{ iri: $end }}) DELETE r RETURN count(r) AS cnt",
            rel_type
        );
        let rows = self.execute_cypher(&cypher, &params)?;
        for row in &rows {
            if let Some(cnt) = row.get("cnt").and_then(|v| v.as_i64()) {
                return Ok(cnt as usize);
            }
        }
        Ok(0)
    }

    // ═══════════════════════════════════════════════════════
    // HTTP 调用核心
    // ═══════════════════════════════════════════════════════

    /// 执行 Cypher 查询，返回原始结果行（用于 schema 初始化等场景）
    pub fn execute_cypher_raw(
        &self,
        cypher: &str,
        params: &JsonValue,
    ) -> Result<Vec<JsonValue>, StoreError> {
        // 简化版本：不要求 "graph" 格式，只返回行数据
        self.do_execute(cypher, params, &["row"])
    }

    /// 执行 Cypher 查询，返回行数据列表（含节点/关系元数据）
    fn execute_cypher(
        &self,
        cypher: &str,
        params: &JsonValue,
    ) -> Result<Vec<JsonValue>, StoreError> {
        self.do_execute(cypher, params, &["row", "graph"])
    }

    /// 核心执行逻辑
    fn do_execute(
        &self,
        cypher: &str,
        params: &JsonValue,
        result_contents: &[&str],
    ) -> Result<Vec<JsonValue>, StoreError> {
        let url = format!(
            "{}/db/{}/tx/commit",
            self.endpoint, self.database
        );

        let body = serde_json::json!({
            "statements": [{
                "statement": cypher,
                "parameters": params,
                "resultDataContents": result_contents
            }]
        });

        let body_str = serde_json::to_string(&body)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let resp = ureq::post(&url)
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .send(body_str.as_bytes());

        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("{}", e);
                if msg.contains("connection") || msg.contains("resolve") || msg.contains("refused") {
                    return Err(StoreError::Connection(format!(
                        "Neo4j at {}: {}", self.endpoint, msg
                    )));
                }
                return Err(StoreError::Query(format!("Neo4j HTTP: {}\nQuery: {}", msg, cypher)));
            }
        };

        let body_str = resp
            .into_body()
            .read_to_string()
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let raw: JsonValue = serde_json::from_str(&body_str)
            .map_err(|e| StoreError::Serialization(format!("{} — body: {}", e, &body_str[..body_str.len().min(500)])))?;

        // 检查错误
        if let Some(errors) = raw.get("errors").and_then(|v| v.as_array()) {
            if !errors.is_empty() {
                return Err(StoreError::Query(format!(
                    "Neo4j query errors: {:?}",
                    errors
                )));
            }
        }

        // 提取结果行（列名映射 + graph 格式）
        let mut all_rows = Vec::new();
        if let Some(results) = raw.get("results").and_then(|v| v.as_array()) {
            for result in results {
                let columns: Vec<&str> = result
                    .get("columns")
                    .and_then(|c| c.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();

                if let Some(data) = result.get("data").and_then(|v| v.as_array()) {
                    for entry in data {
                        let row_val = match entry.get("row") {
                            Some(row) => row,
                            None => continue,
                        };

                        // graph 格式：包含 nodes/relationships 元数据
                        if let Some(_graph) = entry.get("graph") {
                            all_rows.push(flatten_graph_row(row_val, _graph, entry, &columns));
                        } else {
                            // 标量查询：将数组 [val1,val2] 映射为 {col1: val1, col2: val2}
                            all_rows.push(array_row_to_map(row_val, &columns));
                        }
                    }
                }
            }
        }

        Ok(all_rows)
    }
}

// ═══════════════════════════════════════════════════════════
// PropertyValue ↔ serde_json::Value
// ═══════════════════════════════════════════════════════════

fn property_to_json(value: &PropertyValue) -> JsonValue {
    match value {
        PropertyValue::String(s) => JsonValue::String(s.clone()),
        PropertyValue::Integer(i) => JsonValue::Number((*i).into()),
        PropertyValue::Float(f) => {
            serde_json::Number::from_f64(*f)
                .map(JsonValue::Number)
                .unwrap_or(JsonValue::Null)
        }
        PropertyValue::Boolean(b) => JsonValue::Bool(*b),
        PropertyValue::List(v) => JsonValue::Array(v.iter().map(property_to_json).collect()),
        PropertyValue::Map(m) => {
            let map: serde_json::Map<String, JsonValue> = m
                .iter()
                .map(|(k, v)| (k.clone(), property_to_json(v)))
                .collect();
            JsonValue::Object(map)
        }
        PropertyValue::Null => JsonValue::Null,
    }
}

fn json_to_property(value: &JsonValue) -> PropertyValue {
    match value {
        JsonValue::String(s) => PropertyValue::String(s.clone()),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                PropertyValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                PropertyValue::Float(f)
            } else {
                PropertyValue::String(n.to_string())
            }
        }
        JsonValue::Bool(b) => PropertyValue::Boolean(*b),
        JsonValue::Array(arr) => {
            PropertyValue::List(arr.iter().map(json_to_property).collect())
        }
        JsonValue::Object(obj) => {
            let map: HashMap<String, PropertyValue> = obj
                .iter()
                .map(|(k, v)| (k.clone(), json_to_property(v)))
                .collect();
            PropertyValue::Map(map)
        }
        JsonValue::Null => PropertyValue::Null,
    }
}

// ═══════════════════════════════════════════════════════════
// 响应解析：JSON 行 → Node / Relationship
// ═══════════════════════════════════════════════════════════

/// 从一行的指定列提取节点
fn extract_node(rows: &[JsonValue], col: &str) -> Result<Option<Node>, StoreError> {
    for row in rows {
        if let Some(node) = parse_node_from_row(row, col) {
            return Ok(Some(node));
        }
    }
    Ok(None)
}

/// 从 JSON 行解析节点
fn parse_node_from_row(row: &JsonValue, col: &str) -> Option<Node> {
    row.get(col).and_then(|n| parse_node(n))
}

/// 解析节点 JSON → Node
fn parse_node(node_json: &JsonValue) -> Option<Node> {
    // Neo4j HTTP 图格式中节点是 { "identity": 1, "labels": [...], "properties": {...} }
    let labels: Vec<String> = node_json
        .get("labels")
        .and_then(|l| l.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let properties: HashMap<String, PropertyValue> = node_json
        .get("properties")
        .and_then(|p| p.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), json_to_property(v)))
                .collect()
        })
        .unwrap_or_default();

    if labels.is_empty() && properties.is_empty() {
        return None;
    }

    Some(Node::new(labels, properties))
}

/// 从 JSON 行解析关系
fn parse_relationship_from_row(row: &JsonValue, col: &str) -> Option<Relationship> {
    let rel_json = row.get(col)?;
    parse_relationship(rel_json)
}

/// 解析关系 JSON → Relationship
/// Neo4j graph 格式: {"id":"179","type":"hasParent","startNode":"46","endNode":"47","properties":{}}
fn parse_relationship(rel_json: &JsonValue) -> Option<Relationship> {
    let rel_type = rel_json
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    if rel_type.is_empty() {
        return None;
    }

    // Neo4j graph 格式用 startNode/endNode（字符串），Bolt 用 start/end（数字）
    let start_id = rel_json.get("startNode").and_then(|s| s.as_str())
        .or_else(|| rel_json.get("start").and_then(|s| s.as_str()))
        .unwrap_or("")
        .to_string();

    let end_id = rel_json.get("endNode").and_then(|e| e.as_str())
        .or_else(|| rel_json.get("end").and_then(|e| e.as_str()))
        .unwrap_or("")
        .to_string();

    let properties: HashMap<String, PropertyValue> = rel_json
        .get("properties")
        .and_then(|p| p.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), json_to_property(v)))
                .collect()
        })
        .unwrap_or_default();

    Some(Relationship {
        rel_type,
        start_node_id: start_id,
        end_node_id: end_id,
        properties,
    })
}

/// 将数组行 [val1, val2] 转换为列名映射 {col1: val1, col2: val2}
fn array_row_to_map(row: &JsonValue, columns: &[&str]) -> JsonValue {
    let arr = match row.as_array() {
        Some(a) => a,
        None => return row.clone(),
    };
    let mut map = serde_json::Map::new();
    for (i, val) in arr.iter().enumerate() {
        let col = columns.get(i).map(|s| s.to_string())
            .unwrap_or_else(|| format!("col_{}", i));
        map.insert(col, val.clone());
    }
    JsonValue::Object(map)
}

/// 从 graph 格式中提取节点/关系的完整信息，使用 meta + graph 映射
/// Neo4j 返回: row=[{props}, {}, {props}], meta=[{id,type}, {id,type}, {id,type}],
///   graph={nodes:[{id,labels,properties}], relationships:[{id,type,start,end,properties}]}
fn flatten_graph_row(
    row: &JsonValue,
    graph: &JsonValue,
    entry: &JsonValue,
    columns: &[&str],
) -> JsonValue {
    let graph_nodes = graph.get("nodes").and_then(|v| v.as_array());
    let graph_rels = graph.get("relationships").and_then(|v| v.as_array());
    let meta = entry.get("meta").and_then(|v| v.as_array());

    let row_arr = match row.as_array() {
        Some(arr) => arr,
        None => return array_row_to_map(row, columns),
    };

    // 构建 graph element id → element 的索引（graph 中 id 是字符串，meta 中 id 是数字）
    let mut node_by_id: HashMap<i64, &JsonValue> = HashMap::new();
    let mut rel_by_id: HashMap<i64, &JsonValue> = HashMap::new();
    if let Some(arr) = graph_nodes {
        for n in arr {
            if let Some(id) = n.get("id").and_then(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()) {
                node_by_id.insert(id, n);
            }
        }
    }
    if let Some(arr) = graph_rels {
        for r in arr {
            if let Some(id) = r.get("id").and_then(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()) {
                rel_by_id.insert(id, r);
            }
        }
    }

    let mut map = serde_json::Map::new();
    for (i, val) in row_arr.iter().enumerate() {
        let col = columns.get(i).map(|s| s.to_string())
            .unwrap_or_else(|| format!("col_{}", i));

        // 查询 meta 确定此列是 node 还是 relationship
        if let Some(meta_arr) = meta {
            if let Some(m) = meta_arr.get(i) {
                let mtype = m.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let mid = m.get("id").and_then(|v| v.as_i64());

                if mtype == "node" {
                    if let Some(id) = mid {
                        if let Some(gnode) = node_by_id.get(&id) {
                            map.insert(col, (*gnode).clone());
                            continue;
                        }
                    }
                } else if mtype == "relationship" {
                    if let Some(id) = mid {
                        if let Some(grel) = rel_by_id.get(&id) {
                            map.insert(col, (*grel).clone());
                            continue;
                        }
                    }
                }
            }
        }

        // fallback: 用原始 row 值
        map.insert(col, val.clone());
    }

    JsonValue::Object(map)
}

// ═══════════════════════════════════════════════════════════
// Cypher 构建工具
// ═══════════════════════════════════════════════════════════

fn build_match_clause(pattern: &GraphPattern) -> String {
    let start = builder::build_node_match(&pattern.start);
    let rel = builder::build_rel_match(&pattern.relationship);
    let end = builder::build_node_match(&pattern.end);
    format!("MATCH {}{}{}", start, rel, end)
}

fn build_pattern_params(pattern: &GraphPattern) -> JsonValue {
    let mut params = serde_json::Map::new();
    // 使用和 builder::build_node_match 一致的参数命名：{var}_{idx}
    let start_var = pattern.start.variable.as_deref().unwrap_or("s");
    let rel_var = pattern.relationship.variable.as_deref().unwrap_or("r");
    let end_var = pattern.end.variable.as_deref().unwrap_or("e");
    collect_node_params(&mut params, &pattern.start, start_var);
    collect_rel_params(&mut params, &pattern.relationship, rel_var);
    collect_node_params(&mut params, &pattern.end, end_var);
    JsonValue::Object(params)
}

fn collect_node_params(
    params: &mut serde_json::Map<String, JsonValue>,
    node: &NodePattern,
    var_prefix: &str,
) {
    for (i, (_key, value)) in node.properties.iter().enumerate() {
        let pname = format!("{}_{}", var_prefix, i);
        params.insert(pname, property_to_json(value));
    }
}

fn collect_rel_params(
    params: &mut serde_json::Map<String, JsonValue>,
    rel: &RelationshipPattern,
    var_prefix: &str,
) {
    for (i, (_key, value)) in rel.properties.iter().enumerate() {
        let pname = format!("{}_{}", var_prefix, i);
        params.insert(pname, property_to_json(value));
    }
}

/// 简易 Base64 编码（避免引入 base64 crate）
fn base64_encode(input: &str) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut result = String::new();

    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);

        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }

    result
}
