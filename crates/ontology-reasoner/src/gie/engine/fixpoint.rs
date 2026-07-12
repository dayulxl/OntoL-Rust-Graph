//! # Fixpoint 不动点迭代循环
//!
//! `InferenceEngine` 内部的核心迭代调度器，实现不动点推理：
//!
//! ```text
//! while 有新事实产生 && 迭代次数 < max_iterations:
//!     1. 检查置信度 — 低于阈值则熔断当前链路
//!     2. 执行全部 QueryPlan
//!     3. 收集新推导事实
//!     4. 去重（id + 事实描述为键）
//!     5. 写入图（串行，避免并发冲突）
//! ```
//!
//! ## 架构约束
//!
//! - Fixpoint 是 [`InferenceEngine`] 的内部组件 — 不独立暴露
//! - 规则匹配阶段只读 repo（线程安全），事实写入阶段串行执行
//! - 置信度低于阈值时熔断当前规则链路，不终止整个 fixpoint
//! - 参见 ARCHITECTURE.md §19（宽容执行）和 CRATE_CONSTRAINTS.md §6.2

use std::collections::HashSet;

use crate::confidence::policy::ConfidencePolicy;
use crate::error::ReasonerError;
use crate::QueryPlan;

use crate::gie::context::InferenceContext;

/// Fixpoint 迭代调度状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixpointState {
    /// 正在运行中。
    Running,
    /// 收敛 — 无新事实产生。
    Converged,
    /// 达到最大迭代次数上限。
    MaxIterationsReached,
    /// 被置信度熔断打断。
    FuseTripped,
}

/// Fixpoint 不动点迭代调度器。
///
/// 控制推理的迭代节奏，决定何时继续、何时停止。
///
/// 当前为骨架阶段 — 字段在后续迭代中由 `run()` 使用。
///
/// # 示例
///
/// ```rust,ignore
/// let mut fixpoint = FixpointLoop::new(100, 0.3);
/// fixpoint.run(ctx, plans, |plan| repo.execute_plan(plan))?;
/// ```
#[allow(dead_code)]
pub struct FixpointLoop {
    /// 最大迭代次数。
    max_iterations: usize,
    /// 置信度熔断阈值。
    fuse_threshold: f64,
    /// 当前状态。
    state: FixpointState,
    /// 已推导事实的去重集合（id + 事实描述）。
    derived_facts: HashSet<String>,
}

impl FixpointLoop {
    /// 创建新的 fixpoint 调度器。
    pub fn new(max_iterations: usize, fuse_threshold: f64) -> Self {
        Self {
            max_iterations,
            fuse_threshold,
            state: FixpointState::Running,
            derived_facts: HashSet::new(),
        }
    }

    /// 使用置信度策略创建。
    pub fn with_policy(max_iterations: usize, policy: &ConfidencePolicy) -> Self {
        Self::new(max_iterations, policy.threshold())
    }

    /// 返回当前状态。
    pub fn state(&self) -> &FixpointState {
        &self.state
    }

    /// 是否已完成（收敛 / 超限 / 熔断）。
    pub fn is_finished(&self) -> bool {
        matches!(
            self.state,
            FixpointState::Converged
                | FixpointState::MaxIterationsReached
                | FixpointState::FuseTripped
        )
    }

    /// 去重后的已推导事实数量。
    pub fn derived_count(&self) -> usize {
        self.derived_facts.len()
    }

    /// 检查是否为新事实（未在去重集合中）。
    pub fn is_novel(&self, fact_key: &str) -> bool {
        !self.derived_facts.contains(fact_key)
    }

    /// 记录新事实到去重集合。
    pub fn record_fact(&mut self, fact_key: String) {
        self.derived_facts.insert(fact_key);
    }

    /// 运行 fixpoint 迭代循环。
    ///
    /// 在后续迭代中实现具体的迭代逻辑。当前为骨架。
    ///
    /// # 参数
    ///
    /// - `ctx` — 推理上下文（传递 cope_version、置信度等）
    /// - `plans` — 要执行的 QueryPlan 列表
    #[allow(unused_variables)]
    pub fn run(
        &mut self,
        ctx: &mut InferenceContext,
        plans: &[QueryPlan],
    ) -> Result<FixpointState, ReasonerError> {
        // 骨架阶段 — 具体实现在后续迭代中添加
        self.state = FixpointState::Converged;
        Ok(self.state.clone())
    }

    /// 重置调度器状态（用于新一轮推理）。
    pub fn reset(&mut self) {
        self.state = FixpointState::Running;
        self.derived_facts.clear();
    }
}

impl Default for FixpointLoop {
    fn default() -> Self {
        Self::new(100, 0.3)
    }
}
