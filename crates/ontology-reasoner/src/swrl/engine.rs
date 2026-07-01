//! SWRL 推理引擎 — 匹配前提 → 绑定变量 → 推导结论。
//!
//! ## 执行流程
//!
//! ```text
//! 1. 加载规则集
//! 2. for each rule:
//!     a. 解析前提原子 → 生成候选图模式
//!     b. 匹配图模式 → 生成变量绑定集合
//!     c. 对于每个绑定：
//!        i.   计算置信度（前提匹配度 + 结构/属性/基数维度）
//!       ii.   置信度熔断检查（< 0.3 → 中止当前规则链路）
//!      iii.   代入结论原子 → 生成新事实
//!     d. 将新事实写入图数据库
//! 3. 重复步骤 2 直到固定点或达到最大迭代次数
//! ```
//!
//! ## 固定点语义
//!
//! 引擎采用 **naive fixpoint** 策略 — 每次迭代对所有规则进行完全匹配，
//! 只将新推导的事实插入图，当一轮迭代没有生成新事实时停止。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use ontology_storage::mapper::graph::property::PropertyValue;
use ontology_storage::mapper::graph::pattern::{
    GraphPattern, NodePattern, RelationshipPattern,
};
use ontology_storage::repository::graph_store::SharedRepository;

use crate::confidence::calculator::{ConfidenceCalculator, ConfidenceInput};
use crate::confidence::fuse::ConfidenceFuse;
use crate::error::ReasonerError;
use crate::swrl::ast::{Atom, ExecutionStats, InferenceResult, Rule, VariableBinding};
use crate::swrl::builtins::{BuiltinRegistry, BuiltinValue};
use crate::confidence::policy::ConfidencePolicy;
use crate::dwl2::ast::ClassExpression;
use crate::dwl2::query::Dwl2QueryEngine;

/// SWRL 推理引擎 — 持有图仓库、内置函数注册表和置信度评估器。

pub struct SwrlEngine {
    repo: SharedRepository,
    builtins: BuiltinRegistry,
    confidence_calc: ConfidenceCalculator,
    max_iterations: usize,
    derived_facts: HashSet<String>,
    verbose: bool,
    policy: Option<ConfidencePolicy>,
    dwl2_engine: Option<Dwl2QueryEngine>,
}

impl SwrlEngine {
    pub fn new(repo: SharedRepository) -> Self {
        let dwl2_engine = Some(Dwl2QueryEngine::new(Arc::clone(&repo)));
        Self {
            repo,
            builtins: BuiltinRegistry::new(),
            confidence_calc: ConfidenceCalculator::default(),
            max_iterations: 100,
            derived_facts: HashSet::new(),
            verbose: false,
            policy: None,
            dwl2_engine,
        }
    }

    pub fn with_max_iterations(mut self, n: usize) -> Self {
        self.max_iterations = n;
        self
    }

    pub fn with_verbose(mut self, v: bool) -> Self {
        self.verbose = v;
        self
    }

    pub fn with_policy(mut self, policy: ConfidencePolicy) -> Self {
        self.policy = Some(policy);
        self
    }

    /// 运行时更新策略（不影响已创建的 Fuse 实例，但后续推理步骤生效）
    pub fn update_policy(&mut self, policy: ConfidencePolicy) {
        self.policy = Some(policy);
    }

    // ═══════════════════════════════════════════════════════
    // 公共接口：规则执行
    // ═══════════════════════════════════════════════════════

    /// 执行规则集（fixpoint 循环），返回所有推理步骤的结果和累计统计。
    pub fn execute_rules(
        &mut self,
        rules: &[Rule],
    ) -> Result<(Vec<InferenceResult>, ExecutionStats), ReasonerError> {
        let start = Instant::now();
        let mut all_results = Vec::new();
        let mut stats = ExecutionStats::default();

        if self.verbose {
            log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            log::info!("⚡ SWRL 推理引擎启动");
            log::info!("   规则数: {}  最大迭代: {}", rules.len(), self.max_iterations);
            log::info!("   已推导事实缓存: {} 条", self.derived_facts.len());
        }

        for iteration in 1..=self.max_iterations {
            let round_start_new_facts = self.derived_facts.len();
            stats.total_steps = iteration;

            if self.verbose {
                log::info!("── 迭代 #{}/{} ──", iteration, self.max_iterations);
            }

            let mut round_derived = 0usize;

            for rule in rules {
                let rule_name = rule.name.as_deref().unwrap_or("<anonymous>");
                let rule_start = Instant::now();

                match self.execute_rule(rule) {
                    Ok(step_results) => {
                        for sr in &step_results {
                            let new_count = self.insert_derived_facts(&sr.derived_facts)?;
                            stats.total_derived += new_count;
                            round_derived += new_count;

                            if new_count > 0 {
                                if self.verbose {
                                    log::info!(
                                        "   ✅ [{}] 绑定={} 置信度={:.4} 新事实={}",
                                        rule_name, sr.binding_count, sr.confidence, new_count
                                    );
                                    for fact in &sr.derived_facts {
                                        log::info!("      ↳ {:?}", format_atom(fact));
                                    }
                                }
                                all_results.push(sr.clone());
                            } else if self.verbose && !step_results.is_empty() {
                                log::info!(
                                    "   ⏭  [{}] 绑定={} 置信度={:.4} (已有事实，跳过)",
                                    rule_name, step_results[0].binding_count,
                                    step_results[0].confidence
                                );
                            }
                        }
                        if self.verbose && step_results.is_empty() {
                            let elapsed = rule_start.elapsed().as_micros() as f64 / 1000.0;
                            log::debug!(
                                "   ⏹  [{}] 无匹配 ({} us)",
                                rule_name, elapsed as u64
                            );
                        }
                    }
                    Err(ReasonerError::ConfidenceFuse { confidence, threshold, rule_name: rn }) => {
                        stats.fuse_trips += 1;
                        if self.verbose {
                            log::warn!(
                                "   🔥 [{}] 置信度熔断! {:.4} < {:.4}",
                                rn, confidence, threshold
                            );
                        }
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }

            if self.verbose {
                log::info!("   ── 本迭代: +{} 新事实, 累计 {} ──", round_derived, self.derived_facts.len());
            }

            if self.derived_facts.len() == round_start_new_facts {
                if self.verbose {
                    log::info!("✅ 固定点收敛 — 无新事实产生，推理结束");
                }
                break;
            }
        }

        stats.total_ms = start.elapsed().as_millis() as u64;

        if self.verbose {
            log::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            log::info!("📊 推理汇总:");
            log::info!("   总迭代: {}  新事实: {}  熔断: {}  耗时: {} ms",
                stats.total_steps, stats.total_derived, stats.fuse_trips, stats.total_ms);
            if stats.total_steps >= self.max_iterations {
                log::warn!("⚠ 达到最大迭代次数上限，可能未收敛!");
            }
        }

        Ok((all_results, stats))
    }

    /// 执行单条规则
    pub fn execute_rule(&self, rule: &Rule) -> Result<Vec<InferenceResult>, ReasonerError> {
        let rule_name = rule.name.as_deref().unwrap_or("<anonymous>");
        let mut fuse = if let Some(ref policy) = self.policy {
            ConfidenceFuse::with_policy(rule_name, policy)
        } else {
            ConfidenceFuse::with_default_threshold(rule_name)
        };

        let t0 = Instant::now();
        let bindings = self.match_antecedent(rule)?;

        if self.verbose {
            let match_ms = t0.elapsed().as_micros() as f64 / 1000.0;
            if bindings.is_empty() {
                log::debug!("   🔍 [{}] 前提匹配: 0 绑定 ({:.1} ms)", rule_name, match_ms);
            } else {
                log::info!(
                    "   🔍 [{}] 前提匹配: {} 绑定 ({:.1} ms)",
                    rule_name, bindings.len(), match_ms
                );
                // 打印前 3 个绑定样例
                for (i, binding) in bindings.iter().take(3).enumerate() {
                    let vars: Vec<String> = binding.iter()
                        .map(|(k, v)| format!("{}={}", k, v))
                        .collect();
                    log::debug!("     绑定#{}: {{ {} }}", i + 1, vars.join(", "));
                }
                if bindings.len() > 3 {
                    log::debug!("     ... 还有 {} 个绑定", bindings.len() - 3);
                }
            }
        }

        if bindings.is_empty() {
            return Ok(vec![]);
        }

        let antecedent_atom_count = rule.antecedent.len();
        let mut results = Vec::new();

        for (bi, binding) in bindings.iter().enumerate() {
            let match_ratio = if antecedent_atom_count > 0 {
                binding.len() as f64 / antecedent_atom_count.max(1) as f64
            } else {
                1.0
            };

            let conf_input = ConfidenceInput {
                source_match: match_ratio.min(1.0),
                source_cardinal: 1.0,
                source_property: 1.0,
                source_structural: (bindings.len() as f64).recip().max(0.1),
            };
            let confidence = self.confidence_calc.evaluate(&conf_input);

            if self.verbose {
                log::info!(
                    "      📐 绑定#{}/{} 置信度={:.4} (match={:.2} card={:.2} prop={:.2} struct={:.2})",
                    bi + 1, bindings.len(), confidence,
                    conf_input.source_match, conf_input.source_cardinal,
                    conf_input.source_property, conf_input.source_structural
                );
            }

            fuse.guard(confidence)?;

            let derived: Vec<Atom> = rule
                .consequent
                .iter()
                .map(|atom| self.substitute_atom(atom, binding))
                .collect();

            results.push(InferenceResult {
                rule_name: rule.name.clone().unwrap_or_else(|| "<anonymous>".into()),
                derived_facts: derived,
                confidence,
                binding_count: bindings.len(),
            });
        }

        Ok(results)
    }

    // ═══════════════════════════════════════════════════════
    // 前提匹配
    // ═══════════════════════════════════════════════════════

    fn match_antecedent(&self, rule: &Rule) -> Result<Vec<VariableBinding>, ReasonerError> {
        if rule.antecedent.is_empty() {
            return Ok(vec![HashMap::new()]);
        }

        let mut class_atoms: Vec<&Atom> = Vec::new();
        let mut prop_atoms: Vec<&Atom> = Vec::new();
        let mut builtin_atoms: Vec<&Atom> = Vec::new();
        let mut eq_atoms: Vec<&Atom> = Vec::new();
        let mut query_atoms: Vec<&Atom> = Vec::new();

        for atom in &rule.antecedent {
            match atom {
                Atom::ClassAtom { .. } => class_atoms.push(atom),
                Atom::ObjectPropertyAtom { .. } | Atom::DataPropertyAtom { .. } => {
                    prop_atoms.push(atom)
                }
                Atom::Builtin { .. } => builtin_atoms.push(atom),
                Atom::SameAs(..) | Atom::DifferentFrom(..) => eq_atoms.push(atom),
                Atom::Query { .. } => query_atoms.push(atom),
            }
        }

        let seed_bindings = if let Some(first) = class_atoms.first() {
            self.match_class_atom(first)?
        } else if let Some(first) = prop_atoms.first() {
            self.match_property_atom(first)?
        } else {
            vec![HashMap::new()]
        };

        if seed_bindings.is_empty() {
            return Ok(vec![]);
        }

        let mut current = seed_bindings;
        for atom in class_atoms.iter().skip(1) {
            let atom_bindings = self.match_class_atom(atom)?;
            current = join_bindings(&current, &atom_bindings);
            if current.is_empty() {
                return Ok(vec![]);
            }
        }

        for atom in &prop_atoms {
            let atom_bindings = self.match_property_atom(atom)?;
            current = join_bindings(&current, &atom_bindings);
            if current.is_empty() {
                return Ok(vec![]);
            }
        }

        for atom in &eq_atoms {
            filter_equality(&mut current, atom);
            if current.is_empty() {
                return Ok(vec![]);
            }
        }

        for atom in &builtin_atoms {
            self.filter_builtin_by(&mut current, atom);
            if current.is_empty() {
                return Ok(vec![]);
            }
        }

        // 处理 DWL2 Query 原子 — 执行类表达式查询，绑定结果 IRI
        for atom in &query_atoms {
            let atom_bindings = self.match_query_atom(atom)?;
            current = join_bindings(&current, &atom_bindings);
            if current.is_empty() {
                return Ok(vec![]);
            }
        }

        Ok(current)
    }

    fn match_class_atom(&self, atom: &Atom) -> Result<Vec<VariableBinding>, ReasonerError> {
        if let Atom::ClassAtom { class_iri, variable } = atom {
            let pattern = GraphPattern::new(
                NodePattern::with_label("Individual").with_variable("ind"),
                RelationshipPattern::with_type("INSTANCE_OF").with_variable("r"),
                NodePattern::with_label("Class")
                    .with_variable("c")
                    .with_property("iri", PropertyValue::from(class_iri.as_str())),
            );
            let results = self.repo.query_pattern(&pattern)?;
            Ok(results
                .iter()
                .filter_map(|(ind, _, _)| {
                    let iri = ind.property("iri").and_then(|v| v.as_str())?;
                    let mut b = HashMap::new();
                    b.insert(variable.clone(), iri.to_string());
                    Some(b)
                })
                .collect())
        } else {
            Err(ReasonerError::SwrlExecution("Expected ClassAtom".into()))
        }
    }

    fn match_property_atom(&self, atom: &Atom) -> Result<Vec<VariableBinding>, ReasonerError> {
        match atom {
            Atom::ObjectPropertyAtom { property_iri, subject, object } => {
                let pattern = GraphPattern::new(
                    NodePattern::with_label("Individual").with_variable("s"),
                    RelationshipPattern::with_type(property_iri.as_str()).with_variable("r"),
                    NodePattern::with_label("Individual").with_variable("o"),
                );
                let results = self.repo.query_pattern(&pattern)?;
                Ok(results
                    .iter()
                    .filter_map(|(s, _, o)| {
                        let s_iri = s.property("iri").and_then(|v| v.as_str())?;
                        let o_iri = o.property("iri").and_then(|v| v.as_str())?;
                        let mut b = HashMap::new();
                        b.insert(subject.clone(), s_iri.to_string());
                        b.insert(object.clone(), o_iri.to_string());
                        Some(b)
                    })
                    .collect())
            }
            Atom::DataPropertyAtom { property_iri, subject, value } => {
                let pattern = GraphPattern::new(
                    NodePattern::with_label("Individual").with_variable("s"),
                    RelationshipPattern::with_type("HAS_VALUE").with_variable("r"),
                    NodePattern::with_label("Property").with_variable("p"),
                );
                let results = self.repo.query_pattern(&pattern)?;
                Ok(results
                    .iter()
                    .filter_map(|(s, rels, p)| {
                        let s_iri = s.property("iri").and_then(|v| v.as_str())?;
                        let p_iri = p.property("iri").and_then(|v| v.as_str())?;
                        if p_iri != property_iri.as_str() { return None; }
                        let rel = rels.first()?;
                        let data_val = rel.property("value").and_then(|v| match v {
                            PropertyValue::String(s) => Some(s.clone()),
                            PropertyValue::Integer(i) => Some(i.to_string()),
                            PropertyValue::Float(f) => Some(f.to_string()),
                            PropertyValue::Boolean(b) => Some(b.to_string()),
                            _ => None,
                        })?;
                        let mut b = HashMap::new();
                        b.insert(subject.clone(), s_iri.to_string());
                        if value.starts_with('?') { b.insert(value.clone(), data_val); }
                        Some(b)
                    })
                    .collect())
            }
            _ => Err(ReasonerError::SwrlExecution("Expected PropertyAtom".into())),
        }
    }

    fn filter_builtin_by(&self, bindings: &mut Vec<VariableBinding>, atom: &Atom) {
        if let Atom::Builtin { builtin_iri, arguments } = atom {
            bindings.retain(|binding| {
                let args: Vec<BuiltinValue> = arguments.iter().map(|arg| {
                    if arg.starts_with('?') {
                        binding.get(arg)
                            .map(|v| BuiltinValue::String(v.clone()))
                            .unwrap_or(BuiltinValue::Unbound)
                    } else {
                        parse_literal(arg)
                    }
                }).collect();
                match self.builtins.execute(builtin_iri, &args) {
                    Ok(r) => match r {
                        crate::swrl::builtins::BuiltinResult::Boolean(b) => b,
                        crate::swrl::builtins::BuiltinResult::Deferred => true,
                        _ => true,
                    },
                    Err(_) => false,
                }
            });
        }
    }

    // ═══════════════════════════════════════════════════════
    // 结论代入与事实写入
    // ═══════════════════════════════════════════════════════

    fn substitute_atom(&self, atom: &Atom, binding: &VariableBinding) -> Atom {
        let r = |v: &str| resolve_or_keep(binding, v);
        match atom {
            Atom::ClassAtom { class_iri, variable } => Atom::ClassAtom {
                class_iri: class_iri.clone(), variable: r(variable),
            },
            Atom::ObjectPropertyAtom { property_iri, subject, object } => {
                Atom::ObjectPropertyAtom {
                    property_iri: property_iri.clone(),
                    subject: r(subject), object: r(object),
                }
            }
            Atom::DataPropertyAtom { property_iri, subject, value } => Atom::DataPropertyAtom {
                property_iri: property_iri.clone(),
                subject: r(subject),
                value: if value.starts_with('?') { r(value) } else { value.clone() },
            },
            Atom::SameAs(a, b) => Atom::SameAs(r(&a), r(&b)),
            Atom::DifferentFrom(a, b) => Atom::DifferentFrom(r(&a), r(&b)),
            Atom::Builtin { builtin_iri, arguments } => Atom::Builtin {
                builtin_iri: builtin_iri.clone(),
                arguments: arguments.iter().map(|arg| {
                    if arg.starts_with('?') { r(arg) } else { arg.clone() }
                }).collect(),
            },
            Atom::Query { dwl2_expression, result_variable } => Atom::Query {
                dwl2_expression: dwl2_expression.clone(),
                result_variable: r(result_variable),
            },
        }
    }

    /// 匹配 DWL2 Query 原子 — 执行 ClassExpression 检索
    fn match_query_atom(&self, atom: &Atom) -> Result<Vec<VariableBinding>, ReasonerError> {
        if let Atom::Query { dwl2_expression, result_variable } = atom {
            let expr = ClassExpression::from_key(dwl2_expression)
                .map_err(|e| ReasonerError::SwrlExecution(format!("Query parse: {}", e)))?;
            let engine = self.dwl2_engine.as_ref()
                .ok_or_else(|| ReasonerError::SwrlExecution("DWL2 engine not available".into()))?;
            let instances = engine.retrieve_instances(&expr)?;
            Ok(instances.into_iter().map(|iri| {
                let mut b = HashMap::new();
                b.insert(result_variable.clone(), iri);
                b
            }).collect())
        } else {
            Err(ReasonerError::SwrlExecution("Expected Query atom".into()))
        }
    }

    fn insert_derived_facts(&mut self, facts: &[Atom]) -> Result<usize, ReasonerError> {
        let mut count = 0;
        for fact in facts {
            let key = fact_key(fact);
            if self.derived_facts.contains(&key) { continue; }
            match fact {
                Atom::ClassAtom { class_iri, variable } => {
                    self.repo.insert_relationship(
                        &ontology_storage::mapper::graph::relationship::Relationship::simple(
                            variable, "INSTANCE_OF", class_iri,
                        ),
                    )?;
                }
                Atom::ObjectPropertyAtom { property_iri, subject, object } => {
                    self.repo.insert_relationship(
                        &ontology_storage::mapper::graph::relationship::Relationship::simple(
                            subject, property_iri, object,
                        ),
                    )?;
                }
                _ => {}
            }
            self.derived_facts.insert(key);
            count += 1;
        }
        Ok(count)
    }
}

// ═══════════════════════════════════════════════════════════
// 自由函数
// ═══════════════════════════════════════════════════════════

fn join_bindings(left: &[VariableBinding], right: &[VariableBinding]) -> Vec<VariableBinding> {
    let mut result = Vec::new();
    for lb in left {
        for rb in right {
            let compatible = lb.iter().all(|(var, val)| {
                rb.get(var).map_or(true, |rv| rv == val)
            });
            if compatible {
                let mut merged = lb.clone();
                for (k, v) in rb {
                    merged.entry(k.clone()).or_insert_with(|| v.clone());
                }
                result.push(merged);
            }
        }
    }
    result
}

fn filter_equality(bindings: &mut Vec<VariableBinding>, atom: &Atom) {
    match atom {
        Atom::SameAs(a, b) => bindings.retain(|binding| {
            resolve_var(binding, a) == resolve_var(binding, b)
        }),
        Atom::DifferentFrom(a, b) => bindings.retain(|binding| {
            let va = resolve_var(binding, a);
            let vb = resolve_var(binding, b);
            va != vb || va.is_none()
        }),
        _ => {}
    }
}

fn resolve_var(binding: &VariableBinding, var: &str) -> Option<String> {
    if var.starts_with('?') { binding.get(var).cloned() } else { Some(var.to_string()) }
}

fn resolve_or_keep(binding: &VariableBinding, var: &str) -> String {
    resolve_var(binding, var).unwrap_or_else(|| var.to_string())
}

fn parse_literal(s: &str) -> BuiltinValue {
    if let Ok(i) = s.parse::<i64>() { BuiltinValue::Integer(i) }
    else if let Ok(f) = s.parse::<f64>() { BuiltinValue::Float(f) }
    else if s == "true" { BuiltinValue::Boolean(true) }
    else if s == "false" { BuiltinValue::Boolean(false) }
    else { BuiltinValue::String(s.to_string()) }
}

/// 格式化 Atom 为可读字符串（用于日志输出）
fn format_atom(atom: &Atom) -> String {
    match atom {
        Atom::ClassAtom { class_iri, variable } => {
            format!("{}({})", class_iri, variable)
        }
        Atom::ObjectPropertyAtom { property_iri, subject, object } => {
            format!("{}({}, {})", property_iri, subject, object)
        }
        Atom::DataPropertyAtom { property_iri, subject, value } => {
            format!("{}({}, {})", property_iri, subject, value)
        }
        Atom::SameAs(a, b) => format!("sameAs({}, {})", a, b),
        Atom::DifferentFrom(a, b) => format!("differentFrom({}, {})", a, b),
        Atom::Builtin { builtin_iri, arguments } => {
            format!("{}({})", builtin_iri, arguments.join(", "))
        }
        Atom::Query { dwl2_expression, result_variable } => {
            format!("Query({}, {})", dwl2_expression, result_variable)
        }
    }
}

fn fact_key(fact: &Atom) -> String {
    match fact {
        Atom::ClassAtom { class_iri, variable } => format!("class:{}:{}", variable, class_iri),
        Atom::ObjectPropertyAtom { property_iri, subject, object } =>
            format!("prop:{}:{}:{}", subject, property_iri, object),
        Atom::DataPropertyAtom { property_iri, subject, value } =>
            format!("dp:{}:{}:{}", subject, property_iri, value),
        _ => format!("other:{:?}", fact),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use ontology_storage::adapters::in_memory::executor::InMemoryAdapter;
    use ontology_storage::mapper::graph::node::Node;
    use ontology_storage::mapper::graph::property::PropertyValue;
    use ontology_storage::mapper::graph::relationship::Relationship;
    use ontology_storage::repository::graph_store::GraphRepository;

    use std::collections::HashMap;
    use ontology_storage::repository::graph_store::SharedRepository;

    use crate::swrl::ast::{Atom, Rule};
    use super::SwrlEngine;

    fn setup_family_repo() -> SharedRepository {
        let adapter = Arc::new(InMemoryAdapter::new());

        for class_iri in &["http://ex#Person", "http://ex#Uncle"] {
            let mut props = HashMap::new();
            props.insert("iri".to_string(), PropertyValue::from(*class_iri));
            adapter.insert_node(&Node::new(vec!["Class".to_string()], props)).unwrap();
        }

        for (iri, label) in &[
            ("http://ex#Alice", "Alice"),
            ("http://ex#Bob", "Bob"),
            ("http://ex#Charlie", "Charlie"),
        ] {
            let mut props = HashMap::new();
            props.insert("iri".to_string(), PropertyValue::from(*iri));
            props.insert("label".to_string(), PropertyValue::from(*label));
            adapter.insert_node(&Node::new(vec!["Individual".to_string()], props)).unwrap();
        }

        for (ind, cls) in &[
            ("http://ex#Alice", "http://ex#Person"),
            ("http://ex#Bob", "http://ex#Person"),
            ("http://ex#Charlie", "http://ex#Person"),
        ] {
            adapter.insert_relationship(&Relationship::simple(*ind, "INSTANCE_OF", *cls)).unwrap();
        }

        adapter.insert_relationship(&Relationship::simple(
            "http://ex#Alice", "hasParent", "http://ex#Bob",
        )).unwrap();
        adapter.insert_relationship(&Relationship::simple(
            "http://ex#Bob", "hasBrother", "http://ex#Charlie",
        )).unwrap();

        adapter
    }

    #[test]
    fn test_family_reasoning() {
        let repo = setup_family_repo();
        let mut engine = SwrlEngine::new(repo);

        let rule = Rule::new("uncle_rule", vec![
            Atom::ObjectPropertyAtom {
                property_iri: "hasParent".into(), subject: "?x".into(), object: "?y".into(),
            },
            Atom::ObjectPropertyAtom {
                property_iri: "hasBrother".into(), subject: "?y".into(), object: "?z".into(),
            },
        ], vec![
            Atom::ObjectPropertyAtom {
                property_iri: "hasUncle".into(), subject: "?x".into(), object: "?z".into(),
            },
        ]);

        let (_results, stats) = engine.execute_rules(&[rule]).unwrap();
        assert!(stats.total_derived >= 1);

        // Check derived facts
        let uncle_facts: Vec<&Atom> = _results.iter()
            .flat_map(|r| &r.derived_facts)
            .filter(|a| matches!(a, Atom::ObjectPropertyAtom { property_iri, .. } if property_iri == "hasUncle"))
            .collect();
        assert!(!uncle_facts.is_empty());
    }

    #[test]
    fn test_fixpoint_terminates() {
        let repo = setup_family_repo();
        let mut engine = SwrlEngine::new(repo).with_max_iterations(5);

        let rule = Rule::new("terminates", vec![
            Atom::ClassAtom { class_iri: "http://ex#Person".into(), variable: "?x".into() },
        ], vec![
            Atom::ClassAtom { class_iri: "http://ex#Person".into(), variable: "?x".into() },
        ]);

        let (_results, stats) = engine.execute_rules(&[rule]).unwrap();
        assert!(stats.total_steps <= 5);
    }
}
