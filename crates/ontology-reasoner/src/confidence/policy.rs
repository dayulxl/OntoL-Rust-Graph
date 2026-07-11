//! 置信度策略引擎。
//!
//! 替代全局阈值 0.3，支持：
//! - 按数据来源 (source) 动态调整权重
//! - 按推理模式 (InferenceMode) 切换熔断阈值
//! - Policy 外部注入，而非引擎内部硬编码

/// 数据来源类别 — 决定置信度权重
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceCategory {
    /// 声纳浮标实时数据 — 权重最高 (0.45)
    SonarRealtime,
    /// 卫星影像 — 中等权重 (0.30)
    Satellite,
    /// 历史情报 — 最低权重 (0.25)
    Historical,
    /// 未标注来源 — 默认权重 (0.20)
    Unknown,
}

impl SourceCategory {
    /// 默认权重
    pub fn default_weight(&self) -> f64 {
        match self {
            SourceCategory::SonarRealtime => 0.45,
            SourceCategory::Satellite => 0.30,
            SourceCategory::Historical => 0.25,
            SourceCategory::Unknown => 0.20,
        }
    }
}

impl std::str::FromStr for SourceCategory {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sonar" | "sonar_realtime" | "sonarrealtime" => Ok(SourceCategory::SonarRealtime),
            "satellite" | "sat" => Ok(SourceCategory::Satellite),
            "historical" | "history" | "hist" => Ok(SourceCategory::Historical),
            "unknown" | "?" => Ok(SourceCategory::Unknown),
            _ => Err(format!("未知来源类别: {}", s)),
        }
    }
}

/// 推理模式 — 决定熔断阈值
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InferenceMode {
    /// 宽松 — 阈值 0.15（低门槛，保留更多线索）
    Permissive,
    /// 均衡 — 阈值 0.30（默认）
    #[default]
    Balanced,
    /// 严格 — 阈值 0.50（高标准，保证数据质量）
    Strict,
}

impl InferenceMode {
    /// 返回该模式的熔断阈值
    pub fn threshold(&self) -> f64 {
        match self {
            InferenceMode::Permissive => 0.15,
            InferenceMode::Balanced => 0.30,
            InferenceMode::Strict => 0.50,
        }
    }
}

impl std::str::FromStr for InferenceMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            // 新名
            "permissive" | "perm" | "loose" => Ok(InferenceMode::Permissive),
            "balanced" | "bal" | "standard" | "default" => Ok(InferenceMode::Balanced),
            "strict" | "precision" | "precise" => Ok(InferenceMode::Strict),
            // 旧名兼容
            "warfighting" | "war" | "war_fighting" => Ok(InferenceMode::Permissive),
            "exercise" | "ex" => Ok(InferenceMode::Balanced),
            "training" | "train" => Ok(InferenceMode::Strict),
            _ => Err(format!("未知推理模式: {}", s)),
        }
    }
}

/// 置信度策略 — 外部注入引擎
#[derive(Debug, Clone, Default)]
pub struct ConfidencePolicy {
    /// 当前推理模式
    pub mode: InferenceMode,
    /// 每类数据来源的权重覆盖（None = 使用默认权重）
    source_weight_overrides: std::collections::HashMap<SourceCategory, f64>,
}

impl ConfidencePolicy {
    /// 创建新策略（默认 Balanced 模式）
    pub fn new() -> Self {
        Self::default()
    }

    /// 指定推理模式创建
    pub fn with_mode(mode: InferenceMode) -> Self {
        Self {
            mode,
            source_weight_overrides: std::collections::HashMap::new(),
        }
    }

    /// 获取当前熔断阈值
    pub fn threshold(&self) -> f64 {
        self.mode.threshold()
    }

    /// 获取指定数据来源的置信度权重
    pub fn source_weight(&self, source: SourceCategory) -> f64 {
        self.source_weight_overrides
            .get(&source)
            .copied()
            .unwrap_or_else(|| source.default_weight())
    }

    /// 切换推理模式
    pub fn switch_mode(&mut self, mode: InferenceMode) {
        self.mode = mode;
    }

    /// 覆盖某数据来源的权重
    pub fn set_source_weight(&mut self, source: SourceCategory, weight: f64) {
        self.source_weight_overrides
            .insert(source, weight.clamp(0.0, 1.0));
    }

    /// 重置来源权重为默认值
    pub fn reset_weights(&mut self) {
        self.source_weight_overrides.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_mode_thresholds() {
        assert!((InferenceMode::Permissive.threshold() - 0.15).abs() < f64::EPSILON);
        assert!((InferenceMode::Balanced.threshold() - 0.30).abs() < f64::EPSILON);
        assert!((InferenceMode::Strict.threshold() - 0.50).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_weights() {
        assert!((SourceCategory::SonarRealtime.default_weight() - 0.45).abs() < f64::EPSILON);
        assert!((SourceCategory::Satellite.default_weight() - 0.30).abs() < f64::EPSILON);
    }

    #[test]
    fn test_policy_default_threshold() {
        let policy = ConfidencePolicy::default();
        assert!((policy.threshold() - 0.30).abs() < f64::EPSILON);
    }

    #[test]
    fn test_policy_switch_mode() {
        let mut policy = ConfidencePolicy::default();
        policy.switch_mode(InferenceMode::Permissive);
        assert!((policy.threshold() - 0.15).abs() < f64::EPSILON);
    }

    #[test]
    fn test_source_weight_override() {
        let mut policy = ConfidencePolicy::default();
        policy.set_source_weight(SourceCategory::SonarRealtime, 0.80);
        assert!((policy.source_weight(SourceCategory::SonarRealtime) - 0.80).abs() < f64::EPSILON);
        // unset sources still use default
        assert!((policy.source_weight(SourceCategory::Satellite) - 0.30).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_inference_mode() {
        // 新名
        assert_eq!(
            InferenceMode::from_str("Permissive").ok(),
            Some(InferenceMode::Permissive)
        );
        assert_eq!(
            InferenceMode::from_str("balanced").ok(),
            Some(InferenceMode::Balanced)
        );
        assert_eq!(
            InferenceMode::from_str("strict").ok(),
            Some(InferenceMode::Strict)
        );
        // 旧名兼容
        assert_eq!(
            InferenceMode::from_str("WarFighting").ok(),
            Some(InferenceMode::Permissive)
        );
        assert_eq!(
            InferenceMode::from_str("exercise").ok(),
            Some(InferenceMode::Balanced)
        );
        assert_eq!(
            InferenceMode::from_str("training").ok(),
            Some(InferenceMode::Strict)
        );
        assert!(InferenceMode::from_str("unknown").is_err());
    }

    #[test]
    fn test_parse_source_category() {
        assert_eq!(
            SourceCategory::from_str("sonar").ok(),
            Some(SourceCategory::SonarRealtime)
        );
        assert_eq!(
            SourceCategory::from_str("sat").ok(),
            Some(SourceCategory::Satellite)
        );
        assert_eq!(
            SourceCategory::from_str("hist").ok(),
            Some(SourceCategory::Historical)
        );
        assert!(SourceCategory::from_str("garbage").is_err());
    }
}
