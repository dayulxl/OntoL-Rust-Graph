//! # 动作路由器
//!
//! 按语言前缀和边属性将动作路由到对应引擎。
//!
//! ## 路由表
//!
//! | 前缀 / actionType | 路由目标 |
//! |-------------------|---------|
//! | `rdfs:` / `owl2:` | Phase 1 — 事实层（属性继承） |
//! | `swrl:` | Phase 2 — 规则层（规则匹配） |
//! | `sh:` | Phase 2 — 规则层（约束验证） |
//! | `rule:` | 推理方向设定 |
//! | `func:` | Phase 3 — 自定义函数调用 |
//! | `$.` | Phase 3 — JSONPath 提取 |
//! | `actionType = "inference"` | GIE 推理机处理 |
//! | 其他 actionType | LLM 关系（跳过） |
//! | 无前缀领域关系 | 非推理边（跳过） |

use ontology_storage::mapper::graph::relationship::Relationship;
use ontology_storage::mapper::unified_mapping::{
    ACTION_TYPE_KEY, FUNC_KEY, FIELD_ID_KEY,
};

/// 动作分类结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionTarget {
    /// Phase 1 — 事实层: rdfs:/owl2: 前缀（属性继承、类层次）
    Facts,
    /// Phase 2 — 规则层: swrl:/sh: 前缀（规则匹配、约束验证）
    Rules,
    /// Phase 3 — 数据提取: func:/$. 前缀（函数调用、JSONPath）
    Extract,
    /// 推理方向设定: rule: 前缀
    Direction,
    /// LLM 关系 — actionType 不是 "inference"
    LlmOnly,
    /// 非推理边 — 无前缀的领域关系
    NonInference,
    /// 自定义函数调用（func: 字段）
    CustomFunc {
        func_name: String,
        target_id: String,
    },
}

/// 路由一个关系边，判断其推理目标。
///
/// 优先级: actionType > 关系类型前缀 > 无前缀
pub fn route_relationship(rel: &Relationship) -> ActionTarget {
    // 1. 检查 actionType 边属性
    if let Some(action_type) = rel.properties.get(ACTION_TYPE_KEY) {
        if let Some(s) = action_type.as_str() {
            if s == "inference" {
                // actionType = "inference" → 交由推理机处理
                // 继续检查关系类型前缀确定具体阶段
            } else {
                // 其他 actionType → LLM 关系，推理机跳过
                return ActionTarget::LlmOnly;
            }
        }
    }

    // 2. 检查 func: 字段（自定义函数）
    if let Some(func_val) = rel.properties.get(FUNC_KEY) {
        if let Some(func_name) = func_val.as_str() {
            let target_id = rel.properties
                .get(FIELD_ID_KEY)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            return ActionTarget::CustomFunc {
                func_name: func_name.to_string(),
                target_id,
            };
        }
    }

    // 3. 按关系类型前缀分类
    let rel_type = &rel.rel_type;
    if let Some(prefix) = crate::language::classify_inference_prefix(rel_type) {
        match prefix {
            crate::language::LanguagePrefix::Rdfs | crate::language::LanguagePrefix::Owl2 => {
                ActionTarget::Facts
            }
            crate::language::LanguagePrefix::Swrl | crate::language::LanguagePrefix::Shacl => {
                ActionTarget::Rules
            }
            crate::language::LanguagePrefix::Rule => ActionTarget::Direction,
            crate::language::LanguagePrefix::Function => ActionTarget::Extract,
        }
    } else if crate::language::is_ontology_relation(rel_type) {
        // 本体语义层关系（subClassOf, INSTANCE_OF 等）→ 事实层
        ActionTarget::Facts
    } else {
        // 无前缀领域关系 → 非推理边
        ActionTarget::NonInference
    }
}
