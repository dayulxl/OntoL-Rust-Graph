//! GET /schema — ASW 知识图谱数据模型 JSON Schema。

use super::super::server::json_error;
use crate::app::AppState;
use ontology_storage::mapper::unified_mapping;
use std::sync::{Arc, Mutex};

pub fn handle(state: &Arc<Mutex<AppState>>) -> (u16, String) {
    let app = match state.lock() {
        Ok(a) => a,
        Err(e) => return (500, json_error(format!("Lock error: {}", e))),
    };

    let entity_count = app
        .repo
        .get_nodes_by_label(unified_mapping::ENTITY_LABEL)
        .map(|v| v.len())
        .unwrap_or(0);
    let type_count = app
        .repo
        .get_nodes_by_label(unified_mapping::TYPE_LABEL)
        .map(|v| v.len())
        .unwrap_or(0);

    let schema = serde_json::json!({
        "domain": "Anti-SubmarineWarfare",
        "labels": {
            "Entity": {
                "description": "实体节点：装备/舰船/传感器/任务",
                "count": entity_count,
                "standard_fields": {
                    "id":           { "type": "string (UUID)",  "description": "主键" },
                    "graph_id":     { "type": "string (UUID)",  "description": "图内标识" },
                    "code":         { "type": "string",         "description": "唯一业务编码（UNIQUE）" },
                    "name":         { "type": "string",         "description": "人类可读名称" },
                    "type":         { "type": "string",         "description": "分类名，关联 Type 节点" },
                    "description":  { "type": "string",         "description": "描述" },
                    "domain":       { "type": "string",         "description": "业务域" },
                    "leven":        { "type": "int",            "description": "层级" },
                    "status":       { "type": "string",         "description": "状态：有效/无效" },
                    "confidence":   { "type": "float",          "description": "置信度 0–1" },
                    "version":      { "type": "string",         "description": "版本号" },
                    "cope_version": { "type": "string",         "description": "协同版本号" },
                    "source":       { "type": "string",         "description": "数据来源" },
                    "owner":        { "type": "string",         "description": "所有者" },
                    "update_time":  { "type": "datetime",       "description": "最后更新时间" },
                    "create_time":  { "type": "datetime",       "description": "创建时间" },
                    "Space_abs":    { "type": "list[float]",    "description": "[纬度, 经度, 水下深度, 水面高度]" },
                    "command_side": { "type": "int",            "description": "0红方/1蓝方/2中立/3不确定" },
                    "precondition": { "type": "string",         "description": "前置条件（SWRL语法）" },
                    "effect":       { "type": "string",         "description": "执行效果（SWRL语法）" },
                    "cost":         { "type": "string",         "description": "资源消耗（SWRL语法）" },
                    "duration":     { "type": "int",            "description": "持续时间（秒）" },
                    "priority":     { "type": "int",            "description": "行为优先级" },
                    "composedOf":   { "type": "string",         "description": "子动作组合（SWRL语法）" }
                }
            },
            "Type": {
                "description": "分类层级节点",
                "count": type_count,
                "fields": {
                    "name": { "type": "string", "description": "分类名" }
                }
            }
        },
        "relationships": {
            "subClassOf": {
                "direction": "(Type)-[:subClassOf]->(Type)",
                "description": "分类层级：子类→父类 (rdfs:subClassOf)"
            },
            "移动": {
                "direction": "(Entity)-[:移动]->(Entity)",
                "description": "实体间移动关系，含 duration 属性"
            }
        }
    });

    (200, schema.to_string())
}
