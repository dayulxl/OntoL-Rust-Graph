//! QueryPlan — 推理层的查询抽象。
//!
//! 推理层不直接构造 `GraphPattern`，而是通过 `QueryPlan` 描述查询意图，
//! 由存储适配器自行翻译为后端查询（Cypher / in-memory scan）。
//!
//! ## 设计目的
//!
//! 解耦推理层与存储层内部 IR（`mapper::graph::pattern`）。
//! `GraphRepository::execute_plan(plan)` 替代 `query_pattern(pattern)`。
//!
//! ## 覆盖的语言
//!
//! | QueryPlan 变体 | 语言前缀 | 用途 |
//! |---------------|---------|------|
//! | `GetParentTypes` | `rdfs:` / `owl2:` | 属性继承 — 查找父类型 |
//! | `DiscoverDownstream` | 全部推理前缀 | BFS 沿推理边发现下游本体 |
//! | `RuleMatch` | `swrl:` | SWRL 规则前提 → 图模式匹配 |
//! | `ValidateConstraint` | `sh:` | SHACL 约束验证 |
//! | `JsonPathLookup` | `$.` | JSONPath 路径提取 |
//! | `CallFunction` | `func:` | 自定义函数调用 |

use std::collections::HashMap;

use crate::mapper::graph::node::Node;
use crate::mapper::graph::property::PropertyValue;
use crate::mapper::graph::relationship::Relationship;

// ═══════════════════════════════════════════════════════════
// QueryPlan
// ═══════════════════════════════════════════════════════════

/// 推理层查询抽象 — 不绑定任何后端。
#[derive(Debug, Clone)]
pub enum QueryPlan {
    // ── 现有 ──
    /// 按 code 获取单个节点。
    GetByCode(String),
    /// 按标签获取所有节点。
    GetByLabel(String),
    /// 获取节点的出向关系（可选按类型过滤）。
    GetRelationships {
        node_code: String,
        rel_type: Option<String>,
    },
    /// 图模式匹配: `(start_label) -[rel_type]-> (end_label)`。
    PatternMatch {
        start_label: Option<String>,
        rel_type: Option<String>,
        end_label: Option<String>,
        end_properties: Vec<(String, PropertyValue)>,
    },

    // ── 新增：属性继承（rdfs: / owl2:）──
    /// 获取节点的所有父类型（沿 INSTANCE_OF / subClassOf 链）。
    /// → `MATCH (c)-[:INSTANCE_OF|subClassOf]->(p:Type) WHERE c.code = $code RETURN p`
    GetParentTypes {
        child_code: String,
    },

    // ── 新增：下游发现 ──
    /// 沿指定关系类型发现下游原实体（`cope_version` 为空）。
    /// `rel_types` 为空时不过滤关系类型。
    DiscoverDownstream {
        node_code: String,
        /// 仅沿这些关系类型发现（空 = 全部）。
        rel_types: Vec<String>,
        /// 仅发现匹配此版本的原实体（通常传空字符串查原实体）。
        cope_version: String,
    },

    // ── 新增：规则匹配（swrl:）──
    /// 按原子模式匹配规则前提，返回变量绑定。
    RuleMatch {
        /// SWRL 规则名（用于日志/审计）。
        rule_name: String,
        /// 前提原子列表。
        atom_patterns: Vec<AtomPattern>,
    },

    // ── 新增：约束验证（sh:）──
    /// 对节点执行 SHACL 约束验证。
    ValidateConstraint {
        node_code: String,
        /// 约束列表。
        constraints: Vec<ConstraintSpec>,
    },

    // ── 新增：JSONPath 提取（$.）──
    /// 沿 JSONPath 定位值。
    /// `$.position.lat` → 拆为 `["position", "lat"]` 逐段查找。
    JsonPathLookup {
        node_code: String,
        /// 已拆分的路径段（不含 `$.` 前缀）。
        segments: Vec<String>,
    },

    // ── 新增：自定义函数（func:）──
    /// 调用自定义函数（func: JSON 翻译目标）。
    /// 适配器内部翻译为对应的图操作。
    CallFunction {
        func_name: String,
        target_id: String,
        params: HashMap<String, PropertyValue>,
    },
}

// ═══════════════════════════════════════════════════════════
// QueryPlan 辅助类型
// ═══════════════════════════════════════════════════════════

/// SWRL 规则前提原子 → 查询意图描述。
#[derive(Debug, Clone)]
pub enum AtomPattern {
    /// 类原子: `Person(?x)` → 变量 ?x 的节点必须有标签 Person。
    ClassAtom {
        class_name: String,
        variable: String,
    },
    /// 属性原子: `hasAge(?x, ?age)` → 节点 ?x 沿 hasAge 边连到 ?age 值。
    PropertyAtom {
        property: String,
        subject_var: String,
        object_var: String,
    },
    /// 同一性原子: `sameAs(?x, ?y)` → ?x 和 ?y 是同一节点。
    SameAs {
        var_a: String,
        var_b: String,
    },
    /// 不等原子: `differentFrom(?x, ?y)` → ?x 和 ?y 是不同节点。
    DifferentFrom {
        var_a: String,
        var_b: String,
    },
    /// 内置原子: `swrlb:greaterThan(?age, 18)` → 比较/数学运算。
    Builtin {
        builtin_iri: String,
        args: Vec<String>,
    },
}

/// SHACL 约束 → 查询意图描述。
#[derive(Debug, Clone)]
pub enum ConstraintSpec {
    /// 属性必须存在且非空: `required(prop)`。
    Required { property: String },
    /// 属性存在: `exists(prop)`。
    Exists { property: String },
    /// 属性等于指定值: `prop = value`。
    Equals { property: String, value: PropertyValue },
    /// 属性不等于指定值: `prop != value`。
    NotEquals { property: String, value: PropertyValue },
    /// 属性大于等于: `prop >= N`。
    MinInclusive { property: String, value: f64 },
    /// 属性小于等于: `prop <= N`。
    MaxInclusive { property: String, value: f64 },
    /// 属性大于: `prop > N`。
    MinExclusive { property: String, value: f64 },
    /// 属性小于: `prop < N`。
    MaxExclusive { property: String, value: f64 },
    /// 正则匹配: `prop matches "re"`。
    Pattern { property: String, regex: String },
    /// 属性值必须在列表中: `prop IN [...]`。
    InValues {
        property: String,
        values: Vec<PropertyValue>,
    },
}

// ═══════════════════════════════════════════════════════════
// QueryResult
// ═══════════════════════════════════════════════════════════

/// 查询结果。
#[derive(Debug, Clone)]
pub enum QueryResult {
    // ── 现有 ──
    Single(Option<Node>),
    List(Vec<Node>),
    Relationships(Vec<Relationship>),
    PatternMatches(Vec<(Node, Vec<Relationship>, Node)>),

    // ── 新增 ──
    /// `GetParentTypes` 返回 — 父类型节点列表。
    ParentTypes(Vec<Node>),
    /// `DiscoverDownstream` 返回 — 下游原实体的 code 列表。
    DownstreamCodes(Vec<String>),
    /// `RuleMatch` 返回 — 变量绑定列表（每行是一个绑定组合）。
    RuleBindings(Vec<HashMap<String, PropertyValue>>),
    /// `ValidateConstraint` 返回 — 约束是否通过。
    ConstraintPassed(bool),
    /// `ValidateConstraint` 返回 — 带详细信息的验证结果。
    ConstraintDetail {
        passed: bool,
        failed_constraints: Vec<String>,
    },
    /// `JsonPathLookup` 返回 — 路径定位到的值。
    JsonValue(Option<PropertyValue>),
    /// `CallFunction` 返回 — 函数执行状态。
    FunctionCalled { success: bool, message: String },
}
