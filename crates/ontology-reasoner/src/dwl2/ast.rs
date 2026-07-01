//! DWL2 DL 类表达式 AST。
//!
//! 描述逻辑（Description Logic）的核心构造：
//! - 原子概念（`ClassName`）和原子角色（`PropertyName`）
//! - 构造子：合取、析取、否定
//! - 量词：全称（∀）、存在（∃）
//! - 基数约束：最小、最大、精确
//!
//! ## DWL2 DL 子语言对照
//!
//! | 构造          | DL 语法              | AST 变体                    |
//! |---------------|----------------------|-----------------------------|
//! | 原子类        | `A`                  | `ClassName(A)`              |
//! | 合取          | `C ⊓ D`             | `Intersection(C, D)`        |
//! | 析取          | `C ⊔ D`             | `Union(C, D)`               |
//! | 否定          | `¬C`                 | `Complement(C)`             |
//! | 全称量词      | `∀R.C`              | `AllValuesFrom(R, C)`       |
//! | 存在量词      | `∃R.C`              | `SomeValuesFrom(R, C)`      |
//! | 基数 ≥       | `≥ n R.C`           | `MinCardinality(n, R, C)`   |
//! | 基数 ≤       | `≤ n R.C`           | `MaxCardinality(n, R, C)`   |
//! | 基数 =       | `= n R.C`           | `ExactCardinality(n, R, C)` |
//! | 顶层          | `⊤`                  | `Top`                       |
//! | 底层          | `⊥`                  | `Bottom`                    |

use std::collections::HashMap;

/// 类表达式 — DWL2 DL 的核心 AST 节点。
///
/// 每个变体对应描述逻辑中的一种构造子。
/// 顶层（Top）和底层（Bottom）用于表示
/// 全集和空集。
#[derive(Debug, Clone, PartialEq)]
pub enum ClassExpression {
    /// ⊤ — 顶层概念，表示所有个体的集合
    Top,

    /// ⊥ — 底层概念，空集
    Bottom,

    /// 原子类名（如 `Person`, `Animal`）
    ClassName(String),

    /// C ⊓ D — 合取（交集）
    Intersection(Box<ClassExpression>, Box<ClassExpression>),

    /// C ⊔ D — 析取（并集）
    Union(Box<ClassExpression>, Box<ClassExpression>),

    /// ¬C — 否定（补集）
    Complement(Box<ClassExpression>),

    /// ∀R.C — 全称量词：所有通过关系 R 可达的个体都属于 C
    AllValuesFrom {
        /// 关系/属性的 IRI
        property: String,
        /// 填充子概念
        filler: Box<ClassExpression>,
    },

    /// ∃R.C — 存在量词：存在通过关系 R 到达的个体属于 C
    SomeValuesFrom {
        property: String,
        filler: Box<ClassExpression>,
    },

    /// ≥ n R.C — 最小基数约束
    MinCardinality {
        n: u32,
        property: String,
        filler: Box<ClassExpression>,
    },

    /// ≤ n R.C — 最大基数约束
    MaxCardinality {
        n: u32,
        property: String,
        filler: Box<ClassExpression>,
    },

    /// = n R.C — 精确基数约束
    ExactCardinality {
        n: u32,
        property: String,
        filler: Box<ClassExpression>,
    },

    /// {a, b, ...} — 枚举个体集合（Nominal）
    OneOf(Vec<String>),

    /// ∃R.Self — 自反性约束
    SelfRestriction(String),
}

/// 量词标签 — 用于 `PropertyRestriction` 中区分全称/存在
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quantifier {
    Universal,
    Existential,
}

/// 基数约束标签
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    Min(u32),
    Max(u32),
    Exact(u32),
}

/// 属性约束（量词 + 基数）
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyRestriction {
    /// ∀R.C 或 ∃R.C
    Quantified {
        quantifier: Quantifier,
        property: String,
        filler: ClassExpression,
    },

    /// ≥n/≤n/=n R.C
    Cardinality {
        cardinality: Cardinality,
        property: String,
        filler: ClassExpression,
    },
}

// ═══════════════════════════════════════════════════════════
// ClassExpression 构造便利方法
// ═══════════════════════════════════════════════════════════

impl ClassExpression {
    /// 构建原子类名
    pub fn class(name: impl Into<String>) -> Self {
        ClassExpression::ClassName(name.into())
    }

    /// C ⊓ D
    pub fn and(self, other: ClassExpression) -> Self {
        ClassExpression::Intersection(Box::new(self), Box::new(other))
    }

    /// C ⊔ D
    pub fn or(self, other: ClassExpression) -> Self {
        ClassExpression::Union(Box::new(self), Box::new(other))
    }

    /// ¬C
    pub fn not(self) -> Self {
        ClassExpression::Complement(Box::new(self))
    }

    /// ∃R.C
    pub fn some(property: impl Into<String>, filler: ClassExpression) -> Self {
        ClassExpression::SomeValuesFrom {
            property: property.into(),
            filler: Box::new(filler),
        }
    }

    /// ∀R.C
    pub fn all(property: impl Into<String>, filler: ClassExpression) -> Self {
        ClassExpression::AllValuesFrom {
            property: property.into(),
            filler: Box::new(filler),
        }
    }

    /// ≥ n R.C
    pub fn min(n: u32, property: impl Into<String>, filler: ClassExpression) -> Self {
        ClassExpression::MinCardinality {
            n,
            property: property.into(),
            filler: Box::new(filler),
        }
    }

    /// ≤ n R.C
    pub fn max(n: u32, property: impl Into<String>, filler: ClassExpression) -> Self {
        ClassExpression::MaxCardinality {
            n,
            property: property.into(),
            filler: Box::new(filler),
        }
    }

    /// = n R.C
    pub fn exact(n: u32, property: impl Into<String>, filler: ClassExpression) -> Self {
        ClassExpression::ExactCardinality {
            n,
            property: property.into(),
            filler: Box::new(filler),
        }
    }

    /// 序列化为文本 key（用于 SWRL Query atom 嵌入）
    ///
    /// 格式: `Variant(arg1,arg2,...)` 递归嵌套
    pub fn to_key(&self) -> String {
        match self {
            ClassExpression::Top => "Top".to_string(),
            ClassExpression::Bottom => "Bottom".to_string(),
            ClassExpression::ClassName(n) => format!("ClassName({})", n),
            ClassExpression::Intersection(l, r) => format!("Intersection({},{})", l.to_key(), r.to_key()),
            ClassExpression::Union(l, r) => format!("Union({},{})", l.to_key(), r.to_key()),
            ClassExpression::Complement(c) => format!("Complement({})", c.to_key()),
            ClassExpression::AllValuesFrom { property, filler } => {
                format!("AllValuesFrom({},{})", property, filler.to_key())
            }
            ClassExpression::SomeValuesFrom { property, filler } => {
                format!("SomeValuesFrom({},{})", property, filler.to_key())
            }
            ClassExpression::MinCardinality { n, property, filler } => {
                format!("MinCardinality({},{},{})", n, property, filler.to_key())
            }
            ClassExpression::MaxCardinality { n, property, filler } => {
                format!("MaxCardinality({},{},{})", n, property, filler.to_key())
            }
            ClassExpression::ExactCardinality { n, property, filler } => {
                format!("ExactCardinality({},{},{})", n, property, filler.to_key())
            }
            ClassExpression::OneOf(iris) => {
                format!("OneOf({})", iris.join(";"))
            }
            ClassExpression::SelfRestriction(p) => format!("SelfRestriction({})", p),
        }
    }

    /// 从文本 key 反序列化（SWRL Query atom 解析用）
    pub fn from_key(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if s == "Top" { return Ok(ClassExpression::Top); }
        if s == "Bottom" { return Ok(ClassExpression::Bottom); }

        if let Some(inner) = strip_prefix(s, "ClassName(") {
            return Ok(ClassExpression::class(inner));
        }
        if let Some(inner) = strip_prefix(s, "SelfRestriction(") {
            return Ok(ClassExpression::SelfRestriction(inner.to_string()));
        }
        if let Some(inner) = strip_prefix(s, "Complement(") {
            return ClassExpression::from_key(inner).map(|c| c.not());
        }
        if let Some(inner) = strip_prefix(s, "AllValuesFrom(") {
            let parts = split_at_comma(inner, 1)?;
            return Ok(ClassExpression::all(parts[0], ClassExpression::from_key(parts[1])?));
        }
        if let Some(inner) = strip_prefix(s, "SomeValuesFrom(") {
            let parts = split_at_comma(inner, 1)?;
            return Ok(ClassExpression::some(parts[0], ClassExpression::from_key(parts[1])?));
        }
        if let Some(inner) = strip_prefix(s, "Intersection(") {
            let parts = split_at_comma(inner, 1)?;
            return Ok(ClassExpression::from_key(parts[0])?.and(ClassExpression::from_key(parts[1])?));
        }
        if let Some(inner) = strip_prefix(s, "Union(") {
            let parts = split_at_comma(inner, 1)?;
            return Ok(ClassExpression::from_key(parts[0])?.or(ClassExpression::from_key(parts[1])?));
        }
        if let Some(inner) = strip_prefix(s, "MinCardinality(") {
            let parts = split_at_comma(inner, 2)?;
            let n: u32 = parts[0].parse().map_err(|e| format!("bad cardinality: {}", e))?;
            return Ok(ClassExpression::min(n, parts[1], ClassExpression::from_key(parts[2])?));
        }
        if let Some(inner) = strip_prefix(s, "MaxCardinality(") {
            let parts = split_at_comma(inner, 2)?;
            let n: u32 = parts[0].parse().map_err(|e| format!("bad cardinality: {}", e))?;
            return Ok(ClassExpression::max(n, parts[1], ClassExpression::from_key(parts[2])?));
        }
        if let Some(inner) = strip_prefix(s, "ExactCardinality(") {
            let parts = split_at_comma(inner, 2)?;
            let n: u32 = parts[0].parse().map_err(|e| format!("bad cardinality: {}", e))?;
            return Ok(ClassExpression::exact(n, parts[1], ClassExpression::from_key(parts[2])?));
        }
        if let Some(inner) = strip_prefix(s, "OneOf(") {
            let iris: Vec<String> = inner.split(';').map(|s| s.trim().to_string()).collect();
            return Ok(ClassExpression::OneOf(iris));
        }
        Err(format!("Unknown ClassExpression key: {}", s))
    }
}

/// 剥离 prefix，返回括号内内容和去除 )
fn strip_prefix<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.starts_with(prefix) && s.ends_with(')') {
        Some(&s[prefix.len()..s.len() - 1])
    } else {
        None
    }
}

/// 在有嵌套括号的情况下按顶级逗号分割
fn split_at_comma(s: &str, expected: usize) -> Result<Vec<&str>, String> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => { if depth > 0 { depth -= 1; } }
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    if parts.len() != expected + 1 {
        return Err(format!("expected {} parts, got {} in '{}'", expected + 1, parts.len(), s));
    }
    Ok(parts.iter().map(|p| p.trim()).collect())
}

// ═══════════════════════════════════════════════════════════
// 本体模型索引 — 推理时使用的轻量结构
// ═══════════════════════════════════════════════════════════

/// 本体 Class 的图内表示（用于推理查询结果）
#[derive(Debug, Clone)]
pub struct OntologyClass {
    pub iri: String,
    pub label: Option<String>,
    /// 等价类表达式（如 `Person ≡ Animal ⊓ Rational`）
    pub equivalent_to: Option<ClassExpression>,
    /// 父类 IRI 列表（subclass 关系）
    pub sub_class_of: Vec<String>,
}

/// 本体 ObjectProperty 的图内表示
#[derive(Debug, Clone)]
pub struct OntologyProperty {
    pub iri: String,
    pub label: Option<String>,
    pub domain: Option<String>,
    pub range: Option<String>,
    /// 超属性链（如 hasParent ◦ hasParent → hasGrandparent）
    pub super_property_chain: Vec<Vec<String>>,
    /// 传递性
    pub transitive: bool,
    /// 对称性
    pub symmetric: bool,
    /// 函数性
    pub functional: bool,
}

/// 本体 Individual 的图内表示
#[derive(Debug, Clone)]
pub struct OntologyIndividual {
    pub iri: String,
    pub label: Option<String>,
    pub types: Vec<String>,
    /// 属性值（property_iri → value）
    pub property_values: HashMap<String, String>,
    /// 对象属性关系（property_iri → target_individual_iri）
    pub object_relations: Vec<(String, String)>,
}

// ═══════════════════════════════════════════════════════════
// 推理查询 — DWL2 DL 查询输入
// ═══════════════════════════════════════════════════════════

/// DWL2 DL 查询。
///
/// 查询指定类表达式的全部实例（个体），或检查包含关系。
#[derive(Debug, Clone)]
pub struct Dwl2Query {
    /// 要查询的类表达式
    pub expression: ClassExpression,

    /// 查询类型
    pub query_type: QueryType,
}

/// 查询类型：实例检索 vs 包含检查
#[derive(Debug, Clone)]
pub enum QueryType {
    /// 检索满足表达式的所有个体
    RetrieveInstances,

    /// 检查 `sub_class` 是否被 `super_class` 包含
    IsSubClassOf {
        sub_class: String,
        super_class: ClassExpression,
    },

    /// 检查个体 `individual` 是否属于表达式描述的类
    IsInstanceOf {
        individual_iri: String,
    },
}

// ═══════════════════════════════════════════════════════════
// 查询结果
// ═══════════════════════════════════════════════════════════

/// DWL2 DL 查询结果
#[derive(Debug, Clone)]
pub struct Dwl2Result {
    /// 匹配的个体 IRI 集合
    pub individuals: Vec<String>,

    /// 是否为包含关系成立的结果
    pub subsumption_holds: Option<bool>,

    /// 查询执行时间（毫秒）
    pub elapsed_ms: u64,
}
