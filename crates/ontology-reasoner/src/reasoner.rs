//! 主推理器 — DWL2 DL + SWRL + 置信度熔断的编排层。
//!
//! `Reasoner` 是推理引擎的顶层入口，整合了：
//! - DWL2 DL 查询引擎（本体类/属性/个体检索）
//! - SWRL 规则执行引擎（逻辑推理 fixpoint 循环）
//! - 置信度熔断器（< 0.3 截断链路）

use std::sync::Arc;
use std::time::Instant;

use ontology_storage::repository::graph_store::SharedRepository;

use crate::confidence::fuse::CONFIDENCE_THRESHOLD;
use crate::confidence::policy::{ConfidencePolicy, OperationMode};
use crate::dwl2::ast::{ClassExpression, Dwl2Query, Dwl2Result, QueryType};
use crate::dwl2::query::Dwl2QueryEngine;
use crate::error::ReasonerError;
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
        Self { repo, config: ReasonerConfig::default() }
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

    pub fn rules(&self) -> &[Rule] { &self.rules }
    pub fn rule_count(&self) -> usize { self.rules.len() }

    // ═══════════════════════════════════════════════════════
    // 推理执行
    // ═══════════════════════════════════════════════════════

    /// 执行全量推理，返回推理报告。
    pub fn reason(&mut self) -> Result<ReasonerReport, ReasonerError> {
        let start = Instant::now();

        if self.rules.is_empty() {
            return Ok(ReasonerReport {
                rules_loaded: 0, results: vec![],
                stats: ExecutionStats::default(), total_ms: 0,
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
                rules.len(), stats.total_steps, stats.total_derived, total_ms,
            );
        }

        Ok(ReasonerReport {
            rules_loaded: rules.len(), results, stats, total_ms,
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
    pub fn query_instances(&self, expression: ClassExpression) -> Result<Dwl2Result, ReasonerError> {
        let engine = Dwl2QueryEngine::new(Arc::clone(&self.repo));
        engine.execute(&Dwl2Query {
            expression,
            query_type: QueryType::RetrieveInstances,
        })
    }

    /// 检查包含关系：sub_class ⊑ super_class
    pub fn check_subsumption(
        &self, sub_class: &str, super_class: &ClassExpression,
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
        &self, individual_iri: &str, expression: ClassExpression,
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

    pub fn repo(&self) -> &SharedRepository { &self.repo }
    pub fn config(&self) -> &ReasonerConfig { &self.config }
    pub fn policy(&self) -> &ConfidencePolicy { &self.policy }
    pub fn switch_policy_mode(&mut self, mode: OperationMode) {
        self.policy.switch_mode(mode);
        // 同步到已初始化的引擎
        if let Some(ref mut engine) = self.swrl_engine {
            engine.update_policy(self.policy.clone());
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
        writeln!(f, "Total steps:   {} (fixpoint iterations)", self.stats.total_steps)?;
        writeln!(f, "Derived facts: {}", self.stats.total_derived)?;
        writeln!(f, "Fuse trips:    {}", self.stats.fuse_trips)?;
        writeln!(f, "Time:          {} ms", self.total_ms)?;

        let low_conf: Vec<_> = self.results.iter().filter(|r| r.confidence < 0.5).collect();
        if !low_conf.is_empty() {
            writeln!(f, "\n⚠ Low-confidence inferences (< 0.5):")?;
            for r in low_conf {
                writeln!(f, "  - Rule '{}': confidence {:.4}, {} binding(s)",
                    r.rule_name, r.confidence, r.binding_count)?;
            }
        }
        Ok(())
    }
}
