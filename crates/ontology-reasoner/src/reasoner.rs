//! 主推理器 — DWL2 DL + SWRL + 置信度熔断的编排层。
//!
//! `Reasoner` 是推理引擎的顶层入口，整合了：
//! - DWL2 DL 查询引擎（本体类/属性/个体检索）
//! - SWRL 规则执行引擎（逻辑推理 fixpoint 循环）
//! - 置信度熔断器（< 0.3 截断链路）

use std::sync::Arc;
use std::time::Instant;

use ontology_storage::mapper::unified_mapping;
use ontology_storage::repository::graph_store::SharedRepository;

use crate::confidence::fuse::CONFIDENCE_THRESHOLD;
use crate::confidence::policy::{ConfidencePolicy, OperationMode};
use crate::dwl2::ast::{ClassExpression, Dwl2Query, Dwl2Result, QueryType};
use crate::dwl2::query::Dwl2QueryEngine;
use crate::error::ReasonerError;
use crate::language;
use crate::swrl::ast::Atom;
use crate::swrl::ast::{ExecutionStats, InferenceResult, Rule};
use crate::swrl::engine::SwrlEngine;
use crate::swrl::parser::SwrlParser;

// ═══════════════════════════════════════════════════════════
// Reasoner 配置
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct ReasonerConfig {
    pub max_iterations: usize,
    pub fuse_threshold: f64,
    pub verbose: bool,
    pub default_prefix: String,
    pub policy: ConfidencePolicy,
}

impl Default for ReasonerConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            fuse_threshold: CONFIDENCE_THRESHOLD,
            verbose: false,
            default_prefix: "http://example.org#".to_string(),
            policy: ConfidencePolicy::default(),
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Reasoner 构建器
// ═══════════════════════════════════════════════════════════

pub struct ReasonerBuilder {
    repo: SharedRepository,
    config: ReasonerConfig,
}

impl ReasonerBuilder {
    pub fn new(repo: SharedRepository) -> Self {
        Self {
            repo,
            config: ReasonerConfig::default(),
        }
    }

    pub fn with_max_iterations(mut self, n: usize) -> Self {
        self.config.max_iterations = n;
        self
    }

    pub fn with_fuse_threshold(mut self, t: f64) -> Self {
        self.config.fuse_threshold = t;
        self
    }

    pub fn with_verbose(mut self, v: bool) -> Self {
        self.config.verbose = v;
        self
    }

    pub fn with_default_prefix(mut self, prefix: &str) -> Self {
        self.config.default_prefix = prefix.to_string();
        self
    }

    pub fn with_policy(mut self, policy: ConfidencePolicy) -> Self {
        self.config.policy = policy;
        self
    }

    pub fn build(self) -> Reasoner {
        let policy = self.config.policy.clone();
        Reasoner {
            repo: self.repo,
            config: self.config,
            rules: Vec::new(),
            swrl_engine: None,
            policy,
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Reasoner
// ═══════════════════════════════════════════════════════════

/// 推理器 — 图数据库推理引擎的核心编排器。
pub struct Reasoner {
    repo: SharedRepository,
    config: ReasonerConfig,
    rules: Vec<Rule>,
    swrl_engine: Option<SwrlEngine>,
    policy: ConfidencePolicy,
}

impl Reasoner {
    // ═══════════════════════════════════════════════════════
    // 规则加载
    // ═══════════════════════════════════════════════════════

    pub fn load_swrl_rule(&mut self, rule_text: &str) -> Result<&mut Self, ReasonerError> {
        let mut parser = SwrlParser::new();
        parser.set_default_prefix(&self.config.default_prefix);
        let rule = parser.parse(rule_text)?;
        self.rules.push(rule);
        Ok(self)
    }

    pub fn load_swrl_rules(&mut self, source: &str) -> Result<&mut Self, ReasonerError> {
        let mut parser = SwrlParser::new();
        parser.set_default_prefix(&self.config.default_prefix);
        let rules = parser.parse_all(source)?;
        self.rules.extend(rules);
        Ok(self)
    }

    pub fn add_rules(&mut self, rules: Vec<Rule>) -> &mut Self {
        self.rules.extend(rules);
        self
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    // ═══════════════════════════════════════════════════════
    // 推理执行
    // ═══════════════════════════════════════════════════════

    /// 执行全量推理，返回推理报告。
    pub fn reason(&mut self) -> Result<ReasonerReport, ReasonerError> {
        let start = Instant::now();

        if self.rules.is_empty() {
            return Ok(ReasonerReport {
                rules_loaded: 0,
                results: vec![],
                stats: ExecutionStats::default(),
                total_ms: 0,
            });
        }

        let rules = self.rules.clone();

        if self.swrl_engine.is_none() {
            self.swrl_engine = Some(
                SwrlEngine::new(Arc::clone(&self.repo))
                    .with_max_iterations(self.config.max_iterations)
                    .with_verbose(self.config.verbose)
                    .with_policy(self.policy.clone()),
            );
        }

        let engine = self.swrl_engine.as_mut().unwrap();
        let (results, stats) = engine.execute_rules(&rules)?;

        let total_ms = start.elapsed().as_millis() as u64;

        if self.config.verbose {
            log::info!(
                "Reasoning: {} rules, {} steps, {} derived, {} ms",
                rules.len(),
                stats.total_steps,
                stats.total_derived,
                total_ms,
            );
        }

        Ok(ReasonerReport {
            rules_loaded: rules.len(),
            results,
            stats,
            total_ms,
        })
    }

    /// 增量推理：重置引擎并重新执行
    pub fn reason_incremental(&mut self) -> Result<ReasonerReport, ReasonerError> {
        self.swrl_engine = Some(
            SwrlEngine::new(Arc::clone(&self.repo))
                .with_max_iterations(self.config.max_iterations)
                .with_verbose(self.config.verbose)
                .with_policy(self.policy.clone()),
        );
        self.reason()
    }

    // ═══════════════════════════════════════════════════════
    // DWL2 DL 查询
    // ═══════════════════════════════════════════════════════

    /// 检索满足类表达式的所有实例
    pub fn query_instances(
        &self,
        expression: ClassExpression,
    ) -> Result<Dwl2Result, ReasonerError> {
        let engine = Dwl2QueryEngine::new(Arc::clone(&self.repo));
        engine.execute(&Dwl2Query {
            expression,
            query_type: QueryType::RetrieveInstances,
        })
    }

    /// 检查包含关系：sub_class ⊑ super_class
    pub fn check_subsumption(
        &self,
        sub_class: &str,
        super_class: &ClassExpression,
    ) -> Result<bool, ReasonerError> {
        let engine = Dwl2QueryEngine::new(Arc::clone(&self.repo));
        let result = engine.execute(&Dwl2Query {
            expression: super_class.clone(),
            query_type: QueryType::IsSubClassOf {
                sub_class: sub_class.to_string(),
                super_class: super_class.clone(),
            },
        })?;
        Ok(result.subsumption_holds.unwrap_or(false))
    }

    /// 检查个体归属
    pub fn check_instance_of(
        &self,
        individual_iri: &str,
        expression: ClassExpression,
    ) -> Result<bool, ReasonerError> {
        let engine = Dwl2QueryEngine::new(Arc::clone(&self.repo));
        let result = engine.execute(&Dwl2Query {
            expression,
            query_type: QueryType::IsInstanceOf {
                individual_iri: individual_iri.to_string(),
            },
        })?;
        Ok(result.subsumption_holds.unwrap_or(false))
    }

    pub fn repo(&self) -> &SharedRepository {
        &self.repo
    }
    pub fn config(&self) -> &ReasonerConfig {
        &self.config
    }
    pub fn policy(&self) -> &ConfidencePolicy {
        &self.policy
    }
    pub fn switch_policy_mode(&mut self, mode: OperationMode) {
        self.policy.switch_mode(mode);
        // 同步到已初始化的引擎
        if let Some(ref mut engine) = self.swrl_engine {
            engine.update_policy(self.policy.clone());
        }
    }

    /// 执行图中所有 Entity 的行为动作。
    ///
    /// 扫描 Entity 节点，解析 precondition/effect/cost/duration/priority/composedOf
    /// 六个行为字段，按优先级执行。precondition 满足时触发 effect，composedOf 递归。
    ///
    /// # 参数
    ///
    /// - `max_depth` — composedOf 递归最大深度（0 表示不递归）
    pub fn execute_behaviors(
        &mut self,
        max_depth: usize,
    ) -> Result<Vec<crate::swrl::behavior::BehaviorResult>, ReasonerError> {
        use crate::swrl::behavior;

        let entities = self
            .repo
            .get_nodes_by_label(unified_mapping::ENTITY_LABEL)
            .map_err(|e| ReasonerError::SwrlExecution(format!("查询 Entity 失败: {}", e)))?;

        if entities.is_empty() {
            return Ok(Vec::new());
        }

        let engine = self.swrl_engine.get_or_insert_with(|| {
            crate::swrl::engine::SwrlEngine::new(Arc::clone(&self.repo))
                .with_max_iterations(self.config.max_iterations)
                .with_verbose(self.config.verbose)
                .with_policy(self.policy.clone())
        });

        Ok(behavior::execute_behaviors_batch(
            self.repo.as_ref(),
            &entities,
            engine,
            max_depth,
        ))
    }

    // ═══════════════════════════════════════════════════════
    // 按节点推理 — 选择性克隆 + 语言前缀路由 + 迭代循环
    // ═══════════════════════════════════════════════════════

    /// 对指定节点执行推理：选择性克隆副本 → 按语言前缀路由引擎 → 迭代推演。
    ///
    /// # 流程
    ///
    /// 1. 按名称/ID 查找每个节点
    /// 2. 选择性克隆指定节点 + 关联本体对象到 cope_version 副本空间
    /// 3. 解析表达式前缀 (`owl2:`/`swrl:`/`sh:`)，分组路由到对应引擎
    /// 4. 迭代循环（最多 max_iterations 次）：
    ///    a. DWL2 查询发现新关联本体 → 按需克隆
    ///    b. SWRL 规则推理 → 在副本图上推导新事实
    ///    c. SHACL 约束验证 → 检查副本图合规性
    ///    d. 收敛检查：本轮无新节点克隆 + 无新推理事实 → 退出
    pub fn reason_on_nodes(
        &mut self,
        request: ReasonOnNodesRequest,
    ) -> Result<ReasonOnNodesReport, ReasonerError> {
        use crate::graph::util;
        use crate::graph::util::find_entity_any;
        use std::collections::HashSet;

        let start_time = Instant::now();
        let mut report = ReasonOnNodesReport::new(request.cope_version.clone());

        // ── Step 1: 按名称查找节点 ──
        let mut original_codes: Vec<String> = Vec::new();
        for name in &request.node_names {
            match find_entity_any(self.repo.as_ref(), name) {
                Some(node) => {
                    let code = node
                        .property("code")
                        .and_then(|v| v.as_str())
                        .unwrap_or(name)
                        .to_string();
                    original_codes.push(code);
                }
                None => {
                    log::warn!("节点 '{}' 未找到，跳过", name);
                }
            }
        }

        if original_codes.is_empty() {
            return Err(ReasonerError::SwrlExecution(
                "未找到任何匹配的节点".to_string(),
            ));
        }

        // ── Step 2: 首次选择性克隆（用户指定的节点 + 关联本体） ──
        let code_map =
            util::clone_nodes_selective(self.repo.as_ref(), &original_codes, &request.cope_version)
                .map_err(ReasonerError::SwrlExecution)?;

        report.cloned_count = code_map.len();
        let mut all_cloned: HashSet<String> = code_map.values().cloned().collect();

        if self.config.verbose {
            log::info!(
                "reason_on_nodes: 克隆了 {} 个节点到版本 '{}'",
                code_map.len(),
                request.cope_version
            );
        }

        // ── Step 3: 解析表达式前缀 ──
        let (owl2_exprs, swrl_exprs, shacl_exprs) =
            language::group_expressions(&request.expressions)
                .map_err(ReasonerError::SwrlExecution)?;

        // 预加载 SWRL 规则
        let mut swrl_rules: Vec<Rule> = Vec::new();
        for rule_text in &swrl_exprs {
            let mut parser = SwrlParser::new();
            parser.set_default_prefix(&self.config.default_prefix);
            let rule = parser
                .parse(rule_text)
                .map_err(|e| ReasonerError::SwrlParse(e.to_string()))?;
            swrl_rules.push(rule);
        }

        // ── Step 4: 迭代循环 ──
        for iteration in 1..=request.max_iterations {
            let mut new_clones_this_round = 0usize;
            let mut new_facts_this_round = 0usize;

            // 4a. DWL2 查询阶段 — 发现新关联本体
            for expr_body in &owl2_exprs {
                let expr =
                    ClassExpression::from_key(expr_body).map_err(ReasonerError::Dwl2Parse)?;

                let dwl2 = Dwl2QueryEngine::new(Arc::clone(&self.repo))
                    .with_cope_version(&request.cope_version);

                let result = dwl2.retrieve_instances(&expr)?;
                report.dwl2_results.push(Dwl2Result {
                    individuals: result,
                    subsumption_holds: None,
                    elapsed_ms: 0,
                });

                // 检查发现的个体是否需要克隆
                for iri in &report
                    .dwl2_results
                    .last()
                    .map(|r| r.individuals.clone())
                    .unwrap_or_default()
                {
                    if all_cloned.contains(iri) {
                        continue;
                    }
                    // 检查是否是本体对象（需要克隆的原实体）
                    if let Some(orig) = find_entity_any(self.repo.as_ref(), iri) {
                        let ver = orig
                            .property("cope_version")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if ver.is_empty() {
                            // 新发现的原实体 → 克隆
                            let delta_codes = vec![iri.clone()];
                            let delta_map = util::clone_nodes_selective(
                                self.repo.as_ref(),
                                &delta_codes,
                                &request.cope_version,
                            )
                            .map_err(ReasonerError::SwrlExecution)?;

                            new_clones_this_round += delta_map.len();
                            for v in delta_map.values() {
                                all_cloned.insert(v.clone());
                            }
                        }
                    }
                }
            }

            // 4b. SWRL 推理阶段 — 在副本图上推导
            if !swrl_rules.is_empty() {
                let mut versioned_engine = SwrlEngine::new(Arc::clone(&self.repo))
                    .with_max_iterations(self.config.max_iterations)
                    .with_verbose(self.config.verbose)
                    .with_policy(self.policy.clone())
                    .with_cope_version(&request.cope_version);

                let (results, stats) = versioned_engine.execute_rules(&swrl_rules)?;
                new_facts_this_round += stats.total_derived;
                report.swrl_stats = stats;

                // 检查推导事实中引用的新节点是否需要克隆
                for result in &results {
                    for fact in &result.derived_facts {
                        if let Atom::ObjectPropertyAtom {
                            subject, object, ..
                        } = fact
                        {
                            for iri in &[subject.clone(), object.clone()] {
                                if all_cloned.contains(iri) || iri.starts_with("http://") {
                                    continue;
                                }
                                if let Some(orig) = find_entity_any(self.repo.as_ref(), iri) {
                                    let ver = orig
                                        .property("cope_version")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    if ver.is_empty() {
                                        let delta_codes = vec![iri.clone()];
                                        let delta_map = util::clone_nodes_selective(
                                            self.repo.as_ref(),
                                            &delta_codes,
                                            &request.cope_version,
                                        )
                                        .map_err(ReasonerError::SwrlExecution)?;
                                        new_clones_this_round += delta_map.len();
                                        for v in delta_map.values() {
                                            all_cloned.insert(v.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // 更新内部引擎状态以保持 derived_facts 累积
                // 注：当前简化实现每次迭代重建引擎
                if let Some(ref mut eng) = self.swrl_engine {
                    *eng = versioned_engine;
                } else {
                    self.swrl_engine = Some(versioned_engine);
                }
            }

            // 4c. SHACL 验证阶段 — 检查副本图合规性
            if !shacl_exprs.is_empty() {
                // SHACL 表达式从文本解析形状图的功能留待后续迭代完善。
                // 当前 sh: 表达式记录到日志，验证引擎需要在 SHACL 模块中
                // 扩展文本解析能力（类似 SWRL 的 SwrlParser）。
                if self.config.verbose {
                    log::info!(
                        "reason_on_nodes: {} SHACL 表达式已记录（文本→形状解析待完善）",
                        shacl_exprs.len()
                    );
                    for (i, expr) in shacl_exprs.iter().enumerate() {
                        log::info!("  sh[{}]: {}", i, expr);
                    }
                }
            }

            report.iterations = iteration;

            // 4d. 收敛检查
            if new_clones_this_round == 0 && new_facts_this_round == 0 {
                if self.config.verbose {
                    log::info!("reason_on_nodes: 在第 {} 次迭代收敛", iteration);
                }
                break;
            }

            if self.config.verbose {
                log::info!(
                    "reason_on_nodes: 迭代 {} — +{} 新克隆, +{} 新事实",
                    iteration,
                    new_clones_this_round,
                    new_facts_this_round
                );
            }

            report.cloned_count += new_clones_this_round;
        }

        report.total_ms = start_time.elapsed().as_millis() as u64;

        if self.config.verbose {
            log::info!(
                "reason_on_nodes: 完成 — {} 次迭代, {} 个副本, {} ms",
                report.iterations,
                report.cloned_count,
                report.total_ms
            );
        }

        Ok(report)
    }
} // impl Reasoner

// ═══════════════════════════════════════════════════════════
// 按节点推理 — 请求/报告类型
// ═══════════════════════════════════════════════════════════

/// `reason_on_nodes` 的输入参数。
#[derive(Debug, Clone)]
pub struct ReasonOnNodesRequest {
    /// 要推理的节点名称/ID 列表
    pub node_names: Vec<String>,
    /// 带语言前缀的表达式列表（如 `"swrl:hasEnemy(?x,?y)->alert(?x,?y)"`）
    pub expressions: Vec<String>,
    /// 副本版本标识（如 `"v1"`、`"scenario_alpha"`）
    pub cope_version: String,
    /// 最大迭代次数（默认 5）
    pub max_iterations: usize,
}

impl Default for ReasonOnNodesRequest {
    fn default() -> Self {
        Self {
            node_names: Vec::new(),
            expressions: Vec::new(),
            cope_version: "default".to_string(),
            max_iterations: 5,
        }
    }
}

/// `reason_on_nodes` 的返回报告。
#[derive(Debug, Clone)]
pub struct ReasonOnNodesReport {
    /// 使用的副本版本号
    pub cope_version: String,
    /// 累计克隆的节点数
    pub cloned_count: usize,
    /// DWL2 查询结果
    pub dwl2_results: Vec<Dwl2Result>,
    /// SWRL 推理统计
    pub swrl_stats: ExecutionStats,
    /// SHACL 验证报告
    pub shacl_reports: Vec<crate::shacl::result::ValidationReport>,
    /// 实际迭代次数
    pub iterations: usize,
    /// 总耗时(ms)
    pub total_ms: u64,
}

impl ReasonOnNodesReport {
    pub fn new(cope_version: String) -> Self {
        Self {
            cope_version,
            cloned_count: 0,
            dwl2_results: Vec::new(),
            swrl_stats: ExecutionStats::default(),
            shacl_reports: Vec::new(),
            iterations: 0,
            total_ms: 0,
        }
    }
}

// ═══════════════════════════════════════════════════════════
// 推理报告
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct ReasonerReport {
    pub rules_loaded: usize,
    pub results: Vec<InferenceResult>,
    pub stats: ExecutionStats,
    pub total_ms: u64,
}

impl std::fmt::Display for ReasonerReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Reasoning Report ===")?;
        writeln!(f, "Rules loaded:  {}", self.rules_loaded)?;
        writeln!(
            f,
            "Total steps:   {} (fixpoint iterations)",
            self.stats.total_steps
        )?;
        writeln!(f, "Derived facts: {}", self.stats.total_derived)?;
        writeln!(f, "Fuse trips:    {}", self.stats.fuse_trips)?;
        writeln!(f, "Time:          {} ms", self.total_ms)?;

        let low_conf: Vec<_> = self.results.iter().filter(|r| r.confidence < 0.5).collect();
        if !low_conf.is_empty() {
            writeln!(f, "\n⚠ Low-confidence inferences (< 0.5):")?;
            for r in low_conf {
                writeln!(
                    f,
                    "  - Rule '{}': confidence {:.4}, {} binding(s)",
                    r.rule_name, r.confidence, r.binding_count
                )?;
            }
        }
        Ok(())
    }
}
