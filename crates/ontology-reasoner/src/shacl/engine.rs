//! SHACL 验证引擎。
//!
//! 对属性图中的节点执行形状约束验证。
//! 引擎只读图数据，不执行写入操作。
//!
//! # 验证流程
//!
//! ```text
//! 1. 加载 ShapesGraph（形状定义）
//! 2. 对每个形状：
//!    a. 解析 Target → 找到焦点节点集合
//!    b. 对每个焦点节点：
//!       - 评估 NodeShape 级约束
//!       - 评估 PropertyShape 级约束（逐出边）
//! 3. 汇总所有结果 → ValidationReport
//! ```
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use ontology_reasoner::shacl::{ShaclEngine, ShapesGraph, Shape, Target, Constraint};
//!
//! let shapes = {
//!     let mut sg = ShapesGraph::new();
//!     sg.add_shape(
//!         Shape::node("EntityShape")
//!             .with_target(Target::target_class("Entity"))
//!             .with_property(PropertyShape::new("code")
//!                 .with_constraint(Constraint::required()))
//!     );
//!     sg
//! };
//!
//! let engine = ShaclEngine::new(repo, shapes);
//! let report = engine.validate()?;
//! println!("{}", report.summary());
//! ```

use std::rc::Rc;
use std::time::Instant;

use ontology_storage::mapper::graph::node::Node;
use ontology_storage::mapper::graph::property::PropertyValue;
use ontology_storage::mapper::unified_mapping;
use ontology_storage::repository::graph_store::GraphRepository;

use super::ast::{
    Constraint, NodeKind, NodeShape, PropertyPath, PropertyShape, Shape, ShapesGraph, Target,
};
use super::error::ShaclError;
use super::result::{ValidationReport, ValidationResult};

// ═══════════════════════════════════════════════════════════
// ShaclEngine
// ═══════════════════════════════════════════════════════════

/// SHACL 验证引擎。
///
/// 持有只读的图仓库引用和形状定义，执行约束验证。
pub struct ShaclEngine {
    /// 图仓库（只读）
    repo: Rc<dyn GraphRepository>,
    /// 形状图定义
    shapes: ShapesGraph,
    /// 副本版本过滤：如果设置，只验证 cope_version 匹配的节点。
    /// `None` 表示不过滤（向后兼容，验证全局图）。
    cope_version: Option<String>,
}

impl ShaclEngine {
    /// 创建新的 SHACL 验证引擎
    pub fn new(repo: Rc<dyn GraphRepository>, shapes: ShapesGraph) -> Self {
        Self {
            repo,
            shapes,
            cope_version: None,
        }
    }

    /// 设置副本版本过滤 — 只验证 cope_version 匹配的节点。
    pub fn with_cope_version(mut self, ver: &str) -> Self {
        self.cope_version = Some(ver.to_string());
        self
    }

    /// 检查节点是否匹配当前的 cope_version 过滤条件。
    fn node_matches_cope_version(&self, node: &Node) -> bool {
        match &self.cope_version {
            Some(v) => node
                .property("cope_version")
                .and_then(|pv| pv.as_str())
                .map(|cv| cv == v.as_str())
                .unwrap_or(false),
            None => true,
        }
    }

    /// 获取形状图的只读引用
    pub fn shapes(&self) -> &ShapesGraph {
        &self.shapes
    }

    /// 替换形状图（用于热更新约束规则）
    pub fn set_shapes(&mut self, shapes: ShapesGraph) {
        self.shapes = shapes;
    }

    // ═══════════════════════════════════════════════════════
    // 核心验证入口
    // ═══════════════════════════════════════════════════════

    /// 执行完整验证，返回验证报告。
    ///
    /// 遍历所有激活的形状，对每个匹配的焦点节点评估约束。
    pub fn validate(&self) -> Result<ValidationReport, ShaclError> {
        let start = Instant::now();
        let active = self.shapes.active_shapes();

        let mut report = ValidationReport::new();
        report.total_shapes = active.len();

        let mut total_nodes = 0usize;

        for shape in &active {
            match shape {
                Shape::NodeShape(ns) => {
                    let focus_nodes = self.resolve_targets(&ns.targets)?;
                    total_nodes += focus_nodes.len();
                    for node in &focus_nodes {
                        self.validate_node_shape(ns, node, &mut report)?;
                    }
                }
                Shape::PropertyShape(_ps) => {
                    // 独立的 PropertyShape（未嵌套在 NodeShape 中）：
                    // 需要外部指定焦点节点。此处跳过，
                    // 仅当被 NodeShape.property_shapes 引用时才验证。
                    // 如需独立使用，通过 validate_property_shape_on_node() 手动调用。
                }
            }
        }

        report.total_nodes = total_nodes;
        report.elapsed_ms = start.elapsed().as_millis() as u64;
        Ok(report)
    }

    /// 对单个节点执行指定形状的验证。
    ///
    /// 用于 API 层按需验证某个节点。
    pub fn validate_node(
        &self,
        shape_name: &str,
        node_id: &str,
    ) -> Result<ValidationReport, ShaclError> {
        let start = Instant::now();
        let node = self
            .repo
            .get_node(node_id)?
            .ok_or_else(|| ShaclError::NodeNotFound(node_id.to_string()))?;

        // cope_version 过滤：节点不在目标版本中则拒绝
        if !self.node_matches_cope_version(&node) {
            return Err(ShaclError::NodeNotFound(format!(
                "节点 '{}' 不在目标版本 '{}' 中",
                node_id,
                self.cope_version.as_deref().unwrap_or("(未设置)")
            )));
        }

        let shape = self
            .shapes
            .get_shape(shape_name)
            .ok_or_else(|| ShaclError::ShapeNotFound(shape_name.to_string()))?;

        let mut report = ValidationReport::new();
        report.total_nodes = 1;
        report.total_shapes = 1;

        match shape {
            Shape::NodeShape(ns) => {
                self.validate_node_shape(ns, &node, &mut report)?;
            }
            Shape::PropertyShape(ps) => {
                self.validate_property_shape_on_node(ps, &node, ps, &mut report)?;
            }
        }

        report.elapsed_ms = start.elapsed().as_millis() as u64;
        Ok(report)
    }

    // ═══════════════════════════════════════════════════════
    // 目标解析
    // ═══════════════════════════════════════════════════════

    /// 解析 Target 列表，返回匹配的焦点节点集合。
    /// 当设置了 cope_version 时，只返回版本匹配的节点。
    fn resolve_targets(&self, targets: &[Target]) -> Result<Vec<Node>, ShaclError> {
        let mut nodes: Vec<Node> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for target in targets {
            let matched = self.resolve_single_target(target)?;
            for node in matched {
                // cope_version 过滤
                if !self.node_matches_cope_version(&node) {
                    continue;
                }
                let node_id = node_id_str(&node);
                if seen.insert(node_id) {
                    nodes.push(node);
                }
            }
        }

        Ok(nodes)
    }

    /// 解析单个 Target
    fn resolve_single_target(&self, target: &Target) -> Result<Vec<Node>, ShaclError> {
        match target {
            Target::TargetClass { label } => self
                .repo
                .get_nodes_by_label(label)
                .map_err(ShaclError::Storage),
            Target::TargetNode { node_id } => {
                if let Some(node) = self.repo.get_node(node_id)? {
                    Ok(vec![node])
                } else {
                    Ok(Vec::new())
                }
            }
            Target::TargetSubjectsOf { property } => {
                // 扫描所有标签，找到拥有此出边属性的节点
                let labels = unified_mapping::DOMAIN_LABELS;
                let mut result = Vec::new();
                for label in labels {
                    let nodes = self.repo.get_nodes_by_label(label)?;
                    for node in &nodes {
                        if node.properties.contains_key(property)
                            || self
                                .repo
                                .get_relationships(&node_id_str(node), Some(property))
                                .map(|rels| !rels.is_empty())
                                .unwrap_or(false)
                        {
                            result.push(node.clone());
                        }
                    }
                }
                Ok(result)
            }
            Target::TargetObjectsOf { property } => {
                // 扫描所有标签，找到被此属性指向的节点
                let labels = unified_mapping::DOMAIN_LABELS;
                let mut result = Vec::new();
                for label in labels {
                    let nodes = self.repo.get_nodes_by_label(label)?;
                    for node in &nodes {
                        let node_id = node_id_str(node);
                        // 检查是否有到此节点的入边
                        let rels = self.repo.get_relationships(&node_id, None)?;
                        let is_target = rels
                            .iter()
                            .any(|r| r.rel_type == *property && r.end_node_id == node_id);
                        if is_target {
                            result.push(node.clone());
                        }
                    }
                }
                Ok(result)
            }
            Target::AllNodes => {
                // 扫描所有标签的所有节点
                let labels = unified_mapping::DOMAIN_LABELS;
                let mut result = Vec::new();
                for label in labels {
                    let nodes = self.repo.get_nodes_by_label(label)?;
                    result.extend(nodes);
                }
                Ok(result)
            }
        }
    }

    // ═══════════════════════════════════════════════════════
    // 形状验证
    // ═══════════════════════════════════════════════════════

    /// 对焦点节点执行 NodeShape 的所有约束验证。
    fn validate_node_shape(
        &self,
        shape: &NodeShape,
        node: &Node,
        report: &mut ValidationReport,
    ) -> Result<(), ShaclError> {
        if shape.deactivated {
            return Ok(());
        }

        let node_id = node_id_str(node);

        // 1. 评估节点级约束
        for constraint in &shape.constraints {
            if let Some(result) =
                self.evaluate_constraint(node, None, constraint, &shape.name, &node_id)
            {
                report.add_result(result);
            }
        }

        // 2. 评估属性级约束
        for prop_shape in &shape.property_shapes {
            if prop_shape.deactivated {
                continue;
            }
            self.validate_property_shape_on_node(prop_shape, node, prop_shape, report)?;
        }

        Ok(())
    }

    /// 对某个节点评估单个 PropertyShape
    fn validate_property_shape_on_node(
        &self,
        prop_shape: &PropertyShape,
        node: &Node,
        // 源形状引用（用于错误消息中的形状名）
        source_shape: &PropertyShape,
        report: &mut ValidationReport,
    ) -> Result<(), ShaclError> {
        let node_id = node_id_str(node);
        let shape_name = &source_shape.name;

        // 沿属性路径收集所有到达的值
        let values = self.walk_property_path(node, &prop_shape.path)?;

        // 评估属性级约束
        for constraint in &prop_shape.constraints {
            // 存在性约束（required / minCount）在值集合为空时也需要评估
            match constraint {
                Constraint::Required if values.is_empty() => {
                    report.add_result(ValidationResult::violation(
                        &node_id,
                        shape_name,
                        "Required",
                        format!(
                            "Required property '{}' is missing or empty",
                            path_display(&prop_shape.path)
                        ),
                    ));
                    continue;
                }
                Constraint::MinCount(n) if values.len() < *n => {
                    report.add_result(ValidationResult::violation(
                        &node_id,
                        shape_name,
                        "MinCount",
                        format!(
                            "Property '{}' has {} value(s), minimum is {}",
                            path_display(&prop_shape.path),
                            values.len(),
                            n
                        ),
                    ));
                    continue;
                }
                Constraint::MaxCount(n) if values.len() > *n => {
                    report.add_result(ValidationResult::violation(
                        &node_id,
                        shape_name,
                        "MaxCount",
                        format!(
                            "Property '{}' has {} value(s), maximum is {}",
                            path_display(&prop_shape.path),
                            values.len(),
                            n
                        ),
                    ));
                    continue;
                }
                _ => {}
            }

            // 对每个值评估
            for val in &values {
                if let Some(result) = self.evaluate_constraint_on_value(
                    node,
                    val,
                    constraint,
                    shape_name,
                    &node_id,
                    &prop_shape.path,
                ) {
                    report.add_result(result);
                }
            }

            // 值唯一性约束（跨所有焦点节点）
            if matches!(constraint, Constraint::UniqueValue) {
                // 在同一形状的范围内检查唯一性
                // NOTE: 完整的唯一性检查需要跨节点去重，此处对当前节点做基础检查
                if values.len() > 1 {
                    // 检查是否有重复值
                    let mut seen_vals: Vec<&PropertyValue> = Vec::new();
                    for (i, val) in values.iter().enumerate() {
                        if seen_vals.contains(&val) {
                            report.add_result(
                                ValidationResult::violation(
                                    &node_id,
                                    shape_name,
                                    "UniqueValue",
                                    format!(
                                        "Duplicate value at position {} in property '{}'",
                                        i,
                                        path_display(&prop_shape.path),
                                    ),
                                )
                                .with_value(format!("{:?}", val)),
                            );
                        }
                        seen_vals.push(val);
                    }
                }
            }
        }

        Ok(())
    }

    // ═══════════════════════════════════════════════════════
    // 属性路径遍历
    // ═══════════════════════════════════════════════════════

    /// 沿属性路径从起始节点收集所有到达的值。
    ///
    /// 返回值列表（可能为空）。
    fn walk_property_path(
        &self,
        start: &Node,
        path: &PropertyPath,
    ) -> Result<Vec<PropertyValue>, ShaclError> {
        match path {
            PropertyPath::Predicate(prop) => {
                // 直接从节点属性取值
                if let Some(val) = start.properties.get(prop) {
                    return Ok(vec![val.clone()]);
                }
                // 尝试从出边关系取值
                let rels = self
                    .repo
                    .get_relationships(&node_id_str(start), Some(prop))?;
                let mut values = Vec::new();
                for rel in &rels {
                    if let Some(target_node) = self.repo.get_node(&rel.end_node_id)? {
                        // 将目标节点作为值返回（用于继续路径遍历）
                        // 也收集目标节点上所有属性
                        for (k, v) in &target_node.properties {
                            if k != "id" && k != "iri" {
                                values.push(v.clone());
                            }
                        }
                    }
                }
                Ok(values)
            }
            PropertyPath::Sequence(steps) => {
                let mut current_nodes: Vec<Node> = vec![start.clone()];
                for step in steps {
                    let mut next_nodes = Vec::new();
                    for cn in &current_nodes {
                        let vals =
                            self.walk_property_path(cn, &PropertyPath::Predicate(step.clone()))?;
                        // 收集到的值需要解析为节点引用
                        for v in &vals {
                            if let Some(id_str) = v.as_str()
                                && let Some(target) = self.repo.get_node(id_str)?
                            {
                                next_nodes.push(target);
                            }
                        }
                    }
                    current_nodes = next_nodes;
                    if current_nodes.is_empty() {
                        break;
                    }
                }
                // 返回最后一跳到达的节点的所有属性值
                let mut result = Vec::new();
                for cn in &current_nodes {
                    for (k, v) in &cn.properties {
                        if k != "id" && k != "iri" {
                            result.push(v.clone());
                        }
                    }
                }
                Ok(result)
            }
            PropertyPath::InversePath(sub_path) => {
                // 反向路径：查找入边关系
                let start_id = node_id_str(start);
                // 从 sub_path 提取关系类型过滤器
                let rel_filter: Option<&str> = match sub_path.as_ref() {
                    PropertyPath::Predicate(p) => Some(p.as_str()),
                    _ => None,
                };
                let labels = unified_mapping::DOMAIN_LABELS;
                let mut values = Vec::new();
                for label in labels {
                    let nodes = self.repo.get_nodes_by_label(label)?;
                    for node in &nodes {
                        let nid = node_id_str(node);
                        let rels = self.repo.get_relationships(&nid, rel_filter)?;
                        for rel in &rels {
                            if rel.end_node_id == start_id {
                                for (k, v) in &node.properties {
                                    if k != "id" && k != "iri" {
                                        values.push(v.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(values)
            }
            PropertyPath::Alternative(paths) => {
                let mut all_values = Vec::new();
                for alt_path in paths {
                    let vals = self.walk_property_path(start, alt_path)?;
                    all_values.extend(vals);
                }
                Ok(all_values)
            }
            PropertyPath::ZeroOrMore(sub_path) => {
                // 零或多次：第 0 跳就是 start 自身
                let mut all_values = Vec::new();
                // 第 0 跳
                for (k, v) in &start.properties {
                    if k != "id" && k != "iri" {
                        all_values.push(v.clone());
                    }
                }
                // 后续跳（广度优先，带 visited 防环）
                let mut visited: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                visited.insert(node_id_str(start));
                let mut frontier = vec![start.clone()];
                while !frontier.is_empty() {
                    let mut next_frontier = Vec::new();
                    for node in &frontier {
                        let vals = self.walk_property_path(node, sub_path)?;
                        for v in &vals {
                            all_values.push(v.clone());
                            if let Some(id_str) = v.as_str()
                                && !visited.contains(id_str)
                            {
                                visited.insert(id_str.to_string());
                                if let Some(target) = self.repo.get_node(id_str)? {
                                    next_frontier.push(target);
                                }
                            }
                        }
                    }
                    frontier = next_frontier;
                }
                Ok(all_values)
            }
            PropertyPath::OneOrMore(sub_path) => {
                // 一或多次：同 ZeroOrMore 但不包含第 0 跳
                let mut all_values = Vec::new();
                let mut visited: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                visited.insert(node_id_str(start));
                let mut frontier = vec![start.clone()];
                while !frontier.is_empty() {
                    let mut next_frontier = Vec::new();
                    for node in &frontier {
                        let vals = self.walk_property_path(node, sub_path)?;
                        for v in &vals {
                            all_values.push(v.clone());
                            if let Some(id_str) = v.as_str()
                                && !visited.contains(id_str)
                            {
                                visited.insert(id_str.to_string());
                                if let Some(target) = self.repo.get_node(id_str)? {
                                    next_frontier.push(target);
                                }
                            }
                        }
                    }
                    frontier = next_frontier;
                }
                Ok(all_values)
            }
        }
    }

    // ═══════════════════════════════════════════════════════
    // 约束评估
    // ═══════════════════════════════════════════════════════

    /// 评估节点级约束（不需要属性路径上下文）。
    ///
    /// 返回 `None` 表示约束通过，返回 `Some(ValidationResult)` 表示违规。
    fn evaluate_constraint(
        &self,
        node: &Node,
        _value: Option<&PropertyValue>,
        constraint: &Constraint,
        shape_name: &str,
        node_id: &str,
    ) -> Option<ValidationResult> {
        match constraint {
            Constraint::Required => {
                // Node-level required: 节点必须有属性值
                None // 节点总是存在，此约束在属性级才有意义
            }
            Constraint::NodeKind(expected_kind) => {
                // 检查节点标签是否符合 NodeKind
                let matched = match expected_kind {
                    NodeKind::Iri | NodeKind::Literal | NodeKind::BlankNode => {
                        // 属性图节点不严格区分这些，总是通过
                        true
                    }
                    _ => true,
                };
                if !matched {
                    return Some(ValidationResult::violation(
                        node_id,
                        shape_name,
                        "NodeKind",
                        format!(
                            "Node kind mismatch: expected {}, got IRI node",
                            expected_kind
                        ),
                    ));
                }
                None
            }
            Constraint::Class(expected_label) => {
                if !node.labels.contains(expected_label) {
                    return Some(ValidationResult::violation(
                        node_id,
                        shape_name,
                        "Class",
                        format!(
                            "Node must have label '{}', found: {:?}",
                            expected_label, node.labels
                        ),
                    ));
                }
                None
            }
            Constraint::Not(sub) => {
                // 如果子约束通过，则 NOT 失败
                if self
                    .evaluate_constraint(node, None, sub, shape_name, node_id)
                    .is_none()
                {
                    return Some(ValidationResult::violation(
                        node_id,
                        shape_name,
                        "Not",
                        "NOT constraint failed: sub-constraint unexpectedly passed".to_string(),
                    ));
                }
                None
            }
            Constraint::And(constraints) => {
                for sub in constraints {
                    if let Some(result) =
                        self.evaluate_constraint(node, None, sub, shape_name, node_id)
                    {
                        return Some(result);
                    }
                }
                None
            }
            Constraint::Or(constraints) => {
                let any_pass = constraints.iter().any(|sub| {
                    self.evaluate_constraint(node, None, sub, shape_name, node_id)
                        .is_none()
                });
                if !any_pass {
                    return Some(ValidationResult::violation(
                        node_id,
                        shape_name,
                        "Or",
                        format!(
                            "OR constraint failed: none of {} alternatives passed",
                            constraints.len()
                        ),
                    ));
                }
                None
            }
            Constraint::Xone(constraints) => {
                let pass_count = constraints
                    .iter()
                    .filter(|sub| {
                        self.evaluate_constraint(node, None, sub, shape_name, node_id)
                            .is_none()
                    })
                    .count();
                if pass_count != 1 {
                    return Some(ValidationResult::violation(
                        node_id,
                        shape_name,
                        "Xone",
                        format!(
                            "XONE constraint failed: exactly 1 must pass, {} passed",
                            pass_count
                        ),
                    ));
                }
                None
            }
            _ => {
                // 其他约束在属性值上下文中才有意义
                None
            }
        }
    }

    /// 评估属性值级约束。
    ///
    /// 返回 `None` 表示通过，`Some(ValidationResult)` 表示违规。
    fn evaluate_constraint_on_value(
        &self,
        _node: &Node,
        value: &PropertyValue,
        constraint: &Constraint,
        shape_name: &str,
        node_id: &str,
        path: &PropertyPath,
    ) -> Option<ValidationResult> {
        let err = |constraint_name: &str, message: String| {
            ValidationResult::violation(node_id, shape_name, constraint_name, message)
                .with_result_path(path_display(path))
                .with_value(format!("{:?}", value))
        };

        match constraint {
            // ── 数据类型约束 ──
            Constraint::Datatype(kind) => match kind {
                NodeKind::String => {
                    if !matches!(value, PropertyValue::String(_)) {
                        return Some(err("Datatype", "Expected String".to_string()));
                    }
                }
                NodeKind::Int => {
                    if !matches!(value, PropertyValue::Integer(_)) {
                        return Some(err("Datatype", "Expected Int".to_string()));
                    }
                }
                NodeKind::Float => {
                    if !matches!(value, PropertyValue::Float(_) | PropertyValue::Integer(_)) {
                        return Some(err("Datatype", "Expected Float".to_string()));
                    }
                }
                NodeKind::Bool => {
                    if !matches!(value, PropertyValue::Boolean(_)) {
                        return Some(err("Datatype", "Expected Bool".to_string()));
                    }
                }
                NodeKind::List => {
                    if !matches!(value, PropertyValue::List(_)) {
                        return Some(err("Datatype", "Expected List".to_string()));
                    }
                }
                NodeKind::Object if !matches!(value, PropertyValue::Map(_)) => {
                    return Some(err("Datatype", "Expected Object/Map".to_string()));
                }
                _ => {
                    // IRI, Literal, BlankNode — 在属性图中没有直接对应
                }
            },

            // ── 值范围约束 ──
            Constraint::MinInclusive(min) => {
                let num = value_to_f64(value);
                if num.is_some_and(|n| n < *min) {
                    return Some(err("MinInclusive", format!("Value < {}", min)));
                }
            }
            Constraint::MaxInclusive(max) => {
                let num = value_to_f64(value);
                if num.is_some_and(|n| n > *max) {
                    return Some(err("MaxInclusive", format!("Value > {}", max)));
                }
            }
            Constraint::MinExclusive(min) => {
                let num = value_to_f64(value);
                if num.is_some_and(|n| n <= *min) {
                    return Some(err("MinExclusive", format!("Value <= {}", min)));
                }
            }
            Constraint::MaxExclusive(max) => {
                let num = value_to_f64(value);
                if num.is_some_and(|n| n >= *max) {
                    return Some(err("MaxExclusive", format!("Value >= {}", max)));
                }
            }

            // ── 字符串约束 ──
            Constraint::MinLength(min) => {
                if let Some(s) = value.as_str() {
                    if s.len() < *min {
                        return Some(err(
                            "MinLength",
                            format!("String length {} < {}", s.len(), min),
                        ));
                    }
                } else if let PropertyValue::List(list) = value
                    && list.len() < *min
                {
                    return Some(err(
                        "MinLength",
                        format!("List length {} < {}", list.len(), min),
                    ));
                }
            }
            Constraint::MaxLength(max) => {
                if let Some(s) = value.as_str() {
                    if s.len() > *max {
                        return Some(err(
                            "MaxLength",
                            format!("String length {} > {}", s.len(), max),
                        ));
                    }
                } else if let PropertyValue::List(list) = value
                    && list.len() > *max
                {
                    return Some(err(
                        "MaxLength",
                        format!("List length {} > {}", list.len(), max),
                    ));
                }
            }
            Constraint::Pattern(regex) => {
                if let Some(s) = value.as_str() {
                    match regex::Regex::new(regex) {
                        Ok(re) => {
                            if !re.is_match(s) {
                                return Some(err(
                                    "Pattern",
                                    format!("'{}' does not match pattern /{}/", s, regex),
                                ));
                            }
                        }
                        Err(e) => {
                            return Some(err("Pattern", format!("Invalid regex: {}", e)));
                        }
                    }
                }
            }
            Constraint::LanguageIn(_langs) => {
                // 属性图模型通常无语言标签，跳过
            }

            // ── 枚举 / 值约束 ──
            Constraint::In(allowed) => {
                if !allowed.iter().any(|v| prop_matches(value, v)) {
                    return Some(err(
                        "In",
                        format!("Value not in allowed set: {:?}", allowed),
                    ));
                }
            }
            Constraint::HasValue(expected) => {
                if !prop_matches(value, expected) {
                    return Some(err(
                        "HasValue",
                        format!("Does not match expected value: {:?}", expected),
                    ));
                }
            }

            // ── 非空约束 ──
            Constraint::NonEmpty => {
                let is_empty = match value {
                    PropertyValue::String(s) => s.is_empty(),
                    PropertyValue::List(v) => v.is_empty(),
                    PropertyValue::Null => true,
                    _ => false,
                };
                if is_empty {
                    return Some(err("NonEmpty", "Value is empty".to_string()));
                }
            }

            // ── 逻辑组合 ──
            Constraint::Not(sub) => {
                if self
                    .evaluate_constraint_on_value(_node, value, sub, shape_name, node_id, path)
                    .is_none()
                {
                    return Some(err("Not", "NOT constraint failed".to_string()));
                }
            }
            Constraint::And(constraints) => {
                for sub in constraints {
                    if let Some(result) = self
                        .evaluate_constraint_on_value(_node, value, sub, shape_name, node_id, path)
                    {
                        return Some(result);
                    }
                }
            }
            Constraint::Or(constraints) => {
                let any_pass = constraints.iter().any(|sub| {
                    self.evaluate_constraint_on_value(_node, value, sub, shape_name, node_id, path)
                        .is_none()
                });
                if !any_pass {
                    return Some(err(
                        "Or",
                        format!("All {} alternatives failed", constraints.len()),
                    ));
                }
            }
            Constraint::Xone(constraints) => {
                let pass_count = constraints
                    .iter()
                    .filter(|sub| {
                        self.evaluate_constraint_on_value(
                            _node, value, sub, shape_name, node_id, path,
                        )
                        .is_none()
                    })
                    .count();
                if pass_count != 1 {
                    return Some(err(
                        "Xone",
                        format!("Exactly 1 must pass, {} passed", pass_count),
                    ));
                }
            }

            // ── 形状引用约束 ──
            Constraint::QualifiedValueShape {
                shape,
                qualified_min_count,
                qualified_max_count,
            } => {
                // 递归验证子形状（仅支持单值场景）
                let sub_shape = shape;
                let mut sub_report = ValidationReport::new();
                if let Shape::NodeShape(ns) = sub_shape.as_ref()
                    // 尝试将 value 解析为节点
                    && let Some(id_str) = value.as_str()
                    && let Ok(Some(target)) = self.repo.get_node(id_str)
                {
                    let _ = self.validate_node_shape(ns, &target, &mut sub_report);
                }
                let qualified_count = if sub_report.conforms { 1 } else { 0 };
                if let Some(min) = qualified_min_count
                    && qualified_count < *min
                {
                    return Some(err(
                        "QualifiedValueShape",
                        format!("Qualified count {} < min {}", qualified_count, min),
                    ));
                }
                if let Some(max) = qualified_max_count
                    && qualified_count > *max
                {
                    return Some(err(
                        "QualifiedValueShape",
                        format!("Qualified count {} > max {}", qualified_count, max),
                    ));
                }
            }

            // ── 节点级约束（在属性值上下文跳过）──
            Constraint::Required
            | Constraint::MinCount(_)
            | Constraint::MaxCount(_)
            | Constraint::UniqueValue
            | Constraint::Class(_)
            | Constraint::NodeKind(_)
            | Constraint::Custom { .. } => {
                // 这些在 validate_property_shape_on_node 中处理，或暂不支持
            }
        }

        None // 通过
    }
}

// ═══════════════════════════════════════════════════════════
// 辅助函数
// ═══════════════════════════════════════════════════════════

/// 从节点中提取标识符（优先 iri，其次 id 属性值）
fn node_id_str(node: &Node) -> String {
    node.properties
        .get("iri")
        .and_then(|v| v.as_str())
        .or_else(|| node.properties.get("id").and_then(|v| v.as_str()))
        .map(String::from)
        .unwrap_or_else(|| format!("{:p}", node))
}

/// 获取 PropertyPath 的可显示字符串
fn path_display(path: &PropertyPath) -> String {
    match path {
        PropertyPath::Predicate(p) => p.clone(),
        PropertyPath::Sequence(steps) => steps.join("/"),
        PropertyPath::InversePath(sub) => format!("^{}", path_display(sub)),
        PropertyPath::Alternative(paths) => {
            let ps: Vec<String> = paths.iter().map(path_display).collect();
            ps.join("|")
        }
        PropertyPath::ZeroOrMore(sub) => format!("({})*", path_display(sub)),
        PropertyPath::OneOrMore(sub) => format!("({})+", path_display(sub)),
    }
}

/// 将 PropertyValue 转换为 f64（若可能）
fn value_to_f64(v: &PropertyValue) -> Option<f64> {
    match v {
        PropertyValue::Integer(i) => Some(*i as f64),
        PropertyValue::Float(f) => Some(*f),
        PropertyValue::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

/// 检查两个 PropertyValue 是否匹配
fn prop_matches(pv: &PropertyValue, expected: &PropertyValue) -> bool {
    pv == expected
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use ontology_storage::adapters::in_memory::executor::InMemoryAdapter;
    use std::collections::HashMap;

    fn make_test_node(id: &str, label: &str, code: &str) -> Node {
        let mut props = HashMap::new();
        props.insert("id".to_string(), PropertyValue::String(id.to_string()));
        props.insert("code".to_string(), PropertyValue::String(code.to_string()));
        Node::new(vec![label.to_string()], props)
    }

    fn make_test_repo() -> Rc<InMemoryAdapter> {
        let repo = Rc::new(InMemoryAdapter::new());
        repo.insert_node(&make_test_node("n1", "Entity", "E001"))
            .unwrap();
        repo.insert_node(&make_test_node("n2", "Entity", "E002"))
            .unwrap();
        repo.insert_node(&make_test_node("n3", "Event", "EV001"))
            .unwrap();
        repo
    }

    #[test]
    fn test_validate_target_class() {
        let repo = make_test_repo();
        let mut sg = ShapesGraph::new();
        sg.add_shape(
            Shape::node("EntityCheck")
                .with_target(Target::target_class("Entity"))
                .with_property(
                    PropertyShape::new("code")
                        .with_path(PropertyPath::predicate("code"))
                        .with_constraint(Constraint::required()),
                ),
        );

        let engine = ShaclEngine::new(repo, sg);
        let report = engine.validate().unwrap();
        assert!(report.conforms);
        assert_eq!(report.total_shapes, 1);
    }

    #[test]
    fn test_validate_missing_property() {
        let repo = make_test_repo();
        let mut sg = ShapesGraph::new();
        sg.add_shape(
            Shape::node("MissingCheck")
                .with_target(Target::target_class("Event"))
                .with_property(
                    PropertyShape::new("missing_prop")
                        .with_path(PropertyPath::predicate("non_existent"))
                        .with_constraint(Constraint::required()),
                ),
        );

        let engine = ShaclEngine::new(repo, sg);
        let report = engine.validate().unwrap();
        assert!(!report.conforms);
        assert_eq!(report.violation_count, 1);
    }

    #[test]
    fn test_value_range_constraint() {
        let pv = PropertyValue::Integer(42);
        let node = make_test_node("test", "Entity", "T001");

        // Test MinInclusive pass
        let c = Constraint::MinInclusive(0.0);
        assert!(
            evaluate_constraint_on_value_static(
                &node,
                &pv,
                &c,
                "S",
                "n",
                &PropertyPath::Predicate("x".into())
            )
            .is_none()
        );

        // Test MinInclusive fail
        let c = Constraint::MinInclusive(100.0);
        assert!(
            evaluate_constraint_on_value_static(
                &node,
                &pv,
                &c,
                "S",
                "n",
                &PropertyPath::Predicate("x".into())
            )
            .is_some()
        );
    }

    #[test]
    fn test_pattern_constraint() {
        let pv = PropertyValue::String("ABC123".to_string());
        let node = make_test_node("test", "Entity", "T001");
        let path = PropertyPath::predicate("code");

        // Pass: matches pattern
        let c = Constraint::pattern(r"^[A-Z]+\d+$");
        assert!(evaluate_constraint_on_value_static(&node, &pv, &c, "S", "n", &path).is_none());

        // Fail: doesn't match pattern
        let pv2 = PropertyValue::String("abc".to_string());
        assert!(evaluate_constraint_on_value_static(&node, &pv2, &c, "S", "n", &path).is_some());
    }

    // 静态辅助函数（用于单元测试独立调用）
    fn evaluate_constraint_on_value_static<'a>(
        node: &Node,
        value: &PropertyValue,
        constraint: &Constraint,
        shape_name: &str,
        node_id: &str,
        path: &PropertyPath,
    ) -> Option<ValidationResult> {
        let repo = Rc::new(InMemoryAdapter::new());
        let sg = ShapesGraph::new();
        let engine = ShaclEngine::new(repo, sg);
        engine.evaluate_constraint_on_value(node, value, constraint, shape_name, node_id, path)
    }

    #[test]
    fn test_path_display() {
        assert_eq!(path_display(&PropertyPath::predicate("name")), "name");
        assert_eq!(
            path_display(&PropertyPath::sequence(vec!["a", "b", "c"])),
            "a/b/c"
        );
    }
}
