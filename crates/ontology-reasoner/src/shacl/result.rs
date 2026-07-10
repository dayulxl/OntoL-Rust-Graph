//! SHACL 验证结果类型。
//!
//! 对应 W3C SHACL 规范中的 `sh:ValidationResult` 和 `sh:ValidationReport`。
//!
//! # 结果结构
//!
//! ```text
//! ValidationReport
//!   ├── conforms: bool            — 全部通过?
//!   ├── total_nodes: usize        — 验证节点总数
//!   ├── total_shapes: usize       — 形状总数
//!   ├── violation_count: usize    — 违规计数
//!   ├── warning_count: usize      — 警告计数
//!   ├── info_count: usize         — 信息计数
//!   └── results: [ValidationResult] — 逐条结果
//! ```

use std::fmt;

use super::ast::Severity;

// ═══════════════════════════════════════════════════════════
// ValidationResult
// ═══════════════════════════════════════════════════════════

/// 单条验证结果。
///
/// 每次约束检查失败产生一条 `ValidationResult`。
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// 严重级别
    pub severity: Severity,
    /// 焦点节点 — 被检查的节点 ID
    pub focus_node: String,
    /// 结果路径 — 指向违规值的属性路径（可选）
    pub result_path: Option<String>,
    /// 违规值（可选，JSON 表示）
    pub value: Option<String>,
    /// 形状名称 — 产生此结果的形状
    pub source_shape: String,
    /// 约束名称 — 具体违反的约束类型
    pub source_constraint: String,
    /// 人类可读的消息
    pub message: String,
    /// 修复建议
    pub suggestion: Option<String>,
    /// 调试用额外信息
    pub detail: Option<String>,
}

impl ValidationResult {
    /// 创建一个 Violation 级别的验证结果
    pub fn violation(
        focus_node: impl Into<String>,
        source_shape: impl Into<String>,
        source_constraint: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Violation,
            focus_node: focus_node.into(),
            result_path: None,
            value: None,
            source_shape: source_shape.into(),
            source_constraint: source_constraint.into(),
            message: message.into(),
            suggestion: None,
            detail: None,
        }
    }

    /// 创建一个 Warning 级别的验证结果
    pub fn warning(
        focus_node: impl Into<String>,
        source_shape: impl Into<String>,
        source_constraint: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Warning,
            focus_node: focus_node.into(),
            result_path: None,
            value: None,
            source_shape: source_shape.into(),
            source_constraint: source_constraint.into(),
            message: message.into(),
            suggestion: None,
            detail: None,
        }
    }

    /// 创建一个 Info 级别的验证结果
    pub fn info(
        focus_node: impl Into<String>,
        source_shape: impl Into<String>,
        source_constraint: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Info,
            focus_node: focus_node.into(),
            result_path: None,
            value: None,
            source_shape: source_shape.into(),
            source_constraint: source_constraint.into(),
            message: message.into(),
            suggestion: None,
            detail: None,
        }
    }

    /// 设置结果路径
    pub fn with_result_path(mut self, path: impl Into<String>) -> Self {
        self.result_path = Some(path.into());
        self
    }

    /// 设置违规值
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// 设置修复建议
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// 设置调试详情
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

impl fmt::Display for ValidationResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {}: {} (shape: {}, constraint: {})",
            self.severity, self.focus_node, self.message, self.source_shape, self.source_constraint
        )?;
        if let Some(ref path) = self.result_path {
            write!(f, " | path: {}", path)?;
        }
        if let Some(ref val) = self.value {
            write!(f, " | value: {}", val)?;
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// ValidationReport
// ═══════════════════════════════════════════════════════════

/// 验证报告 — 一次完整的 SHACL 验证运行的汇总结果。
///
/// 包含是否合规的布尔判定、统计计数、以及逐条验证结果。
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// 图是否完全合规（无 Violation 级别结果）
    pub conforms: bool,
    /// 验证的节点总数
    pub total_nodes: usize,
    /// 使用的形状总数
    pub total_shapes: usize,
    /// Violation 级别结果数
    pub violation_count: usize,
    /// Warning 级别结果数
    pub warning_count: usize,
    /// Info 级别结果数
    pub info_count: usize,
    /// 逐条验证结果
    pub results: Vec<ValidationResult>,
    /// 验证耗时（毫秒）
    pub elapsed_ms: u64,
}

impl ValidationReport {
    /// 创建一个空的合规报告
    pub fn new() -> Self {
        Self {
            conforms: true,
            total_nodes: 0,
            total_shapes: 0,
            violation_count: 0,
            warning_count: 0,
            info_count: 0,
            results: Vec::new(),
            elapsed_ms: 0,
        }
    }

    /// 添加一条验证结果并更新统计
    pub fn add_result(&mut self, result: ValidationResult) {
        match result.severity {
            Severity::Violation => {
                self.violation_count += 1;
                self.conforms = false;
            }
            Severity::Warning => self.warning_count += 1,
            Severity::Info => self.info_count += 1,
        }
        self.results.push(result);
    }

    /// 只返回违规结果（严重级别 = Violation）
    pub fn violations(&self) -> Vec<&ValidationResult> {
        self.results
            .iter()
            .filter(|r| r.severity == Severity::Violation)
            .collect()
    }

    /// 只返回警告结果
    pub fn warnings(&self) -> Vec<&ValidationResult> {
        self.results
            .iter()
            .filter(|r| r.severity == Severity::Warning)
            .collect()
    }

    /// 按形状分组结果
    pub fn group_by_shape(&self) -> Vec<(&str, Vec<&ValidationResult>)> {
        let mut groups: std::collections::BTreeMap<&str, Vec<&ValidationResult>> =
            std::collections::BTreeMap::new();
        for r in &self.results {
            groups.entry(r.source_shape.as_str()).or_default().push(r);
        }
        groups.into_iter().collect()
    }

    /// 按焦点节点分组结果
    pub fn group_by_node(&self) -> Vec<(&str, Vec<&ValidationResult>)> {
        let mut groups: std::collections::BTreeMap<&str, Vec<&ValidationResult>> =
            std::collections::BTreeMap::new();
        for r in &self.results {
            groups.entry(r.focus_node.as_str()).or_default().push(r);
        }
        groups.into_iter().collect()
    }

    /// 获取摘要字符串
    pub fn summary(&self) -> String {
        if self.conforms {
            format!(
                "✓ SHACL Validation PASSED | {} nodes, {} shapes | {} results ({}W {}I) | {}ms",
                self.total_nodes,
                self.total_shapes,
                self.results.len(),
                self.warning_count,
                self.info_count,
                self.elapsed_ms,
            )
        } else {
            format!(
                "✗ SHACL Validation FAILED | {} violations, {} warnings, {} info | {} nodes, {} shapes | {}ms",
                self.violation_count,
                self.warning_count,
                self.info_count,
                self.total_nodes,
                self.total_shapes,
                self.elapsed_ms,
            )
        }
    }
}

impl Default for ValidationReport {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.summary())?;
        if self.results.is_empty() {
            writeln!(
                f,
                "  (no results — graph may be empty or no targets matched)"
            )?;
        } else {
            for (i, result) in self.results.iter().enumerate() {
                writeln!(f, "  {}. {}", i + 1, result)?;
            }
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_result_violation() {
        let r = ValidationResult::violation(
            "node_001",
            "PersonShape",
            "MinCount",
            "Missing required property 'name'",
        )
        .with_result_path("name")
        .with_value("<none>")
        .with_suggestion("Add a 'name' property to this node");

        assert_eq!(r.severity, Severity::Violation);
        assert_eq!(r.focus_node, "node_001");
        assert_eq!(r.source_shape, "PersonShape");
        assert_eq!(r.source_constraint, "MinCount");
        assert!(r.suggestion.is_some());
    }

    #[test]
    fn test_report_conforms() {
        let mut report = ValidationReport::new();
        report.total_nodes = 10;
        report.total_shapes = 3;
        report.add_result(ValidationResult::info(
            "n1",
            "S1",
            "Pattern",
            "Value matches pattern",
        ));
        report.add_result(ValidationResult::warning(
            "n2",
            "S1",
            "MaxCount",
            "Exceeds recommended count",
        ));
        assert!(report.conforms, "No violations → should conform");
        assert_eq!(report.violation_count, 0);
        assert_eq!(report.warning_count, 1);
        assert_eq!(report.info_count, 1);
        assert_eq!(report.violations().len(), 0);
        assert_eq!(report.warnings().len(), 1);
    }

    #[test]
    fn test_report_nonconforming() {
        let mut report = ValidationReport::new();
        report.total_nodes = 5;
        report.add_result(ValidationResult::violation(
            "n3",
            "S2",
            "Datatype",
            "Expected Int, got String",
        ));
        assert!(!report.conforms);
        assert_eq!(report.violation_count, 1);
        assert_eq!(report.violations().len(), 1);
        assert!(report.summary().contains("FAILED"));
    }

    #[test]
    fn test_group_by_shape() {
        let mut report = ValidationReport::new();
        report.add_result(ValidationResult::violation("n1", "ShapeA", "C1", "msg1"));
        report.add_result(ValidationResult::violation("n2", "ShapeB", "C2", "msg2"));
        report.add_result(ValidationResult::warning("n1", "ShapeA", "C3", "msg3"));
        let grouped = report.group_by_shape();
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].1.len(), 2); // ShapeA has 2 results
        assert_eq!(grouped[1].1.len(), 1); // ShapeB has 1 result
    }
}
