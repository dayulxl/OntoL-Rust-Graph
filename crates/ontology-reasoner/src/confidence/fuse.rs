//! 置信度熔断器 — 推理链路截断守卫。
//!
//! 在每次推理步骤执行前后检查置信度：
//! - `before_step` — 上游置信度低于阈值 → 拒绝执行
//! - `after_step`  — 当前步骤产出置信度低于阈值 → 标记熔断
//!
//! ## 硬性规则
//!
//! 一旦置信度 < `CONFIDENCE_THRESHOLD`（默认 0.3），
//! 立即停止当前推理链路，返回 `ConfidenceFuse` 错误。

use crate::error::ReasonerError;

/// 默认熔断阈值
pub const CONFIDENCE_THRESHOLD: f64 = 0.3;

/// 熔断状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuseState {
    /// 正常 — 置信度在阈值之上
    Normal,

    /// 熔断 — 置信度低于阈值，链路已截断
    Tripped,
}

/// 置信度熔断器。
///
/// 携带当前规则名和阈值，提供步骤级的守卫方法。
/// 每个推理步骤必须调用 `guard` 进行校验 —
/// 返回值中 `FuseState::Tripped` 表示必须立即中止。
#[derive(Debug, Clone)]
pub struct ConfidenceFuse {
    /// 熔断阈值（默认 0.3）
    pub threshold: f64,

    /// 当前执行的规则名（用于错误报告）
    pub rule_name: String,

    /// 当前状态
    pub state: FuseState,

    /// 最近一次评估的置信度
    pub last_confidence: f64,
}

impl ConfidenceFuse {
    /// 创建熔断器实例
    pub fn new(rule_name: impl Into<String>, threshold: f64) -> Self {
        Self {
            threshold,
            rule_name: rule_name.into(),
            state: FuseState::Normal,
            last_confidence: 1.0,
        }
    }

    /// 使用默认阈值（0.3）创建熔断器
    pub fn with_default_threshold(rule_name: impl Into<String>) -> Self {
        Self::new(rule_name, CONFIDENCE_THRESHOLD)
    }

    /// 使用策略中的阈值创建熔断器
    pub fn with_policy(
        rule_name: impl Into<String>,
        policy: &crate::confidence::policy::ConfidencePolicy,
    ) -> Self {
        Self::new(rule_name, policy.threshold())
    }

    /// 创建已熔断的实例（用于传播熔断状态）
    pub fn tripped(rule_name: impl Into<String>, threshold: f64, last_confidence: f64) -> Self {
        Self {
            threshold,
            rule_name: rule_name.into(),
            state: FuseState::Tripped,
            last_confidence,
        }
    }

    /// 守卫方法 — 检查置信度是否在阈值之上。
    ///
    /// - 返回 `Ok(())` → 可以继续推理
    /// - 返回 `Err(ReasonerError::ConfidenceFuse)` → 必须立即中止
    ///
    /// 此方法在每次推理步骤前调用，确保上游置信度仍在安全范围。
    pub fn guard(&mut self, confidence: f64) -> Result<(), ReasonerError> {
        self.last_confidence = confidence;

        if confidence < self.threshold {
            self.state = FuseState::Tripped;
            return Err(ReasonerError::ConfidenceFuse {
                confidence,
                threshold: self.threshold,
                rule_name: self.rule_name.clone(),
            });
        }

        self.state = FuseState::Normal;
        Ok(())
    }

    /// 松弛守卫 — 仅在已熔断时检测，不更新状态。
    ///
    /// 用于推理步骤间的快速检查。
    pub fn check(&self) -> FuseState {
        if self.last_confidence < self.threshold {
            FuseState::Tripped
        } else {
            FuseState::Normal
        }
    }

    /// 是否已熔断
    pub fn is_tripped(&self) -> bool {
        self.state == FuseState::Tripped
    }

    /// 重置熔断器（用于新的推理链路）
    pub fn reset(&mut self) {
        self.state = FuseState::Normal;
        self.last_confidence = 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guard_passes_above_threshold() {
        let mut fuse = ConfidenceFuse::new("test_rule", 0.3);
        assert!(fuse.guard(0.45).is_ok());
        assert_eq!(fuse.state, FuseState::Normal);
    }

    #[test]
    fn test_guard_trips_below_threshold() {
        let mut fuse = ConfidenceFuse::new("test_rule", 0.3);
        let result = fuse.guard(0.25);
        assert!(result.is_err());
        match result {
            Err(ReasonerError::ConfidenceFuse {
                confidence,
                threshold,
                rule_name,
            }) => {
                assert!((confidence - 0.25).abs() < f64::EPSILON);
                assert!((threshold - 0.3).abs() < f64::EPSILON);
                assert_eq!(rule_name, "test_rule");
            }
            _ => panic!("Expected ConfidenceFuse error"),
        }
    }

    #[test]
    fn test_tripped_check() {
        let mut fuse = ConfidenceFuse::new("rule", 0.3);
        let _ = fuse.guard(0.5);
        assert_eq!(fuse.check(), FuseState::Normal);

        let _ = fuse.guard(0.1);
        assert_eq!(fuse.check(), FuseState::Tripped);
    }

    #[test]
    fn test_reset() {
        let mut fuse = ConfidenceFuse::new("rule", 0.3);
        let _ = fuse.guard(0.1);
        assert!(fuse.is_tripped());

        fuse.reset();
        assert!(!fuse.is_tripped());
        assert!((fuse.last_confidence - 1.0).abs() < f64::EPSILON);
    }
}
