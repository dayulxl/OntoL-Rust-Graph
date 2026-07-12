//! 统一图-描述逻辑映射层 — Single Source of Truth。
//!
//! 基于 W3C OWL2-to-RDF Mapping 标准将 RDF 三元组映射到属性图编码。
//!
//! 标准: <https://www.w3.org/TR/owl2-mapping-to-rdf/>
//!
//! # 三层架构
//!
//! **Layer 0** — W3C 命名空间 URI 常量
//! **Layer 1** — 图词汇表常量 (const &str): 节点标签、关系类型、属性键
//! **Layer 2** — 双向查找表 (LazyLock<HashMap>): 标签 ↔ 实体类型, 关系 ↔ 公理
//! **Layer 3** — 便捷函数 + 预组合切片
//!
//! # 推理边约定
//!
//! 本模块定义的核心 OWL 关系常量（`subClassOf`、`INSTANCE_OF`、`HAS_PROPERTY` 等）是
//! **隐式推理边**——它们是 OWL2 语义基础，由 `OwlAxiomPredicate` 和 `owl_axiom_for_relation()`
//! 直接识别，无需也不使用语言前缀。
//!
//! **领域扩展关系**（非 OWL 标准的关系类型）若要被推理引擎处理，必须使用 6 种语言前缀命名：
//!
//! ```text
//! swrl:hasEnemy   → 推理边，SWRL 引擎处理
//! owl2:衍生自      → 推理边，DWL2 引擎处理
//! sh:validate      → 推理边，SHACL 引擎处理
//! rule:forwardChain → 推理边，推理方向控制
//! action:check     → 推理边，LLM 模糊推理
//! func:calc       → 推理边，LLM JSON 调用
//! 移动             → 非推理边，仅展示/结构遍历
//! ```
//!
//! 详见 CLAUDE.md §13「推理边/推理属性前缀规范」。
//!
//! # 使用
//!
//! ```rust,ignore
//! use ontology_storage::mapper::unified_mapping;
//!
//! // 直接使用常量（性能关键路径）
//! let nodes = repo.get_nodes_by_label(unified_mapping::CLASS_LABEL)?;
//! let rel = Relationship::simple(src, unified_mapping::INSTANCE_OF_REL, tgt);
//!
//! // 内省查询（SHACL / LLM 工具生成）
//! let entity = unified_mapping::owl_entity_for_label("Class"); // Some(OwlEntityType::Class)
//! let axiom = unified_mapping::owl_axiom_for_relation("INSTANCE_OF"); // Some(ClassAssertion)
//! ```

use std::collections::HashMap;
use std::sync::LazyLock;

// ═══════════════════════════════════════════════════════════════
// Layer 0 — W3C 命名空间
// ═══════════════════════════════════════════════════════════════

/// OWL2 命名空间: `http://www.w3.org/2002/07/owl#`
pub const OWL_NS: &str = "http://www.w3.org/2002/07/owl#";

/// RDF 命名空间: `http://www.w3.org/1999/02/22-rdf-syntax-ns#`
pub const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

/// RDFS 命名空间: `http://www.w3.org/2000/01/rdf-schema#`
pub const RDFS_NS: &str = "http://www.w3.org/2000/01/rdf-schema#";

/// XML Schema 命名空间: `http://www.w3.org/2001/XMLSchema#`
pub const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema#";

// ═══════════════════════════════════════════════════════════════
// Layer 1 — 图词汇表常量 (SSOT)
// ═══════════════════════════════════════════════════════════════

// ── 节点标签 (编码实体声明, OWL2 §2.1) ──

/// `owl:Class` — 本体类节点
pub const CLASS_LABEL: &str = "Class";

/// `owl:NamedIndividual` — 命名个体节点
pub const INDIVIDUAL_LABEL: &str = "Individual";

/// `owl:ObjectProperty / owl:DatatypeProperty` — 属性节点
pub const PROPERTY_LABEL: &str = "Property";

/// `owl:Restriction` — 匿名限制节点
pub const RESTRICTION_LABEL: &str = "Restriction";

/// `owl:Axiom` — 公理具象化节点
pub const AXIOM_LABEL: &str = "Axiom";

/// `owl:Ontology` — 本体头节点
pub const ONTOLOGY_LABEL: &str = "Ontology";

// ── 领域扩展标签 (业务域子类) ──

/// 领域实体（`owl:NamedIndividual` 的子类）
pub const ENTITY_LABEL: &str = "Entity";

/// 领域分类节点（`owl:Class` 的子类）
pub const TYPE_LABEL: &str = "Type";

/// 事件节点
pub const EVENT_LABEL: &str = "Event";

/// 巡逻节点
pub const PATROL_LABEL: &str = "Patrol";

/// 打击节点
pub const STRIKE_LABEL: &str = "Strike";

/// 行为节点
pub const BEHAVIOR_LABEL: &str = "Behavior";

/// SWRL 规则节点
pub const RULE_LABEL: &str = "Rule";

// ── 关系类型 (编码公理谓词, OWL2 §2.2) ──

/// `rdf:type` / ClassAssertion — 个体属于某类
pub const INSTANCE_OF_REL: &str = "INSTANCE_OF";

/// `rdfs:subClassOf` — 子类关系
pub const SUB_CLASS_OF_REL: &str = "subClassOf";

/// `rdfs:subPropertyOf` — 子属性关系
pub const SUB_PROPERTY_OF_REL: &str = "subPropertyOf";

/// ObjectPropertyDomain — 域类拥有某属性
pub const HAS_PROPERTY_REL: &str = "HAS_PROPERTY";

/// ObjectPropertyRange — 属性的值域
pub const HAS_RANGE_REL: &str = "HAS_RANGE";

/// DataPropertyAssertion — 个体具有某数据属性值
pub const HAS_VALUE_REL: &str = "HAS_VALUE";

/// `owl:equivalentClass` — 等价类
pub const EQUIVALENT_CLASS_REL: &str = "equivalentClass";

/// `owl:disjointWith` — 不相交类
pub const DISJOINT_WITH_REL: &str = "disjointWith";

/// `owl:sameAs` — 等同个体
pub const SAME_AS_REL: &str = "sameAs";

/// `owl:differentFrom` — 不同个体
pub const DIFFERENT_FROM_REL: &str = "differentFrom";

/// `owl:inverseOf` — 逆属性
pub const INVERSE_OF_REL: &str = "inverseOf";

/// `owl:complementOf` — 补类
pub const COMPLEMENT_OF_REL: &str = "complementOf";

/// `owl:intersectionOf` — 交集
pub const INTERSECTION_OF_REL: &str = "intersectionOf";

/// `owl:unionOf` — 并集
pub const UNION_OF_REL: &str = "unionOf";

/// `owl:oneOf` — 枚举
pub const ONE_OF_REL: &str = "oneOf";

/// 本体语义层关系常量合集 — 所有 W3C OWL2/RDFS 标准关系。
///
/// 这些关系编码了本体的语义结构（类层次、实例归属、属性约束等）。
/// 推理机在处理时**优先于** SWRL/SHACL/rule/action/function 层。
/// 也用于 `is_ontology_relation()` 判断。
pub const ONTOLOGY_SEMANTIC_RELS: &[&str] = &[
    INSTANCE_OF_REL,     // rdf:type — 类归属
    SUB_CLASS_OF_REL,    // rdfs:subClassOf — 类层次
    SUB_PROPERTY_OF_REL, // rdfs:subPropertyOf — 属性层次
    HAS_PROPERTY_REL,
    HAS_RANGE_REL,
    HAS_VALUE_REL,
    EQUIVALENT_CLASS_REL,
    DISJOINT_WITH_REL,
    SAME_AS_REL,
    DIFFERENT_FROM_REL,
    INVERSE_OF_REL,
    COMPLEMENT_OF_REL,
    INTERSECTION_OF_REL,
    UNION_OF_REL,
    ONE_OF_REL,
];

// ── 属性键 (编码注释属性 + OWL 方面, OWL2 §2.3-2.4) ──

/// 标识 IRI（节点身份）
pub const IRI_KEY: &str = "iri";

/// `rdfs:label` — 人类可读标签
pub const LABEL_KEY: &str = "label";

/// `rdfs:comment` — 注释说明
pub const COMMENT_KEY: &str = "comment";

/// `rdfs:domain` — 定义域
pub const DOMAIN_KEY: &str = "domain";

/// `rdfs:range` — 值域
pub const RANGE_KEY: &str = "range";

/// 通用类型描述
pub const TYPE_KEY: &str = "type";

/// 通用值
pub const VALUE_KEY: &str = "value";

/// `owl:TransitiveProperty` — 传递性
pub const TRANSITIVE_KEY: &str = "transitive";

/// `owl:SymmetricProperty` — 对称性
pub const SYMMETRIC_KEY: &str = "symmetric";

/// `owl:FunctionalProperty` — 函数性
pub const FUNCTIONAL_KEY: &str = "functional";

/// `owl:InverseFunctionalProperty` — 逆函数性
pub const INVERSE_FUNCTIONAL_KEY: &str = "inverseFunctional";

/// `owl:ReflexiveProperty` — 自反性
pub const REFLEXIVE_KEY: &str = "reflexive";

/// `owl:IrreflexiveProperty` — 非自反性
pub const IRREFLEXIVE_KEY: &str = "irreflexive";

/// `owl:deprecated` — 已弃用标记
pub const DEPRECATED_KEY: &str = "deprecated";

// ── 行为字段属性键 (Entity 行为引擎, OWL2 风格) ──

/// `hasPrecondition` — 触发前约束（SHACL 执行语言）
pub const HAS_PRECONDITION_KEY: &str = "hasPrecondition";

/// `hasEffect` — 效果（语言触发）
pub const HAS_EFFECT_KEY: &str = "hasEffect";

/// `hasCost` — 消耗描述
pub const HAS_COST_KEY: &str = "hasCost";

/// `hasDuration` — 持续时间（秒，字符串）
pub const HAS_DURATION_KEY: &str = "hasDuration";

/// `hasPriority` — 优先级（0-10，字符串）
pub const HAS_PRIORITY_KEY: &str = "hasPriority";

/// `composedOf` — 组合动作（分号分隔的 entity code 列表）
pub const COMPOSED_OF_KEY: &str = "composedOf";

/// 所有行为字段属性键（遍历用）
pub const BEHAVIOR_FIELD_KEYS: &[&str] = &[
    HAS_PRECONDITION_KEY,
    HAS_EFFECT_KEY,
    HAS_COST_KEY,
    HAS_DURATION_KEY,
    HAS_PRIORITY_KEY,
    COMPOSED_OF_KEY,
];

// ── 边属性键 (自定义动作接口, 9 个标准字段) ──

/// `actionType` — 路由标识, 指定执行分支 (如 "inference" 表示走推理机逻辑)
pub const ACTION_TYPE_KEY: &str = "actionType";

/// `required` — 阻断控制, 校验失败时是否强制中断当前业务流程
pub const REQUIRED_KEY: &str = "required";

/// `validationType` — 规则级别: "Strong"(强校验/阻断) / "Weak"(弱校验/提醒不阻断)
pub const VALIDATION_TYPE_KEY: &str = "validationType";

/// `ruleId` — 规则锚点, 指向图数据库中的规则本体节点
pub const RULE_ID_KEY: &str = "ruleId";

/// `func` — 执行指令, 映射底层要调用的具体函数名
pub const FUNC_KEY: &str = "func";

/// `id` — 数据锚点, 当前需要被校验的具体业务数据节点
pub const FIELD_ID_KEY: &str = "id";

/// `msg` — 详细说明作用
pub const MSG_KEY: &str = "msg";

/// `synonym` — 同义词
pub const SYNONYM_KEY: &str = "synonym";

/// `queryVariant` — 错意词
pub const QUERY_VARIANT_KEY: &str = "queryVariant";

/// 所有边属性键（遍历用）
pub const EDGE_ACTION_KEYS: &[&str] = &[
    ACTION_TYPE_KEY,
    REQUIRED_KEY,
    VALIDATION_TYPE_KEY,
    RULE_ID_KEY,
    FUNC_KEY,
    FIELD_ID_KEY,
    MSG_KEY,
    SYNONYM_KEY,
    QUERY_VARIANT_KEY,
];

// ── 预组合切片 ──

/// 所有标准 OWL2 节点标签
pub const OWL_NODE_LABELS: &[&str] = &[
    CLASS_LABEL,
    INDIVIDUAL_LABEL,
    PROPERTY_LABEL,
    RESTRICTION_LABEL,
    AXIOM_LABEL,
    ONTOLOGY_LABEL,
];

/// 所有领域节点标签 (顺序稳定，遍历用)
pub const DOMAIN_LABELS: &[&str] = &[
    ENTITY_LABEL,
    EVENT_LABEL,
    PATROL_LABEL,
    STRIKE_LABEL,
    TYPE_LABEL,
    BEHAVIOR_LABEL,
];

/// 所有标准 + 领域标签
pub const ALL_KNOWN_LABELS: &[&str] = &[
    CLASS_LABEL,
    INDIVIDUAL_LABEL,
    PROPERTY_LABEL,
    RESTRICTION_LABEL,
    AXIOM_LABEL,
    ONTOLOGY_LABEL,
    ENTITY_LABEL,
    TYPE_LABEL,
    EVENT_LABEL,
    PATROL_LABEL,
    STRIKE_LABEL,
    BEHAVIOR_LABEL,
    RULE_LABEL,
];

// ═══════════════════════════════════════════════════════════════
// Layer 2 — 枚举 + 双向查找表
// ═══════════════════════════════════════════════════════════════

/// OWL2 实体类型 — 对应 W3C 规范中的声明类型。
///
/// 用于从图节点标签反向推断 OWL 语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OwlEntityType {
    /// `owl:Class`
    Class,
    /// `owl:ObjectProperty`
    ObjectProperty,
    /// `owl:DatatypeProperty`
    DatatypeProperty,
    /// `owl:AnnotationProperty`
    AnnotationProperty,
    /// `owl:NamedIndividual`
    NamedIndividual,
    /// `owl:Restriction` (匿名)
    Restriction,
    /// `owl:Ontology`
    Ontology,
    /// 公理具象化节点
    Axiom,
}

/// OWL2 公理谓词 — 对应 W3C 规范 §2.2 中的公理类型。
///
/// 用于从图关系类型反向推断 OWL 语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OwlAxiomPredicate {
    /// `ClassAssertion` — `rdf:type` → `INSTANCE_OF`
    ClassAssertion,
    /// `SubClassOf` — `rdfs:subClassOf` → `subClassOf`
    SubClassOf,
    /// `SubObjectPropertyOf` — `rdfs:subPropertyOf` → `subPropertyOf`
    SubPropertyOf,
    /// `ObjectPropertyAssertion` — 直接属性边 `I1 P I2`
    ObjectPropertyAssertion,
    /// `DataPropertyAssertion` — `HAS_VALUE`
    DataPropertyAssertion,
    /// `ObjectPropertyDomain` — `HAS_PROPERTY`
    ObjectPropertyDomain,
    /// `ObjectPropertyRange` — `HAS_RANGE`
    ObjectPropertyRange,
    /// `EquivalentClasses` — `equivalentClass`
    EquivalentClasses,
    /// `DisjointClasses` — `disjointWith`
    DisjointClasses,
    /// `SameIndividual` — `sameAs`
    SameIndividual,
    /// `DifferentIndividuals` — `differentFrom`
    DifferentIndividuals,
    /// `InverseObjectProperties` — `inverseOf`
    InverseObjectProperties,
    /// 其他/未知公理类型
    Other,
}

/// 属性键的分类 — 用于 SHACL / LLM 区分标准属性和领域属性。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyKeyCategory {
    /// `rdfs:label`, `rdfs:comment` — 标准注释
    Annotation,
    /// `iri`, `type`, `value` — 核心标识
    Core,
    /// `transitive`, `symmetric`, `functional` 等 — OWL 属性特征
    OwlAspect,
    /// 业务域定义的属性
    DomainSpecific,
}

// ── 节点标签 ↔ OwlEntityType 双向查找 ──

/// `OwlEntityType` → 标准图节点标签
pub static DL_TO_LABEL: LazyLock<HashMap<OwlEntityType, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(OwlEntityType::Class, CLASS_LABEL);
    m.insert(OwlEntityType::ObjectProperty, PROPERTY_LABEL);
    m.insert(OwlEntityType::DatatypeProperty, PROPERTY_LABEL);
    m.insert(OwlEntityType::AnnotationProperty, PROPERTY_LABEL);
    m.insert(OwlEntityType::NamedIndividual, INDIVIDUAL_LABEL);
    m.insert(OwlEntityType::Restriction, RESTRICTION_LABEL);
    m.insert(OwlEntityType::Ontology, ONTOLOGY_LABEL);
    m.insert(OwlEntityType::Axiom, AXIOM_LABEL);
    m
});

/// 图节点标签 → `OwlEntityType` 反向查找
pub static LABEL_TO_DL: LazyLock<HashMap<&'static str, OwlEntityType>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(CLASS_LABEL, OwlEntityType::Class);
    m.insert(INDIVIDUAL_LABEL, OwlEntityType::NamedIndividual);
    m.insert(PROPERTY_LABEL, OwlEntityType::ObjectProperty);
    m.insert(RESTRICTION_LABEL, OwlEntityType::Restriction);
    m.insert(ONTOLOGY_LABEL, OwlEntityType::Ontology);
    m.insert(AXIOM_LABEL, OwlEntityType::Axiom);
    m
});

// ── 关系类型 ↔ OwlAxiomPredicate 双向查找 ──

/// `OwlAxiomPredicate` → 图关系类型
pub static DL_TO_REL: LazyLock<HashMap<OwlAxiomPredicate, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(OwlAxiomPredicate::ClassAssertion, INSTANCE_OF_REL);
    m.insert(OwlAxiomPredicate::SubClassOf, SUB_CLASS_OF_REL);
    m.insert(OwlAxiomPredicate::SubPropertyOf, SUB_PROPERTY_OF_REL);
    m.insert(OwlAxiomPredicate::ObjectPropertyDomain, HAS_PROPERTY_REL);
    m.insert(OwlAxiomPredicate::ObjectPropertyRange, HAS_RANGE_REL);
    m.insert(OwlAxiomPredicate::DataPropertyAssertion, HAS_VALUE_REL);
    m.insert(OwlAxiomPredicate::EquivalentClasses, EQUIVALENT_CLASS_REL);
    m.insert(OwlAxiomPredicate::DisjointClasses, DISJOINT_WITH_REL);
    m.insert(OwlAxiomPredicate::SameIndividual, SAME_AS_REL);
    m.insert(OwlAxiomPredicate::DifferentIndividuals, DIFFERENT_FROM_REL);
    m.insert(OwlAxiomPredicate::InverseObjectProperties, INVERSE_OF_REL);
    m
});

/// 图关系类型 → `OwlAxiomPredicate` 反向查找
pub static REL_TO_DL: LazyLock<HashMap<&'static str, OwlAxiomPredicate>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(INSTANCE_OF_REL, OwlAxiomPredicate::ClassAssertion);
    m.insert(SUB_CLASS_OF_REL, OwlAxiomPredicate::SubClassOf);
    m.insert(SUB_PROPERTY_OF_REL, OwlAxiomPredicate::SubPropertyOf);
    m.insert(HAS_PROPERTY_REL, OwlAxiomPredicate::ObjectPropertyDomain);
    m.insert(HAS_RANGE_REL, OwlAxiomPredicate::ObjectPropertyRange);
    m.insert(HAS_VALUE_REL, OwlAxiomPredicate::DataPropertyAssertion);
    m.insert(EQUIVALENT_CLASS_REL, OwlAxiomPredicate::EquivalentClasses);
    m.insert(DISJOINT_WITH_REL, OwlAxiomPredicate::DisjointClasses);
    m.insert(SAME_AS_REL, OwlAxiomPredicate::SameIndividual);
    m.insert(DIFFERENT_FROM_REL, OwlAxiomPredicate::DifferentIndividuals);
    m.insert(INVERSE_OF_REL, OwlAxiomPredicate::InverseObjectProperties);
    m
});

// ── 属性键 → PropertyKeyCategory ──

/// 标准属性键的分类映射
pub static PROP_KEY_CATEGORIES: LazyLock<HashMap<&'static str, PropertyKeyCategory>> =
    LazyLock::new(|| {
        let mut m = HashMap::new();
        m.insert(IRI_KEY, PropertyKeyCategory::Core);
        m.insert(TYPE_KEY, PropertyKeyCategory::Core);
        m.insert(VALUE_KEY, PropertyKeyCategory::Core);
        m.insert(LABEL_KEY, PropertyKeyCategory::Annotation);
        m.insert(COMMENT_KEY, PropertyKeyCategory::Annotation);
        m.insert(DOMAIN_KEY, PropertyKeyCategory::Core);
        m.insert(RANGE_KEY, PropertyKeyCategory::Core);
        m.insert(TRANSITIVE_KEY, PropertyKeyCategory::OwlAspect);
        m.insert(SYMMETRIC_KEY, PropertyKeyCategory::OwlAspect);
        m.insert(FUNCTIONAL_KEY, PropertyKeyCategory::OwlAspect);
        m.insert(INVERSE_FUNCTIONAL_KEY, PropertyKeyCategory::OwlAspect);
        m.insert(REFLEXIVE_KEY, PropertyKeyCategory::OwlAspect);
        m.insert(IRREFLEXIVE_KEY, PropertyKeyCategory::OwlAspect);
        m.insert(DEPRECATED_KEY, PropertyKeyCategory::OwlAspect);
        m
    });

// ═══════════════════════════════════════════════════════════════
// Layer 3 — 便捷查询函数
// ═══════════════════════════════════════════════════════════════

/// 通过图节点标签查询对应的 OWL 实体类型。
///
/// 若标签不在标准 OWL 词汇表中（如为领域标签），返回 `None`。
pub fn owl_entity_for_label(label: &str) -> Option<OwlEntityType> {
    LABEL_TO_DL.get(label).copied()
}

/// 获取 OWL 实体类型对应的图节点标签。
pub fn label_for_owl(entity: OwlEntityType) -> &'static str {
    DL_TO_LABEL.get(&entity).copied().unwrap_or(CLASS_LABEL)
}

/// 通过图关系类型查询对应的 OWL 公理谓词。
///
/// 若关系类型不在标准词汇表中（如为领域自定义关系），返回 `None`。
pub fn owl_axiom_for_relation(rel_type: &str) -> Option<OwlAxiomPredicate> {
    REL_TO_DL.get(rel_type).copied()
}

/// 获取 OWL 公理谓词对应的图关系类型。
pub fn rel_for_axiom(axiom: OwlAxiomPredicate) -> Option<&'static str> {
    DL_TO_REL.get(&axiom).copied()
}

/// 获取属性键的分类。
///
/// 对于不在标准集合中的键返回 `None`（调用方自行归类为 `DomainSpecific`）。
pub fn categorize_property_key(key: &str) -> Option<PropertyKeyCategory> {
    PROP_KEY_CATEGORIES.get(key).copied()
}

/// 该属性键是否为标准 OWL 注释属性 (`rdfs:label`, `rdfs:comment`)。
pub fn is_annotation_property(key: &str) -> bool {
    matches!(
        categorize_property_key(key),
        Some(PropertyKeyCategory::Annotation)
    )
}

/// 该属性键是否为 OWL 属性特征 (`transitive`, `symmetric`, 等)。
pub fn is_owl_aspect(key: &str) -> bool {
    matches!(
        categorize_property_key(key),
        Some(PropertyKeyCategory::OwlAspect)
    )
}

/// 该节点标签是否为已知的 OWL 或领域标签。
pub fn is_known_label(label: &str) -> bool {
    LABEL_TO_DL.contains_key(label) || DOMAIN_LABELS.contains(&label) || label == RULE_LABEL
}

/// 该标签是否对应某个 `owl:NamedIndividual` 变体。
pub fn is_individual_label(label: &str) -> bool {
    matches!(
        owl_entity_for_label(label),
        Some(OwlEntityType::NamedIndividual)
    ) || DOMAIN_LABELS.contains(&label)
        || label == RULE_LABEL
}

/// 该标签是否对应 `owl:Class` 或其领域子类。
pub fn is_class_label(label: &str) -> bool {
    label == CLASS_LABEL || label == TYPE_LABEL
}

/// 该关系类型是否为已知的 OWL 公理关系。
pub fn is_known_relation(rel_type: &str) -> bool {
    REL_TO_DL.contains_key(rel_type)
}

/// 获取某 OWL 实体类型的 RDF type IRI。
pub fn rdf_type_iri_for_entity(entity: OwlEntityType) -> &'static str {
    match entity {
        OwlEntityType::Class => "owl:Class",
        OwlEntityType::ObjectProperty => "owl:ObjectProperty",
        OwlEntityType::DatatypeProperty => "owl:DatatypeProperty",
        OwlEntityType::AnnotationProperty => "owl:AnnotationProperty",
        OwlEntityType::NamedIndividual => "owl:NamedIndividual",
        OwlEntityType::Restriction => "owl:Restriction",
        OwlEntityType::Ontology => "owl:Ontology",
        OwlEntityType::Axiom => "owl:Axiom",
    }
}

/// 获取公理谓词的 RDF 谓词 IRI。
pub fn rdf_predicate_iri_for_axiom(axiom: OwlAxiomPredicate) -> &'static str {
    match axiom {
        OwlAxiomPredicate::ClassAssertion => "rdf:type",
        OwlAxiomPredicate::SubClassOf => "rdfs:subClassOf",
        OwlAxiomPredicate::SubPropertyOf => "rdfs:subPropertyOf",
        OwlAxiomPredicate::ObjectPropertyAssertion => "(direct)",
        OwlAxiomPredicate::DataPropertyAssertion => "(data edge)",
        OwlAxiomPredicate::ObjectPropertyDomain => "rdfs:domain",
        OwlAxiomPredicate::ObjectPropertyRange => "rdfs:range",
        OwlAxiomPredicate::EquivalentClasses => "owl:equivalentClass",
        OwlAxiomPredicate::DisjointClasses => "owl:disjointWith",
        OwlAxiomPredicate::SameIndividual => "owl:sameAs",
        OwlAxiomPredicate::DifferentIndividuals => "owl:differentFrom",
        OwlAxiomPredicate::InverseObjectProperties => "owl:inverseOf",
        OwlAxiomPredicate::Other => "(unknown)",
    }
}

// ═══════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── 常量值验证 ──

    #[test]
    fn test_label_constants_match_expected() {
        assert_eq!(CLASS_LABEL, "Class");
        assert_eq!(INDIVIDUAL_LABEL, "Individual");
        assert_eq!(ENTITY_LABEL, "Entity");
        assert_eq!(TYPE_LABEL, "Type");
        assert_eq!(EVENT_LABEL, "Event");
        assert_eq!(PATROL_LABEL, "Patrol");
        assert_eq!(STRIKE_LABEL, "Strike");
        assert_eq!(BEHAVIOR_LABEL, "Behavior");
        assert_eq!(RULE_LABEL, "Rule");
    }

    #[test]
    fn test_rel_constants_match_expected() {
        assert_eq!(INSTANCE_OF_REL, "INSTANCE_OF");
        assert_eq!(SUB_CLASS_OF_REL, "subClassOf");
        assert_eq!(HAS_PROPERTY_REL, "HAS_PROPERTY");
        assert_eq!(HAS_RANGE_REL, "HAS_RANGE");
        assert_eq!(HAS_VALUE_REL, "HAS_VALUE");
    }

    #[test]
    fn test_prop_constants_match_expected() {
        assert_eq!(IRI_KEY, "iri");
        assert_eq!(LABEL_KEY, "label");
        assert_eq!(COMMENT_KEY, "comment");
        assert_eq!(DOMAIN_KEY, "domain");
        assert_eq!(RANGE_KEY, "range");
        assert_eq!(TRANSITIVE_KEY, "transitive");
        assert_eq!(SYMMETRIC_KEY, "symmetric");
        assert_eq!(FUNCTIONAL_KEY, "functional");
    }

    // ── 双向查找 ──

    #[test]
    fn test_label_to_dl_roundtrip() {
        assert_eq!(
            owl_entity_for_label(CLASS_LABEL),
            Some(OwlEntityType::Class)
        );
        assert_eq!(label_for_owl(OwlEntityType::Class), CLASS_LABEL);
    }

    #[test]
    fn test_individual_label_lookup() {
        assert_eq!(
            owl_entity_for_label(INDIVIDUAL_LABEL),
            Some(OwlEntityType::NamedIndividual)
        );
        // 领域标签不是标准 OWL 实体
        assert_eq!(owl_entity_for_label(ENTITY_LABEL), None);
    }

    #[test]
    fn test_rel_to_dl_roundtrip() {
        assert_eq!(
            owl_axiom_for_relation(INSTANCE_OF_REL),
            Some(OwlAxiomPredicate::ClassAssertion)
        );
        assert_eq!(
            rel_for_axiom(OwlAxiomPredicate::ClassAssertion),
            Some(INSTANCE_OF_REL)
        );
    }

    #[test]
    fn test_unknown_relation_returns_none() {
        assert_eq!(owl_axiom_for_relation("arbitrary_custom_rel"), None);
    }

    // ── 分类函数 ──

    #[test]
    fn test_is_annotation_property() {
        assert!(is_annotation_property(LABEL_KEY));
        assert!(is_annotation_property(COMMENT_KEY));
        assert!(!is_annotation_property(IRI_KEY));
        assert!(!is_annotation_property("code"));
    }

    #[test]
    fn test_is_owl_aspect() {
        assert!(is_owl_aspect(TRANSITIVE_KEY));
        assert!(is_owl_aspect(SYMMETRIC_KEY));
        assert!(!is_owl_aspect(LABEL_KEY));
    }

    #[test]
    fn test_is_individual_label() {
        assert!(is_individual_label(INDIVIDUAL_LABEL));
        assert!(is_individual_label(ENTITY_LABEL));
        assert!(is_individual_label(EVENT_LABEL));
        assert!(is_individual_label(PATROL_LABEL));
        assert!(!is_individual_label(CLASS_LABEL));
    }

    #[test]
    fn test_is_class_label() {
        assert!(is_class_label(CLASS_LABEL));
        assert!(is_class_label(TYPE_LABEL));
        assert!(!is_class_label(ENTITY_LABEL));
    }

    // ── 切片验证 ──

    #[test]
    fn test_domain_labels_contains_all() {
        assert_eq!(DOMAIN_LABELS.len(), 6);
        assert!(DOMAIN_LABELS.contains(&ENTITY_LABEL));
        assert!(DOMAIN_LABELS.contains(&EVENT_LABEL));
        assert!(DOMAIN_LABELS.contains(&PATROL_LABEL));
        assert!(DOMAIN_LABELS.contains(&STRIKE_LABEL));
        assert!(DOMAIN_LABELS.contains(&TYPE_LABEL));
        assert!(DOMAIN_LABELS.contains(&BEHAVIOR_LABEL));
    }

    #[test]
    fn test_all_known_labels_includes_everything() {
        assert!(ALL_KNOWN_LABELS.contains(&CLASS_LABEL));
        assert!(ALL_KNOWN_LABELS.contains(&ENTITY_LABEL));
        assert!(ALL_KNOWN_LABELS.contains(&RULE_LABEL));
    }

    // ── RDF IRI 函数 ──

    #[test]
    fn test_rdf_type_iri() {
        assert_eq!(rdf_type_iri_for_entity(OwlEntityType::Class), "owl:Class");
        assert_eq!(
            rdf_type_iri_for_entity(OwlEntityType::NamedIndividual),
            "owl:NamedIndividual"
        );
    }

    #[test]
    fn test_rdf_predicate_iri() {
        assert_eq!(
            rdf_predicate_iri_for_axiom(OwlAxiomPredicate::ClassAssertion),
            "rdf:type"
        );
        assert_eq!(
            rdf_predicate_iri_for_axiom(OwlAxiomPredicate::SubClassOf),
            "rdfs:subClassOf"
        );
    }
}
