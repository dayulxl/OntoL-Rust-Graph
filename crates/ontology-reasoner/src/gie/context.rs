//! # InferenceContext — 推理过程状态传递
//!
//! 在推理流水线各阶段之间传递共享状态。
//!
//! ## 置信度传播模型
//!
//! ```text
//! 初始: confidence = 0.8 (默认)
//!
//! BFS 逐层传播:
//!   节点有 confidence 属性 → current *= node.confidence
//!   节点无 confidence 属性 → current 不变，继续
//!   current < engine.threshold → 阻断此分支下游传播
//! ```

/// 推理过程的共享上下文。
///
/// 在推理流水线各阶段之间传递，确保各组件对当前状态有一致视图。
pub struct InferenceContext {
    /// 场景版本标识（UUID），推理在副本上执行。
    pub cope_version: String,
    /// 当前推理链路的置信度（0.0 ~ 1.0）。
    /// 默认 0.8。在 BFS 传播中随节点 confidence 属性逐步调整。
    pub current_confidence: f64,
    /// 当前迭代计数。
    pub iteration: usize,
    /// 推理引擎置信度阈值。
    pub threshold: f64,
}

impl InferenceContext {
    /// 使用给定的 `cope_version` 创建上下文。
    /// 默认置信度 0.8，阈值 0.3（Balanced 模式）。
    pub fn new(cope_version: String) -> Self {
        Self {
            cope_version,
            current_confidence: 0.8,
            iteration: 0,
            threshold: 0.3,
        }
    }

    /// 设置初始置信度（clamp 到 0.0~1.0）。
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.current_confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// 设置推理引擎阈值。
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold = threshold;
        self
    }

    /// 置信度传播：节点有 confidence → 相乘返回新值；无 → 返回当前值不变。
    ///
    /// # 参数
    /// - `node_confidence` — 节点的 confidence 属性值（Option）
    ///
    /// # 返回
    /// - 传播后的置信度值
    pub fn propagate(&self, node_confidence: Option<f64>) -> f64 {
        match node_confidence {
            Some(nc) => {
                let new = self.current_confidence * nc;
                new.clamp(0.0, 1.0)
            }
            None => self.current_confidence,
        }
    }

    /// 检查当前置信度是否低于阈值（低于阈值 → 应阻断下游传播）。
    pub fn is_below_threshold(&self, confidence: f64) -> bool {
        confidence < self.threshold
    }
}

impl Default for InferenceContext {
    fn default() -> Self {
        Self::new("default".to_string())
    }
}
