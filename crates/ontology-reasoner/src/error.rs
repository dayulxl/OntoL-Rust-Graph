//! 推理引擎统一错误类型。
//!
//! 覆盖 DWL2 DL 解析/查询/SWRL 执行/置信度计算四个维度的错误。

use std::fmt;

use ontology_storage::error::StoreError;

// ═══════════════════════════════════════════════════════════
// 推理引擎错误枚举
// ═══════════════════════════════════════════════════════════

#[derive(Debug)]
pub enum ReasonerError {
    /// DWL2 DL 表达式解析失败
    Dwl2Parse(String),

    /// DWL2 DL 查询编译或执行失败
    Dwl2Query(String),

    /// SWRL 规则语法错误
    SwrlParse(String),

    /// SWRL 规则执行错误
    SwrlExecution(String),

    /// 置信度熔断触发 — 推理链路因置信度过低而中止
    ConfidenceFuse {
        /// 当前置信度值
        confidence: f64,
        /// 阈值
        threshold: f64,
        /// 触发的规则名
        rule_name: String,
    },

    /// 置信度计算异常
    ConfidenceError(String),

    /// 图存储操作错误（透传）
    Storage(StoreError),

    /// 推理超时
    Timeout(String),

    /// 空结果集（无匹配的推理前提）
    NoMatch(String),
}

impl fmt::Display for ReasonerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReasonerError::Dwl2Parse(m) => write!(f, "DWL2 DL parse error: {}", m),
            ReasonerError::Dwl2Query(m) => write!(f, "DWL2 DL query error: {}", m),
            ReasonerError::SwrlParse(m) => write!(f, "SWRL parse error: {}", m),
            ReasonerError::SwrlExecution(m) => write!(f, "SWRL execution error: {}", m),
            ReasonerError::ConfidenceFuse {
                confidence,
                threshold,
                rule_name,
            } => write!(
                f,
                "Confidence fuse tripped: {:.4} < {:.4} in rule '{}'",
                confidence, threshold, rule_name
            ),
            ReasonerError::ConfidenceError(m) => write!(f, "Confidence error: {}", m),
            ReasonerError::Storage(e) => write!(f, "Storage error: {}", e),
            ReasonerError::Timeout(m) => write!(f, "Reasoning timeout: {}", m),
            ReasonerError::NoMatch(m) => write!(f, "No match: {}", m),
        }
    }
}

impl std::error::Error for ReasonerError {}

impl From<StoreError> for ReasonerError {
    fn from(e: StoreError) -> Self {
        ReasonerError::Storage(e)
    }
}
