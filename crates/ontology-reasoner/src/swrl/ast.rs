//! SWRL 规则 AST。
//!
//! ## 结构
//!
//! ```text
//! rule ::= name? atom* "->" atom*
//! atom ::= class_atom | property_atom | builtin_atom | same_as | different_from
//! class_atom    ::= IRI "(" variable ")"
//! property_atom ::= IRI "(" variable "," variable ")"
//! builtin_atom  ::= builtin_IRI "(" arg "," arg ")"
//! ```
//!
//! ## 变量绑定
//!
//! 推理过程中变量通过 `VariableBinding` 映射到图数据库中的个体 IRI。
//! 未绑定变量用 `?` 前缀表示（如 `?x`, `?y`）。

use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
// 变量系统
// ═══════════════════════════════════════════════════════════

/// SWRL 变量（以 `?` 前缀）
pub type Variable = String;

/// 变量绑定 — 变量名 → 个体 IRI
///
/// 推理引擎在匹配前提时逐步填充此映射，
/// 然后将其代入结论原子生成新事实。
pub type VariableBinding = HashMap<Variable, String>;

/// 变量绑定集合（一个规则可能有多个匹配，产生多个绑定）
pub type BindingSet = Vec<VariableBinding>;

// ═══════════════════════════════════════════════════════════
// SWRL Atom
// ═══════════════════════════════════════════════════════════

/// SWRL 原子 — 规则的前提或结论的最小单元。
///
/// 每个原子对应一个条件或推论：
/// - `C(?x)`        → 个体 `?x` 属于类 `C`
/// - `P(?x, ?y)`    → 个体 `?x` 和 `?y` 之间有关系 `P`
/// - `sameAs(?x, ?y)`   → `?x` 和 `?y` 是同一个个体
/// - `differentFrom(?x, ?y)` → `?x` 和 `?y` 是不同的个体
/// - `builtin(?x, ?y)` → 内置函数调用
#[derive(Debug, Clone, PartialEq)]
pub enum Atom {
    /// C(?x) — 类成员关系原子
    ClassAtom {
        /// 类 IRI
        class_iri: String,
        /// 个体变量
        variable: Variable,
    },

    /// P(?x, ?y) — 对象属性原子
    ObjectPropertyAtom {
        /// 属性 IRI
        property_iri: String,
        /// 主语（主体）变量
        subject: Variable,
        /// 宾语（客体）变量
        object: Variable,
    },

    /// D(?x, ?v) — 数据属性原子
    DataPropertyAtom {
        /// 数据属性 IRI
        property_iri: String,
        /// 主语变量
        subject: Variable,
        /// 值变量或常量
        value: String,
    },

    /// sameAs(?x, ?y) — 等价个体
    SameAs(Variable, Variable),

    /// differentFrom(?x, ?y) — 不等价个体
    DifferentFrom(Variable, Variable),

    /// 内置函数调用：`swrlb:greaterThan(?x, ?y)` 等
    Builtin {
        /// 内置函数 IRI（如 `swrlb:greaterThan`）
        builtin_iri: String,
        /// 参数列表
        arguments: Vec<String>,
    },

    /// DWL2 子查询：执行 DWL2 类表达式查询，将结果 IRI 绑定到变量
    Query {
        /// DWL2 表达式 key (ClassExpression::to_key())
        dwl2_expression: String,
        /// 绑定查询结果 IRI 的变量
        result_variable: Variable,
    },
}

impl Atom {
    /// 返回此原子引用的所有变量名
    pub fn variables(&self) -> Vec<&str> {
        match self {
            Atom::ClassAtom { variable, .. } => vec![variable.as_str()],
            Atom::ObjectPropertyAtom {
                subject, object, ..
            } => {
                vec![subject.as_str(), object.as_str()]
            }
            Atom::DataPropertyAtom { subject, value, .. } => {
                let mut vars = vec![subject.as_str()];
                if value.starts_with('?') {
                    vars.push(value.as_str());
                }
                vars
            }
            Atom::SameAs(a, b) | Atom::DifferentFrom(a, b) => {
                vec![a.as_str(), b.as_str()]
            }
            Atom::Builtin { arguments, .. } => arguments
                .iter()
                .filter(|a| a.starts_with('?'))
                .map(|a| a.as_str())
                .collect(),
            Atom::Query {
                result_variable, ..
            } => {
                vec![result_variable.as_str()]
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════
// SWRL 规则
// ═══════════════════════════════════════════════════════════

/// SWRL 规则 — `antecedent → consequent`
///
/// 语义：对于图中所有满足前提（antecedent）的变量绑定，
/// 推导出结论（consequent）中描述的新事实。
///
/// ## 示例
///
/// ```text
/// [parentChild: hasParent(?x, ?y) ^ hasBrother(?y, ?z) -> hasUncle(?x, ?z)]
/// ```
///
/// 对应的 AST：
/// ```text
/// Rule {
///   name: "parentChild",
///   antecedent: [
///     ObjectPropertyAtom("hasParent", "?x", "?y"),
///     ObjectPropertyAtom("hasBrother", "?y", "?z"),
///   ],
///   consequent: [
///     ObjectPropertyAtom("hasUncle", "?x", "?z"),
///   ],
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Rule {
    /// 规则名（可选，用于调试和错误报告）
    pub name: Option<String>,

    /// 前提原子集合（AND 语义）
    pub antecedent: Vec<Atom>,

    /// 结论原子集合（AND 语义）
    pub consequent: Vec<Atom>,

    /// 规则级备注
    pub comment: Option<String>,
}

impl Rule {
    /// 创建新规则
    pub fn new(name: impl Into<String>, antecedent: Vec<Atom>, consequent: Vec<Atom>) -> Self {
        Self {
            name: Some(name.into()),
            antecedent,
            consequent,
            comment: None,
        }
    }

    /// 创建匿名规则
    pub fn anonymous(antecedent: Vec<Atom>, consequent: Vec<Atom>) -> Self {
        Self {
            name: None,
            antecedent,
            consequent,
            comment: None,
        }
    }

    /// 返回前提中所有不同的变量名
    pub fn antecedent_variables(&self) -> Vec<&str> {
        let mut vars: Vec<&str> = self.antecedent.iter().flat_map(|a| a.variables()).collect();
        vars.sort();
        vars.dedup();
        vars
    }

    /// 返回结论中的所有变量名
    pub fn consequent_variables(&self) -> Vec<&str> {
        let mut vars: Vec<&str> = self.consequent.iter().flat_map(|a| a.variables()).collect();
        vars.sort();
        vars.dedup();
        vars
    }

    /// 检查规则的变量安全性。
    ///
    /// 安全规则：结论中的每个变量都必须在前提中出现过。
    /// 违反安全性的规则可能产生无限推论，这是 SWRL 的硬性约束。
    pub fn is_safe(&self) -> bool {
        let ant_vars: Vec<&str> = self.antecedent_variables();
        let con_vars = self.consequent_variables();
        con_vars.iter().all(|cv| ant_vars.contains(cv))
    }
}

// ═══════════════════════════════════════════════════════════
// 推理结果
// ═══════════════════════════════════════════════════════════

/// 单次推理步骤的结果
#[derive(Debug, Clone)]
pub struct InferenceResult {
    /// 触发的规则名
    pub rule_name: String,

    /// 新推导出的事实（以 Atom 表示）
    pub derived_facts: Vec<Atom>,

    /// 本次推理的置信度
    pub confidence: f64,

    /// 匹配到的变量绑定数
    pub binding_count: usize,
}

/// 规则执行统计
#[derive(Debug, Clone, Default)]
pub struct ExecutionStats {
    /// 总推理步数（fixpoint 迭代次数）
    pub total_steps: usize,

    /// 推导出的总事实数
    pub total_derived: usize,

    /// 因置信度熔断而中止的次数
    pub fuse_trips: usize,

    /// 总耗时（毫秒）
    pub total_ms: u64,
}
