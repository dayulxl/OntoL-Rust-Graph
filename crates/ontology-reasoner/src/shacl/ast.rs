//! SHACL 形状 AST。
//!
//! 适配属性图的 SHACL 约束表达层。
//! 参考 W3C SHACL (Shapes Constraint Language) 规范，
//! 将 RDF 术语映射到属性图模型：
//!
//! | RDF 概念      | 属性图映射            |
//! |---------------|-----------------------|
//! | `sh:targetClass`  | 节点标签 (label)       |
//! | `sh:property`     | 出边属性 (relationship)|
//! | `sh:nodeKind`     | 节点类型约束           |
//! | `sh:pattern`      | 属性值正则匹配         |
//!
//! # 核心类型
//!
//! - [`Shape`] — 形状顶层定义（NodeShape / PropertyShape）
//! - [`Target`] — 验证目标声明
//! - [`Constraint`] — 约束条件集合
//! - [`PropertyPath`] — 属性路径（单跳 / 链式）

use std::collections::HashMap;

use ontology_storage::mapper::graph::property::PropertyValue;

// ═══════════════════════════════════════════════════════════
// 形状定义
// ═══════════════════════════════════════════════════════════

/// SHACL 形状 — 对图中节点的约束定义。
///
/// 支持两种形状类型：
/// - **NodeShape** — 节点级约束（标签、属性存在性、跨属性约束）
/// - **PropertyShape** — 属性级约束（值范围、基数、类型）
///
/// # 示例
///
/// ```rust,ignore
/// let shape = Shape::node("PersonShape")
///     .with_target(Target::target_class("Person"))
///     .with_property(
///         PropertyShape::new("hasAge")
///             .with_constraint(Constraint::min_inclusive(0.0))
///             .with_constraint(Constraint::max_inclusive(150.0))
///     )
///     .with_constraint(Constraint::min_count("hasName", 1));
/// ```
#[derive(Debug, Clone)]
pub enum Shape {
    /// 节点形状 — 直接约束目标节点的标签和属性
    NodeShape(NodeShape),
    /// 属性形状 — 约束目标节点的某条出边的值
    PropertyShape(PropertyShape),
}

impl Shape {
    /// 创建一个新的 NodeShape
    pub fn node(name: impl Into<String>) -> Self {
        Shape::NodeShape(NodeShape::new(name))
    }

    /// 创建一个新的 PropertyShape
    pub fn property(name: impl Into<String>) -> Self {
        Shape::PropertyShape(PropertyShape::new(name))
    }

    /// 返回形状名称
    pub fn name(&self) -> &str {
        match self {
            Shape::NodeShape(s) => &s.name,
            Shape::PropertyShape(s) => &s.name,
        }
    }

    // ── NodeShape 转发方法（仅对 NodeShape 变体有效）──

    /// 添加 Target（仅 NodeShape 变体）
    pub fn with_target(self, target: Target) -> Self {
        match self {
            Shape::NodeShape(ns) => Shape::NodeShape(ns.with_target(target)),
            other => other,
        }
    }

    /// 添加节点级 Constraint（仅 NodeShape 变体）
    pub fn with_constraint(self, constraint: Constraint) -> Self {
        match self {
            Shape::NodeShape(ns) => Shape::NodeShape(ns.with_constraint(constraint)),
            other => other,
        }
    }

    /// 添加 PropertyShape（仅 NodeShape 变体）
    pub fn with_property(self, prop: PropertyShape) -> Self {
        match self {
            Shape::NodeShape(ns) => Shape::NodeShape(ns.with_property(prop)),
            other => other,
        }
    }
}

/// 节点级形状定义。
///
/// 直接约束节点的标签、属性存在性、以及跨属性条件。
#[derive(Debug, Clone)]
pub struct NodeShape {
    /// 形状名称（用于错误报告和调试）
    pub name: String,
    /// 验证目标声明
    pub targets: Vec<Target>,
    /// 节点级约束
    pub constraints: Vec<Constraint>,
    /// 子属性形状（对节点出边的约束）
    pub property_shapes: Vec<PropertyShape>,
    /// 形状级描述
    pub description: Option<String>,
    /// 严重级别覆盖（默认 Violation）
    pub severity: Option<Severity>,
    /// 是否禁用此形状
    pub deactivated: bool,
    /// 扩展元数据
    pub metadata: HashMap<String, String>,
}

impl NodeShape {
    /// 创建新的 NodeShape
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            targets: Vec::new(),
            constraints: Vec::new(),
            property_shapes: Vec::new(),
            description: None,
            severity: None,
            deactivated: false,
            metadata: HashMap::new(),
        }
    }

    /// 添加一个 Target
    pub fn with_target(mut self, target: Target) -> Self {
        self.targets.push(target);
        self
    }

    /// 添加一个节点级 Constraint
    pub fn with_constraint(mut self, constraint: Constraint) -> Self {
        self.constraints.push(constraint);
        self
    }

    /// 添加一个 PropertyShape
    pub fn with_property(mut self, prop: PropertyShape) -> Self {
        self.property_shapes.push(prop);
        self
    }

    /// 设置描述
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// 设置严重级别
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = Some(severity);
        self
    }
}

/// 属性级形状定义。
///
/// 约束目标节点的某个属性（出边 / 数据属性）的值。
#[derive(Debug, Clone)]
pub struct PropertyShape {
    /// 形状名称
    pub name: String,
    /// 属性路径 — 目标节点到被约束值的路径
    pub path: PropertyPath,
    /// 属性级约束
    pub constraints: Vec<Constraint>,
    /// 属性描述
    pub description: Option<String>,
    /// 严重级别覆盖
    pub severity: Option<Severity>,
    /// 是否禁用
    pub deactivated: bool,
    /// 扩展元数据
    pub metadata: HashMap<String, String>,
}

impl PropertyShape {
    /// 创建新的 PropertyShape
    pub fn new(name: impl Into<String>) -> Self {
        let name_str = name.into();
        Self {
            path: PropertyPath::Predicate(name_str.clone()),
            name: name_str,
            constraints: Vec::new(),
            description: None,
            severity: None,
            deactivated: false,
            metadata: HashMap::new(),
        }
    }

    /// 指定属性路径
    pub fn with_path(mut self, path: PropertyPath) -> Self {
        self.path = path;
        self
    }

    /// 添加约束
    pub fn with_constraint(mut self, constraint: Constraint) -> Self {
        self.constraints.push(constraint);
        self
    }

    /// 设置描述
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// 设置严重级别
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = Some(severity);
        self
    }
}

// ═══════════════════════════════════════════════════════════
// 目标声明
// ═══════════════════════════════════════════════════════════

/// 验证目标 — 声明形状作用于图中的哪些节点。
#[derive(Debug, Clone)]
pub enum Target {
    /// 按节点标签匹配（等价于 `sh:targetClass`）
    TargetClass {
        /// 标签名称
        label: String,
    },
    /// 按节点 ID 匹配（等价于 `sh:targetNode`）
    TargetNode {
        /// 节点 ID
        node_id: String,
    },
    /// 按属性存在性匹配（等价于 `sh:targetSubjectsOf`）
    TargetSubjectsOf {
        /// 属性名 — 拥有此属性的节点被选中
        property: String,
    },
    /// 按被指向属性匹配（等价于 `sh:targetObjectsOf`）
    TargetObjectsOf {
        /// 属性名 — 被此属性指向的节点被选中
        property: String,
    },
    /// 所有节点（兜底，需显式声明）
    AllNodes,
}

impl Target {
    /// 按标签匹配目标
    pub fn target_class(label: impl Into<String>) -> Self {
        Target::TargetClass {
            label: label.into(),
        }
    }

    /// 按节点 ID 匹配目标
    pub fn target_node(node_id: impl Into<String>) -> Self {
        Target::TargetNode {
            node_id: node_id.into(),
        }
    }

    /// 按主语属性匹配目标
    pub fn target_subjects_of(property: impl Into<String>) -> Self {
        Target::TargetSubjectsOf {
            property: property.into(),
        }
    }

    /// 按客体属性匹配目标
    pub fn target_objects_of(property: impl Into<String>) -> Self {
        Target::TargetObjectsOf {
            property: property.into(),
        }
    }
}

// ═══════════════════════════════════════════════════════════
// 属性路径
// ═══════════════════════════════════════════════════════════

/// 属性路径 — 描述从目标节点到约束值之间的路径跨度。
///
/// 支持单跳谓词路径和链式路径。
#[derive(Debug, Clone)]
pub enum PropertyPath {
    /// 直接属性（单跳关系或属性名）
    Predicate(String),
    /// 链式路径 — 多跳串联
    Sequence(Vec<String>),
    /// 反向路径（`^`前缀，等价于 incoming relationship）
    InversePath(Box<PropertyPath>),
    /// 备选路径（`|`分隔，匹配任意一条即可）
    Alternative(Vec<PropertyPath>),
    /// 零或多次（`*`）
    ZeroOrMore(Box<PropertyPath>),
    /// 一或多次（`+`）
    OneOrMore(Box<PropertyPath>),
}

impl PropertyPath {
    /// 创建一个直接属性路径
    pub fn predicate(name: impl Into<String>) -> Self {
        PropertyPath::Predicate(name.into())
    }

    /// 创建一个链式路径
    pub fn sequence(steps: Vec<&str>) -> Self {
        PropertyPath::Sequence(steps.into_iter().map(String::from).collect())
    }

    /// 创建一个反向路径
    pub fn inverse(path: PropertyPath) -> Self {
        PropertyPath::InversePath(Box::new(path))
    }
}

// ═══════════════════════════════════════════════════════════
// 约束条件
// ═══════════════════════════════════════════════════════════

/// 约束条件 — 对节点或属性值的限制。
///
/// 所有约束的可组合集合，对应 SHACL 核心约束组件。
#[derive(Debug, Clone)]
pub enum Constraint {
    // ── 基数约束 ──
    /// 最小出现次数（`sh:minCount`）
    MinCount(usize),
    /// 最大出现次数（`sh:maxCount`）
    MaxCount(usize),

    // ── 值类型约束 ──
    /// 值必须是指定数据类型（对应 sh:datatype，映射为属性图类型：String, Int, Float, Bool, List）
    Datatype(NodeKind),
    /// 值必须属于指定标签的节点（`sh:class`）
    Class(String),
    /// 节点类型约束（`sh:nodeKind`）
    NodeKind(NodeKind),

    // ── 值范围约束 ──
    /// 最小值（含，`sh:minInclusive`）
    MinInclusive(f64),
    /// 最大值（含，`sh:maxInclusive`）
    MaxInclusive(f64),
    /// 最小值（不含，`sh:minExclusive`）
    MinExclusive(f64),
    /// 最大值（不含，`sh:maxExclusive`）
    MaxExclusive(f64),

    // ── 字符串约束 ──
    /// 最小长度（`sh:minLength`）
    MinLength(usize),
    /// 最大长度（`sh:maxLength`）
    MaxLength(usize),
    /// 正则表达式模式匹配（`sh:pattern`）
    Pattern(String),
    /// 语言标签约束（`sh:languageIn`）
    LanguageIn(Vec<String>),

    // ── 枚举与值约束 ──
    /// 值必须在枚举列表中（`sh:in`）
    In(Vec<PropertyValue>),
    /// 必须包含指定值（`sh:hasValue`）
    HasValue(PropertyValue),

    // ── 逻辑组合约束 ──
    /// 逻辑非 — 子约束必须不满足
    Not(Box<Constraint>),
    /// 逻辑与 — 所有子约束必须满足
    And(Vec<Constraint>),
    /// 逻辑或 — 至少一个子约束必须满足
    Or(Vec<Constraint>),
    /// 异或 — 恰好一个子约束必须满足
    Xone(Vec<Constraint>),

    // ── 形状引用 ──
    /// 嵌套形状验证（`sh:node`）
    QualifiedValueShape {
        /// 引用的子形状
        shape: Box<Shape>,
        /// 必须满足此形状的值的最小数量（qualifiedMinCount）
        qualified_min_count: Option<usize>,
        /// 必须满足此形状的值的最大数量（qualifiedMaxCount）
        qualified_max_count: Option<usize>,
    },

    // ── 存在性 / 唯一性 ──
    /// 属性值必须唯一（跨所有目标节点）
    UniqueValue,
    /// 属性必须存在（等价于 minCount >= 1）
    Required,
    /// 属性必须存在且非空
    NonEmpty,

    // ── 自定义约束 ──
    /// 自定义约束 — SPARQL-style 或用户自定义的闭包验证
    Custom {
        /// 约束名称
        name: String,
        /// 约束参数
        params: HashMap<String, String>,
    },
}

impl Constraint {
    /// 最小基数
    pub fn min_count(n: usize) -> Self {
        Constraint::MinCount(n)
    }

    /// 最大基数
    pub fn max_count(n: usize) -> Self {
        Constraint::MaxCount(n)
    }

    /// 最小值（含）
    pub fn min_inclusive(v: f64) -> Self {
        Constraint::MinInclusive(v)
    }

    /// 最大值（含）
    pub fn max_inclusive(v: f64) -> Self {
        Constraint::MaxInclusive(v)
    }

    /// 最小值（不含）
    pub fn min_exclusive(v: f64) -> Self {
        Constraint::MinExclusive(v)
    }

    /// 最大值（不含）
    pub fn max_exclusive(v: f64) -> Self {
        Constraint::MaxExclusive(v)
    }

    /// 最小长度
    pub fn min_length(n: usize) -> Self {
        Constraint::MinLength(n)
    }

    /// 最大长度
    pub fn max_length(n: usize) -> Self {
        Constraint::MaxLength(n)
    }

    /// 正则模式
    pub fn pattern(regex: impl Into<String>) -> Self {
        Constraint::Pattern(regex.into())
    }

    /// 枚举值
    pub fn in_values(values: Vec<PropertyValue>) -> Self {
        Constraint::In(values)
    }

    /// 必须包含指定值
    pub fn has_value(value: PropertyValue) -> Self {
        Constraint::HasValue(value)
    }

    /// 数据类型约束
    pub fn datatype(kind: NodeKind) -> Self {
        Constraint::Datatype(kind)
    }

    /// 必须存在（非空）
    pub fn required() -> Self {
        Constraint::Required
    }
}

// ═══════════════════════════════════════════════════════════
// 节点类型
// ═══════════════════════════════════════════════════════════

/// 节点类型 — 对应 `sh:nodeKind` 的数据类型约束。
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// 字面量字符串
    Literal,
    /// IRI 引用（属性图中为节点 ID 引用）
    Iri,
    /// 空白节点
    BlankNode,
    /// 字符串类型
    String,
    /// 整数类型
    Int,
    /// 浮点数类型
    Float,
    /// 布尔类型
    Bool,
    /// 列表 / 数组类型
    List,
    /// 任意 JSON 对象
    Object,
}

impl std::str::FromStr for NodeKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "literal" => Ok(NodeKind::Literal),
            "iri" => Ok(NodeKind::Iri),
            "blanknode" | "blank_node" | "bnode" => Ok(NodeKind::BlankNode),
            "string" | "str" => Ok(NodeKind::String),
            "int" | "integer" => Ok(NodeKind::Int),
            "float" | "double" | "number" => Ok(NodeKind::Float),
            "bool" | "boolean" => Ok(NodeKind::Bool),
            "list" | "array" => Ok(NodeKind::List),
            "object" | "json" => Ok(NodeKind::Object),
            _ => Err(format!("未知节点类型: {}", s)),
        }
    }
}

impl std::fmt::Display for NodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeKind::Literal => write!(f, "Literal"),
            NodeKind::Iri => write!(f, "IRI"),
            NodeKind::BlankNode => write!(f, "BlankNode"),
            NodeKind::String => write!(f, "String"),
            NodeKind::Int => write!(f, "Int"),
            NodeKind::Float => write!(f, "Float"),
            NodeKind::Bool => write!(f, "Bool"),
            NodeKind::List => write!(f, "List"),
            NodeKind::Object => write!(f, "Object"),
        }
    }
}

// ═══════════════════════════════════════════════════════════
// 严重级别
// ═══════════════════════════════════════════════════════════

/// 验证结果的严重级别。
///
/// 对应 `sh:Violation` / `sh:Warning` / `sh:Info`。
#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    /// 违规 — 必须修复
    Violation,
    /// 警告 — 建议修复
    Warning,
    /// 信息 — 仅供参考
    Info,
}

impl std::str::FromStr for Severity {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "violation" | "error" => Ok(Severity::Violation),
            "warning" | "warn" => Ok(Severity::Warning),
            "info" | "information" => Ok(Severity::Info),
            _ => Err(format!("未知严重级别: {}", s)),
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Violation => write!(f, "Violation"),
            Severity::Warning => write!(f, "Warning"),
            Severity::Info => write!(f, "Info"),
        }
    }
}

// ═══════════════════════════════════════════════════════════
// 形状图（ShapesGraph）
// ═══════════════════════════════════════════════════════════

/// 形状图 — 一组 SHACL 形状的集合。
///
/// 对应 SHACL 规范中的 `sh:ShapesGraph`。
/// 按名称索引，支持形状间的引用。
#[derive(Debug, Clone, Default)]
pub struct ShapesGraph {
    /// 形状集合（按名称索引）
    pub shapes: HashMap<String, Shape>,
    /// 形状图描述
    pub description: Option<String>,
    /// 全局扩展元数据
    pub metadata: HashMap<String, String>,
}

impl ShapesGraph {
    /// 创建空的形状图
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加形状
    pub fn add_shape(&mut self, shape: Shape) -> &mut Self {
        self.shapes.insert(shape.name().to_string(), shape);
        self
    }

    /// 获取形状
    pub fn get_shape(&self, name: &str) -> Option<&Shape> {
        self.shapes.get(name)
    }

    /// 移除形状
    pub fn remove_shape(&mut self, name: &str) -> Option<Shape> {
        self.shapes.remove(name)
    }

    /// 形状总数
    pub fn len(&self) -> usize {
        self.shapes.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    /// 返回所有激活的形状（未被禁用的）
    pub fn active_shapes(&self) -> Vec<&Shape> {
        self.shapes
            .values()
            .filter(|s| match s {
                Shape::NodeShape(ns) => !ns.deactivated,
                Shape::PropertyShape(ps) => !ps.deactivated,
            })
            .collect()
    }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_build_node_shape() {
        let shape = Shape::node("TestShape")
            .with_target(Target::target_class("Entity"))
            .with_constraint(Constraint::required())
            .with_property(
                PropertyShape::new("code")
                    .with_path(PropertyPath::predicate("code"))
                    .with_constraint(Constraint::min_length(1))
                    .with_constraint(Constraint::max_length(64)),
            );

        match &shape {
            Shape::NodeShape(ns) => {
                assert_eq!(ns.name, "TestShape");
                assert_eq!(ns.targets.len(), 1);
                assert_eq!(ns.property_shapes.len(), 1);
                assert_eq!(ns.property_shapes[0].constraints.len(), 2);
            }
            _ => panic!("Expected NodeShape"),
        }
    }

    #[test]
    fn test_shapes_graph() {
        let mut graph = ShapesGraph::new();
        graph.add_shape(
            Shape::node("Person")
                .with_target(Target::target_class("Person"))
                .with_constraint(Constraint::required()),
        );
        assert_eq!(graph.len(), 1);
        assert!(graph.get_shape("Person").is_some());
        assert!(graph.get_shape("Missing").is_none());
    }

    #[test]
    fn test_node_kind_from_str() {
        assert_eq!(NodeKind::from_str("String").ok(), Some(NodeKind::String));
        assert_eq!(NodeKind::from_str("INT").ok(), Some(NodeKind::Int));
        assert_eq!(NodeKind::from_str("bool").ok(), Some(NodeKind::Bool));
        assert!(NodeKind::from_str("unknown").is_err());
    }

    #[test]
    fn test_severity_from_str() {
        assert_eq!(
            Severity::from_str("violation").ok(),
            Some(Severity::Violation)
        );
        assert_eq!(Severity::from_str("WARNING").ok(), Some(Severity::Warning));
        assert_eq!(Severity::from_str("info").ok(), Some(Severity::Info));
    }

    #[test]
    fn test_property_path_sequence() {
        let path = PropertyPath::sequence(vec!["located_in", "has_coordinate"]);
        match path {
            PropertyPath::Sequence(steps) => {
                assert_eq!(steps.len(), 2);
                assert_eq!(steps[0], "located_in");
                assert_eq!(steps[1], "has_coordinate");
            }
            _ => panic!("Expected Sequence"),
        }
    }
}
