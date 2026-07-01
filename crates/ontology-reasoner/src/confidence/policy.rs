//! 置信度策略引擎。
//!
//! 替代全局阈值 0.3，支持：
//! - 按数据来源 (source) 动态调整权重
//! - 按作战模式 (OperationMode) 切换熔断阈值
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

    /// 从字符串解析（用于 HTTP 参数）
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "sonar" | "sonar_realtime" | "sonarrealtime" => Some(SourceCategory::SonarRealtime),
            "satellite" | "sat" => Some(SourceCategory::Satellite),
            "historical" | "history" | "hist" => Some(SourceCategory::Historical),
            "unknown" | "?" => Some(SourceCategory::Unknown),
            _ => None,
        }
    }
}

/// 作战模式 — 决定熔断阈值
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationMode {
    /// 战时 — 阈值 0.15（宽松，保留更多线索）
    WarFighting,
    /// 训练 — 阈值 0.50（严格，保证数据纯净）
    Training,
    /// 演习 — 阈值 0.30（默认）
    Exercise,
}

impl OperationMode {
    /// 返回该模式的熔断阈值
    pub fn threshold(&self) -> f64 {
        match self {
            OperationMode::WarFighting => 0.15,
            OperationMode::Training => 0.50,
            OperationMode::Exercise => 0.30,
        }
    }

    /// 从字符串解析（用于 HTTP 参数 / 环境变量）
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "warfighting" | "war" | "war_fighting" => Some(OperationMode::WarFighting),
            "training" | "train" => Some(OperationMode::Training),
            "exercise" | "ex" | "default" => Some(OperationMode::Exercise),
            _ => None,
        }
    }
}

impl Default for OperationMode {
    fn default() -> Self {
        OperationMode::Exercise
    }
}

/// 置信度策略 — 外部注入引擎
#[derive(Debug, Clone)]
pub struct ConfidencePolicy {
    /// 当前作战模式
    pub mode: OperationMode,
    /// 每类数据来源的权重覆盖（None = 使用默认权重）
    source_weight_overrides: std::collections::HashMap<SourceCategory, f64>,
}

impl Default for ConfidencePolicy {
    fn default() -> Self {
        Self {
            mode: OperationMode::default(),
            source_weight_overrides: std::collections::HashMap::new(),
        }
    }
}

impl ConfidencePolicy {
    /// 创建新策略（默认 Exercise 模式）
    pub fn new() -> Self {
        Self::default()
    }

    /// 指定作战模式创建
    pub fn with_mode(mode: OperationMode) -> Self {
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

    /// 切换作战模式
    pub fn switch_mode(&mut self, mode: OperationMode) {
        self.mode = mode;
    }

    /// 覆盖某数据来源的权重
    pub fn set_source_weight(&mut self, source: SourceCategory, weight: f64) {
        self.source_weight_overrides.insert(source, weight.clamp(0.0, 1.0));
    }

    /// 重置来源权重为默认值
    pub fn reset_weights(&mut self) {
        self.source_weight_overrides.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode_thresholds() {
        assert!((OperationMode::WarFighting.threshold() - 0.15).abs() < f64::EPSILON);
        assert!((OperationMode::Training.threshold() - 0.50).abs() < f64::EPSILON);
        assert!((OperationMode::Exercise.threshold() - 0.30).abs() < f64::EPSILON);
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
        policy.switch_mode(OperationMode::WarFighting);
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
    fn test_parse_operation_mode() {
        assert_eq!(OperationMode::from_str("WarFighting"), Some(OperationMode::WarFighting));
        assert_eq!(OperationMode::from_str("training"), Some(OperationMode::Training));
        assert_eq!(OperationMode::from_str("ex"), Some(OperationMode::Exercise));
        assert_eq!(OperationMode::from_str("unknown"), None);
    }

    #[test]
    fn test_parse_source_category() {
        assert_eq!(SourceCategory::from_str("sonar"), Some(SourceCategory::SonarRealtime));
        assert_eq!(SourceCategory::from_str("sat"), Some(SourceCategory::Satellite));
        assert_eq!(SourceCategory::from_str("hist"), Some(SourceCategory::Historical));
        assert_eq!(SourceCategory::from_str("garbage"), None);
    }
}
