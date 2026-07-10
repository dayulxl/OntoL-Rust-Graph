//! 语言前缀解析器。
//!
//! 本 crate 中的规则、查询、约束使用统一前缀来区分不同的本体语言。
//! 此模块提供前缀检测和表达式剥离功能。
//!
//! ## 前缀约定
//!
//! | 前缀 | 语言 | 引擎 |
//! |------|------|------|
//! | `owl2:` | OWL2-DL | DWL2 查询引擎 |
//! | `swrl:` | SWRL | SWRL 规则推理引擎 |
//! | `sh:` | SHACL | SHACL 验证引擎 |
//!
//! ## 示例
//!
//! ```rust,ignore
//! use ontology_reasoner::language::parse_language_expression;
//!
//! let parsed = parse_language_expression("swrl:hasEnemy(?x,?y) -> alert(?x,?y)").unwrap();
//! assert_eq!(parsed.prefix, LanguagePrefix::Swrl);
//! assert_eq!(parsed.body, "hasEnemy(?x,?y) -> alert(?x,?y)");
//! ```

/// 本体语言前缀枚举。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LanguagePrefix {
    /// `owl2:` — OWL2-DL 描述逻辑，用于 DWL2 类表达式查询
    Owl2,
    /// `swrl:` — 语义 Web 规则语言，用于 SWRL 规则推理
    Swrl,
    /// `sh:` — 形状约束语言，用于 SHACL 图验证
    Shacl,
}

impl std::fmt::Display for LanguagePrefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Owl2 => write!(f, "owl2"),
            Self::Swrl => write!(f, "swrl"),
            Self::Shacl => write!(f, "sh"),
        }
    }
}

/// 解析后的表达式 — 包含语言前缀和剥离后的表达式体。
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedExpression {
    /// 语言前缀
    pub prefix: LanguagePrefix,
    /// 去掉前缀后的表达式体
    pub body: String,
}

/// 将完整的表达式字符串解析为 `ParsedExpression`。
///
/// 支持三种前缀（大小写敏感）：
/// - `owl2:` → OWL2-DL 类表达式
/// - `swrl:` → SWRL 规则
/// - `sh:`  → SHACL 形状约束
///
/// # 错误
///
/// - 字符串为空
/// - 前缀不匹配任何已知语言
pub fn parse_language_expression(input: &str) -> Result<ParsedExpression, String> {
    if input.is_empty() {
        return Err("表达式为空".to_string());
    }

    if let Some(rest) = input.strip_prefix("owl2:") {
        if rest.is_empty() {
            return Err("owl2: 前缀后缺少表达式体".to_string());
        }
        Ok(ParsedExpression {
            prefix: LanguagePrefix::Owl2,
            body: rest.to_string(),
        })
    } else if let Some(rest) = input.strip_prefix("swrl:") {
        if rest.is_empty() {
            return Err("swrl: 前缀后缺少表达式体".to_string());
        }
        Ok(ParsedExpression {
            prefix: LanguagePrefix::Swrl,
            body: rest.to_string(),
        })
    } else if let Some(rest) = input.strip_prefix("sh:") {
        if rest.is_empty() {
            return Err("sh: 前缀后缺少表达式体".to_string());
        }
        Ok(ParsedExpression {
            prefix: LanguagePrefix::Shacl,
            body: rest.to_string(),
        })
    } else {
        Err(format!(
            "不支持的表达式前缀。期望 `owl2:`、`swrl:` 或 `sh:`，收到: {}",
            truncate_for_error(input)
        ))
    }
}

/// 按语言前缀分组的表达式三元组。
///
/// `(owl2_body, swrl_body, shacl_body)`
pub type GroupedExpressions = (Vec<String>, Vec<String>, Vec<String>);

/// 批量解析传入的表达式，按语言前缀分组返回。
///
/// 返回三元组 `(owl2_expression_bodies, swrl_expression_bodies, shacl_expression_bodies)`。
pub fn group_expressions(expressions: &[String]) -> Result<GroupedExpressions, String> {
    let mut owl2 = Vec::new();
    let mut swrl = Vec::new();
    let mut shacl = Vec::new();

    for expr in expressions {
        let parsed = parse_language_expression(expr)?;
        match parsed.prefix {
            LanguagePrefix::Owl2 => owl2.push(parsed.body),
            LanguagePrefix::Swrl => swrl.push(parsed.body),
            LanguagePrefix::Shacl => shacl.push(parsed.body),
        }
    }

    Ok((owl2, swrl, shacl))
}

fn truncate_for_error(s: &str) -> String {
    let end = s
        .char_indices()
        .take(40)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    if s.len() > end {
        format!("{}...", &s[..end])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_owl2() {
        let parsed =
            parse_language_expression("owl2:ObjectIntersectionOf(:Person :Employee)").unwrap();
        assert_eq!(parsed.prefix, LanguagePrefix::Owl2);
        assert_eq!(parsed.body, "ObjectIntersectionOf(:Person :Employee)");
    }

    #[test]
    fn test_parse_swrl() {
        let parsed =
            parse_language_expression("swrl:Person(?p) ^ hasAge(?p, ?age) -> Adult(?p)").unwrap();
        assert_eq!(parsed.prefix, LanguagePrefix::Swrl);
        assert_eq!(parsed.body, "Person(?p) ^ hasAge(?p, ?age) -> Adult(?p)");
    }

    #[test]
    fn test_parse_sh() {
        let parsed =
            parse_language_expression("sh:property [ sh:path :name; sh:minCount 1 ]").unwrap();
        assert_eq!(parsed.prefix, LanguagePrefix::Shacl);
        assert_eq!(parsed.body, "property [ sh:path :name; sh:minCount 1 ]");
    }

    #[test]
    fn test_empty_input() {
        assert!(parse_language_expression("").is_err());
    }

    #[test]
    fn test_unknown_prefix() {
        assert!(parse_language_expression("rdf:something").is_err());
    }

    #[test]
    fn test_empty_body() {
        assert!(parse_language_expression("owl2:").is_err());
        assert!(parse_language_expression("swrl:").is_err());
        assert!(parse_language_expression("sh:").is_err());
    }

    #[test]
    fn test_group_expressions() {
        let exprs = vec![
            "owl2::Person".to_string(),
            "swrl:Person(?x) -> Adult(?x)".to_string(),
            "sh:MinCount 1".to_string(),
        ];
        let (owl2, swrl, shacl) = group_expressions(&exprs).unwrap();
        assert_eq!(owl2, vec![":Person"]);
        assert_eq!(swrl, vec!["Person(?x) -> Adult(?x)"]);
        assert_eq!(shacl, vec!["MinCount 1"]);
    }

    #[test]
    fn test_display_prefix() {
        assert_eq!(LanguagePrefix::Owl2.to_string(), "owl2");
        assert_eq!(LanguagePrefix::Swrl.to_string(), "swrl");
        assert_eq!(LanguagePrefix::Shacl.to_string(), "sh");
    }
}
