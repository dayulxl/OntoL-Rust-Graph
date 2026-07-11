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
use crate::confidence::policy::{ConfidencePolicy, InferenceMode};
use crate::dwl2::ast::{ClassExpression, Dwl2Query, Dwl2Result, QueryType};
use crate::dwl2::query::Dwl2QueryEngine;
use crate::error::ReasonerError;
use crate::language;
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
    pub fn switch_policy_mode(&mut self, mode: InferenceMode) {
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

    /// 对指定节点执行推理。
    ///
    /// ## 流水线 (layer-by-layer)
    ///
    /// - Step 1: find entities by name
    /// - Step 2: parse expression prefixes, preload SWRL rules
    /// - Step 3: BFS layer processing
    ///   For each entity in current layer:
    ///   a. get all relationships (outgoing + incoming)
    ///   b. property inheritance (OWL2/RDFS first): parent type props -> child
    ///   c. copy to scene version (with inherited properties)
    ///   d. reason on this entity independently (behavior + SWRL)
    ///   e. discover downstream entities via inference edges -> next layer
    /// - Step 4: after all entities ready, copy relationships between copies
    /// - Step 5: SHACL validation (verify after reasoning)
    ///
    /// Core principle: **inherit before clone, ontology before relations,
    /// reason before validate**.
    /// Each entity independently goes through inherit -> copy -> reason,
    /// one layer at a time.
    pub fn reason_on_nodes(
        &mut self,
        request: ReasonOnNodesRequest,
    ) -> Result<ReasonOnNodesReport, ReasonerError> {
        use crate::graph::util;
        use crate::graph::util::find_entity_any;
        use std::collections::{HashMap, HashSet, VecDeque};

        let start_time = Instant::now();
        let mut report = ReasonOnNodesReport::new(request.cope_version.clone());

        // ═══════════════════════════════════════════════════════
        // Step 1: 按名称查找原始实体
        // ═══════════════════════════════════════════════════════
        let mut seed_codes: Vec<String> = Vec::new();
        for name in &request.node_names {
            match find_entity_any(self.repo.as_ref(), name) {
                Some(node) => {
                    let code = node
                        .property("code")
                        .and_then(|v| v.as_str())
                        .unwrap_or(name)
                        .to_string();
                    seed_codes.push(code);
                }
                None => {
                    log::warn!("节点 '{}' 未找到，跳过", name);
                }
            }
        }

        if seed_codes.is_empty() {
            return Err(ReasonerError::SwrlExecution(
                "未找到任何匹配的节点".to_string(),
            ));
        }

        log::info!(
            "reason_on_nodes: 种子实体 {} 个, 版本 '{}'",
            seed_codes.len(),
            request.cope_version
        );

        // ═══════════════════════════════════════════════════════
        // Step 2: 解析表达式前缀 + 预加载 SWRL 规则
        // ═══════════════════════════════════════════════════════
        let grouped = language::group_expressions(&request.expressions)
            .map_err(ReasonerError::SwrlExecution)?;

        let mut swrl_rules: Vec<Rule> = Vec::new();
        for rule_text in &grouped.swrl {
            let mut parser = SwrlParser::new();
            parser.set_default_prefix(&self.config.default_prefix);
            let rule = parser
                .parse(rule_text)
                .map_err(|e| ReasonerError::SwrlParse(e.to_string()))?;
            swrl_rules.push(rule);
        }

        // ═══════════════════════════════════════════════════════
        // Step 3: 逐层处理 — BFS 遍历
        // ═══════════════════════════════════════════════════════
        let mut code_map: HashMap<String, String> = HashMap::new(); // old_code → new_code
        let mut all_cloned: HashSet<String> = HashSet::new();
        let mut visited: HashSet<String> = HashSet::new();

        // BFS 队列：每层一组实体
        let mut queue: VecDeque<Vec<String>> = VecDeque::new();
        queue.push_back(seed_codes.clone());

        let mut depth = 0usize;
        let max_depth = request.max_iterations.clamp(1, 10);

        while let Some(layer_codes) = queue.pop_front() {
            if layer_codes.is_empty() || depth >= max_depth {
                depth += 1;
                continue;
            }

            log::info!(
                "reason_on_nodes: 第 {} 层 — {} 个本体待处理",
                depth,
                layer_codes.len()
            );

            let mut next_layer: Vec<String> = Vec::new();

            for code in &layer_codes {
                if visited.contains(code) {
                    continue;
                }
                visited.insert(code.clone());

                // ── 3a. 获取原实体 + 全部关系 ──
                let original_entity = match find_entity_any(self.repo.as_ref(), code) {
                    Some(n) => n,
                    None => {
                        log::warn!("实体 '{}' 未找到，跳过", code);
                        continue;
                    }
                };
                let entity_code = original_entity
                    .property("code")
                    .and_then(|v| v.as_str())
                    .unwrap_or(code)
                    .to_string();

                let all_rels = util::get_all_relationships(self.repo.as_ref(), &entity_code);

                if self.config.verbose {
                    log::info!("  本体 '{}': {} 个关系", entity_code, all_rels.len());
                }

                // ── 3b. 属性继承展开（OWL2/RDFS 优先）──
                let enriched =
                    util::inherit_entity_properties(self.repo.as_ref(), &original_entity);
                if self.config.verbose {
                    let orig_count = original_entity.properties.len();
                    let enriched_count = enriched.properties.len();
                    if enriched_count > orig_count {
                        log::info!(
                            "  本体 '{}': 继承属性 {}+{}→{}",
                            entity_code,
                            orig_count,
                            enriched_count - orig_count,
                            enriched_count
                        );
                    }
                }

                // ── 3c. 按场景 ID 复制副本本体（含继承后属性）──
                let new_code = util::ensure_cope_version_with_props(
                    self.repo.as_ref(),
                    &entity_code,
                    &request.cope_version,
                    &enriched.labels,
                    &enriched.properties,
                )
                .map_err(ReasonerError::SwrlExecution)?;

                code_map.insert(entity_code.clone(), new_code.clone());
                all_cloned.insert(new_code.clone());
                report.cloned_count += 1;

                // ── 3d. 对本实体独立推理（行为 + SWRL）──
                // 行为引擎：解析行为字段，执行 hasEffect
                let behavior = crate::swrl::behavior::parse_behavior(&original_entity);
                if !behavior.is_empty() {
                    let engine = self.swrl_engine.get_or_insert_with(|| {
                        crate::swrl::engine::SwrlEngine::new(Arc::clone(&self.repo))
                            .with_max_iterations(self.config.max_iterations)
                            .with_verbose(self.config.verbose)
                            .with_policy(self.policy.clone())
                    });

                    let result = crate::swrl::behavior::execute_behavior(
                        self.repo.as_ref(),
                        &behavior,
                        engine,
                        5,
                        0,
                    );

                    if result.triggered {
                        log::info!(
                            "  本体 '{}': 行为触发 — {} 条推导事实",
                            entity_code,
                            result.derived_count
                        );
                        report.swrl_stats.total_derived += result.derived_count;
                    } else if let Some(reason) = &result.blocked_reason {
                        log::debug!("  本体 '{}': 行为阻断 — {}", entity_code, reason);
                    }
                }

                // SWRL 规则推理（如果加载了规则）
                if !swrl_rules.is_empty() {
                    let mut versioned_engine = SwrlEngine::new(Arc::clone(&self.repo))
                        .with_max_iterations(self.config.max_iterations)
                        .with_verbose(self.config.verbose)
                        .with_policy(self.policy.clone())
                        .with_cope_version(&request.cope_version);

                    match versioned_engine.execute_rules(&swrl_rules) {
                        Ok((_results, stats)) => {
                            report.swrl_stats.total_steps += stats.total_steps;
                            report.swrl_stats.total_derived += stats.total_derived;
                            report.swrl_stats.fuse_trips += stats.fuse_trips;

                            if self.config.verbose && stats.total_derived > 0 {
                                log::info!(
                                    "  本体 '{}': SWRL 推导 {} 条事实",
                                    entity_code,
                                    stats.total_derived
                                );
                            }
                        }
                        Err(e) => {
                            log::warn!("  本体 '{}': SWRL 推理失败 — {}", entity_code, e);
                        }
                    }
                }

                // DWL2 查询（owl2: 表达式）
                for expr_body in &grouped.owl2 {
                    let expr = match ClassExpression::from_key(expr_body) {
                        Ok(e) => e,
                        Err(e) => {
                            log::warn!("DWL2 表达式解析失败 '{}': {}", expr_body, e);
                            continue;
                        }
                    };
                    let dwl2 = Dwl2QueryEngine::new(Arc::clone(&self.repo))
                        .with_cope_version(&request.cope_version);
                    match dwl2.retrieve_instances(&expr) {
                        Ok(individuals) => {
                            report.dwl2_results.push(Dwl2Result {
                                individuals,
                                subsumption_holds: None,
                                elapsed_ms: 0,
                            });
                        }
                        Err(e) => {
                            log::warn!("DWL2 查询失败: {}", e);
                        }
                    }
                }

                // ── 3e. 沿推理边发现下游本体 → 加入下一层 ──
                for rel in &all_rels {
                    // 只沿推理边发现下游（非推理边不触发本体发现）
                    if !crate::language::is_inference_relation(&rel.rel_type)
                        && !crate::language::is_ontology_relation(&rel.rel_type)
                    {
                        continue;
                    }

                    let neighbor_id = if rel.start_node_id == entity_code {
                        &rel.end_node_id
                    } else {
                        &rel.start_node_id
                    };

                    if visited.contains(neighbor_id) || all_cloned.contains(neighbor_id) {
                        continue;
                    }

                    // 检查是否是本体对象（需要处理的原实体）
                    if let Some(neighbor) = find_entity_any(self.repo.as_ref(), neighbor_id) {
                        let ver = neighbor
                            .property("cope_version")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if ver.is_empty() {
                            let ncode = neighbor
                                .property("code")
                                .and_then(|v| v.as_str())
                                .unwrap_or(neighbor_id)
                                .to_string();

                            if !visited.contains(&ncode) {
                                log::debug!(
                                    "  本体 '{}' 发现下游: '{}' (via {})",
                                    entity_code,
                                    ncode,
                                    rel.rel_type
                                );
                                next_layer.push(ncode);
                            }
                        }
                    }
                }
            }

            // 下一层入队
            if !next_layer.is_empty() && depth + 1 < max_depth {
                queue.push_back(next_layer);
            }

            depth += 1;
            report.iterations = depth;
        }

        // ═══════════════════════════════════════════════════════
        // Step 4: 本体全部就绪 → 复制副本间的关系
        // ═══════════════════════════════════════════════════════
        let mut rels_copied = 0usize;
        for (old_code, new_code) in &code_map {
            let out_rels = self
                .repo
                .get_relationships(old_code, None)
                .unwrap_or_default();
            for r in &out_rels {
                if let Some(new_tgt) = code_map.get(&r.end_node_id) {
                    let new_rel = ontology_storage::mapper::graph::relationship::Relationship {
                        rel_type: r.rel_type.clone(),
                        start_node_id: new_code.clone(),
                        end_node_id: new_tgt.clone(),
                        properties: r.properties.clone(),
                    };
                    let _ = self.repo.insert_relationship(&new_rel);
                    rels_copied += 1;
                }
            }
        }

        if self.config.verbose {
            log::info!("reason_on_nodes: 复制了 {} 条关系到副本空间", rels_copied);
        }

        // ═══════════════════════════════════════════════════════
        // Step 5: SHACL 校验（推理后验证合规性）
        // ═══════════════════════════════════════════════════════
        if !grouped.shacl.is_empty() && self.config.verbose {
            log::info!(
                "reason_on_nodes: {} SHACL 表达式已记录（文本→形状解析待完善）",
                grouped.shacl.len()
            );
            for (i, expr) in grouped.shacl.iter().enumerate() {
                log::info!("  sh[{}]: {}", i, expr);
            }
        }

        // rule: / action: / function: 扩展前缀处理（记录到日志）
        if self.config.verbose {
            if !grouped.rule.is_empty() {
                log::info!(
                    "reason_on_nodes: {} 条推理方向设定（rule:）",
                    grouped.rule.len()
                );
            }
            if !grouped.action.is_empty() {
                log::info!(
                    "reason_on_nodes: {} 条自定义动作（action:）",
                    grouped.action.len()
                );
            }
            if !grouped.function.is_empty() {
                log::info!(
                    "reason_on_nodes: {} 条自定义函数（function:）",
                    grouped.function.len()
                );
            }
        }

        report.total_ms = start_time.elapsed().as_millis() as u64;

        if self.config.verbose {
            log::info!(
                "reason_on_nodes: 完成 — {} 层, {} 个副本, {} 条关系, {} ms",
                report.iterations,
                report.cloned_count,
                rels_copied,
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
