//! 置信度计算器 — 多源加权置信度评估。
//!
//! ## 置信度来源与权重
//!
//! 置信度 = Σ(source_i × weight_i) / Σ(weight_i)
//!
//! | 来源              | 默认权重 | 说明                                    |
//! |-------------------|----------|-----------------------------------------|
//! | `source_match`    | 0.35     | SWRL 前提匹配度（绑定覆盖/变量数）      |
//! | `source_cardinal` | 0.20     | 基数约束满足度                          |
//! | `source_property` | 0.25     | 属性值一致性（值比较结果）              |
//! | `source_structural`| 0.20    | 结构匹配度（图模式匹配的边/节点比）     |
//!
//! ## 使用示例
//!
//! ```rust,ignore
//! let calc = ConfidenceCalculator::default();
//! let conf = calc.evaluate(&conf_input);
//! assert!(conf >= 0.0 && conf <= 1.0);
//! ```

/// 置信度输入 — 推理步骤中的各维度度量值
#[derive(Debug, Clone)]
pub struct ConfidenceInput {
    /// 前提匹配度 [0, 1]：已绑定的原子数 / 总前提原子数
    pub source_match: f64,

    /// 基数满足度 [0, 1]：满足基数约束的关系数 / 期望基数
    pub source_cardinal: f64,

    /// 属性值一致性 [0, 1]：属性匹配数 / 总属性条件数
    pub source_property: f64,

    /// 结构匹配度 [0, 1]：图模式匹配成功的路径数 / 预期路径数
    pub source_structural: f64,
}

impl Default for ConfidenceInput {
    fn default() -> Self {
        Self {
            source_match: 1.0,
            source_cardinal: 1.0,
            source_property: 1.0,
            source_structural: 1.0,
        }
    }
}

/// 置信度权重配置
#[derive(Debug, Clone)]
pub struct ConfidenceWeights {
    pub match_weight: f64,
    pub cardinal_weight: f64,
    pub property_weight: f64,
    pub structural_weight: f64,
}

impl Default for ConfidenceWeights {
    fn default() -> Self {
        Self {
            match_weight: 0.35,
            cardinal_weight: 0.20,
            property_weight: 0.25,
            structural_weight: 0.20,
        }
    }
}

/// 置信度计算器。
///
/// 加权求和后归一化，返回 [0, 1] 区间的置信度值。
/// 权重可在构造时自定义。
#[derive(Debug, Clone)]
pub struct ConfidenceCalculator {
    weights: ConfidenceWeights,
}

impl Default for ConfidenceCalculator {
    fn default() -> Self {
        Self {
            weights: ConfidenceWeights::default(),
        }
    }
}

impl ConfidenceCalculator {
    /// 使用自定义权重创建计算器
    pub fn new(weights: ConfidenceWeights) -> Self {
        Self { weights }
    }

    /// 计算加权置信度。
    ///
    /// # 公式
    ///
    /// ```text
    /// confidence = (source_match × w_match + source_cardinal × w_cardinal
    ///              + source_property × w_property + source_structural × w_structural)
    ///             / (w_match + w_cardinal + w_property + w_structural)
    /// ```
    ///
    /// 若所有输入均为 0，返回 0.0；若除零（所有权重为 0），返回 0.0。
    pub fn evaluate(&self, input: &ConfidenceInput) -> f64 {
        let total_weight = self.weights.match_weight
            + self.weights.cardinal_weight
            + self.weights.property_weight
            + self.weights.structural_weight;

        if total_weight == 0.0 {
            return 0.0;
        }

        let weighted_sum = input.source_match * self.weights.match_weight
            + input.source_cardinal * self.weights.cardinal_weight
            + input.source_property * self.weights.property_weight
            + input.source_structural * self.weights.structural_weight;

        let confidence = weighted_sum / total_weight;

        // 裁剪到 [0, 1]
        confidence.clamp(0.0, 1.0)
    }

    /// 快速评估：仅基于匹配度（等权重快速通道）。
    ///
    /// 用于推理链路的中间步骤，减少计算开销。
    pub fn evaluate_fast(&self, match_ratio: f64) -> f64 {
        match_ratio.clamp(0.0, 1.0)
    }

    /// 返回当前权重配置（用于调试和调优）
    pub fn weights(&self) -> &ConfidenceWeights {
        &self.weights
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perfect_confidence() {
        let calc = ConfidenceCalculator::default();
        let input = ConfidenceInput {
            source_match: 1.0,
            source_cardinal: 1.0,
            source_property: 1.0,
            source_structural: 1.0,
        };
        assert!((calc.evaluate(&input) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_zero_confidence() {
        let calc = ConfidenceCalculator::default();
        let input = ConfidenceInput {
            source_match: 0.0,
            source_cardinal: 0.0,
            source_property: 0.0,
            source_structural: 0.0,
        };
        assert!((calc.evaluate(&input) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_partial_confidence() {
        let calc = ConfidenceCalculator::default();
        let input = ConfidenceInput {
            source_match: 0.8,
            source_cardinal: 0.5,
            source_property: 0.9,
            source_structural: 0.7,
        };
        let conf = calc.evaluate(&input);
        // expected = (0.8*0.35 + 0.5*0.2 + 0.9*0.25 + 0.7*0.2) / 1.0 = 0.745
        let expected = 0.8 * 0.35 + 0.5 * 0.20 + 0.9 * 0.25 + 0.7 * 0.20;
        assert!((conf - expected).abs() < 1e-10);
    }

    #[test]
    fn test_below_fuse_threshold() {
        let calc = ConfidenceCalculator::default();
        let input = ConfidenceInput {
            source_match: 0.2,
            source_cardinal: 0.3,
            source_property: 0.1,
            source_structural: 0.2,
        };
        let conf = calc.evaluate(&input);
        assert!(conf < 0.3);
    }
}
