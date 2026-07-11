//! 语言前缀解析器。
//!
//! 本 crate 中的规则、查询、约束使用统一前缀来区分不同的本体语言和推理指令。
//! 此模块提供前缀检测和表达式剥离功能。
//!
//! ## 前缀约定
//!
//! | 序号 | 前缀 | 语言/类别 | 路由目标 | 说明 |
//! |------|------|----------|----------|------|
//! | 1 | `rdfs:` | RDFS 语言 | 本体语义层 | 基础类型系统（subClassOf、domain、range 等） |
//! | 2 | `owl2:` | OWL2 DL 语言 | DWL2 查询引擎 | 本体语义和关系定义（主力） |
//! | 3 | `swrl:` | SWRL 语言 | SWRL 规则推理引擎 | 推理规则表达 |
//! | 4 | `sh:` | SHACL 语言 | SHACL 验证引擎 | 数据校验约束 |
//! | 5 | `rule:` | 规则设定 | 推理引擎 | forwardChain（前链）/ backwardChain（后链），默认前链 |
//! | 6 | `action:` | 自定义动作 | LLM 模糊推理 | 后面写汉字，大模型自主判断执行什么动作 |
//! | 7 | `function:` | 自定义函数 | LLM JSON 调用 | JSON 格式 `{"id":"图ID","func":"函数名"}` |
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

use std::collections::HashMap;

/// 本体语言前缀枚举（7 种）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LanguagePrefix {
    /// `rdfs:` — RDFS 基础类型系统，用于 subClassOf/domain/range 等
    Rdfs,
    /// `owl2:` — OWL2-DL 描述逻辑，用于 DWL2 类表达式查询
    Owl2,
    /// `swrl:` — 语义 Web 规则语言，用于 SWRL 规则推理
    Swrl,
    /// `sh:` — 形状约束语言，用于 SHACL 图验证
    Shacl,
    /// `rule:` — 推理链方向控制（`forwardChain` / `backward`，默认前向）
    Rule,
    /// `action:` — 自定义动作，对接大模型模糊推理
    Action,
    /// `function:` — 自定义函数，JSON 格式 `{"id":"图ID","func":"函数名"}`
    Function,
}

impl LanguagePrefix {
    /// 所有推理前缀的字符串切片，用于批量匹配
    pub const ALL_PREFIXES: &[&str] = &[
        "rdfs:",
        "owl2:",
        "swrl:",
        "sh:",
        "rule:",
        "action:",
        "function:",
    ];

    /// 根据前缀字符串返回对应的 `LanguagePrefix`（不要求 body 非空）
    fn from_prefix_str(s: &str) -> Option<LanguagePrefix> {
        match s {
            "rdfs:" => Some(LanguagePrefix::Rdfs),
            "owl2:" => Some(LanguagePrefix::Owl2),
            "swrl:" => Some(LanguagePrefix::Swrl),
            "sh:" => Some(LanguagePrefix::Shacl),
            "rule:" => Some(LanguagePrefix::Rule),
            "action:" => Some(LanguagePrefix::Action),
            "function:" => Some(LanguagePrefix::Function),
            _ => None,
        }
    }
}

impl std::fmt::Display for LanguagePrefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rdfs => write!(f, "rdfs"),
            Self::Owl2 => write!(f, "owl2"),
            Self::Swrl => write!(f, "swrl"),
            Self::Shacl => write!(f, "sh"),
            Self::Rule => write!(f, "rule"),
            Self::Action => write!(f, "action"),
            Self::Function => write!(f, "function"),
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
/// 支持七种前缀（大小写敏感）：
/// - `rdfs:` → RDFS 基础类型系统
/// - `owl2:` → OWL2-DL 类表达式
/// - `swrl:` → SWRL 规则
/// - `sh:`  → SHACL 形状约束
/// - `rule:` → 推理链方向
/// - `action:` → 自定义动作（LLM 模糊推理）
/// - `function:` → 自定义函数（JSON 格式）
///
/// 前缀后内容为空时返回空 body（宽容模式：仅前缀标记也视为有效推理表达式）。
///
/// # 错误
///
/// - 字符串为空
/// - 前缀不匹配任何已知语言
pub fn parse_language_expression(input: &str) -> Result<ParsedExpression, String> {
    if input.is_empty() {
        return Err("表达式为空".to_string());
    }

    for prefix_str in LanguagePrefix::ALL_PREFIXES {
        if let Some(rest) = input.strip_prefix(prefix_str) {
            return Ok(ParsedExpression {
                prefix: LanguagePrefix::from_prefix_str(prefix_str).unwrap(),
                body: rest.to_string(),
            });
        }
    }

    Err(format!(
        "不支持的表达式前缀。期望 `rdfs:`、`owl2:`、`swrl:`、`sh:`、`rule:`、`action:` 或 `function:`，收到: {}",
        truncate_for_error(input)
    ))
}

/// 按语言前缀分组后的表达式集合。
#[derive(Debug, Clone, Default)]
pub struct GroupedExpressions {
    /// `rdfs:` 前缀的表达式体
    pub rdfs: Vec<String>,
    /// `owl2:` 前缀的表达式体
    pub owl2: Vec<String>,
    /// `swrl:` 前缀的表达式体
    pub swrl: Vec<String>,
    /// `sh:` 前缀的表达式体
    pub shacl: Vec<String>,
    /// `rule:` 前缀的表达式体
    pub rule: Vec<String>,
    /// `action:` 前缀的表达式体
    pub action: Vec<String>,
    /// `function:` 前缀的表达式体
    pub function: Vec<String>,
}

/// 批量解析传入的表达式，按语言前缀分组返回。
pub fn group_expressions(expressions: &[String]) -> Result<GroupedExpressions, String> {
    let mut grouped = GroupedExpressions::default();

    for expr in expressions {
        let parsed = parse_language_expression(expr)?;
        match parsed.prefix {
            LanguagePrefix::Rdfs => grouped.rdfs.push(parsed.body),
            LanguagePrefix::Owl2 => grouped.owl2.push(parsed.body),
            LanguagePrefix::Swrl => grouped.swrl.push(parsed.body),
            LanguagePrefix::Shacl => grouped.shacl.push(parsed.body),
            LanguagePrefix::Rule => grouped.rule.push(parsed.body),
            LanguagePrefix::Action => grouped.action.push(parsed.body),
            LanguagePrefix::Function => grouped.function.push(parsed.body),
        }
    }

    Ok(grouped)
}

// ═══════════════════════════════════════════════════════════
// 本体语义层关系判断
// ═══════════════════════════════════════════════════════════

/// 判断关系类型是否属于本体语义层（OWL2/RDFS）。
///
/// 本体语义层关系编码了本体的结构语义（类层次、实例归属、属性约束等），
/// 推理机必须**优先处理**，完成后才能进行 SWRL/SHACL 等其他层的推理。
///
/// 匹配规则（任一满足即可）：
/// - 关系类型以 `rdfs:` 或 `owl2:` 前缀开头
/// - 关系类型匹配 `unified_mapping::ONTOLOGY_SEMANTIC_RELS` 中的任一核心常量
pub fn is_ontology_relation(rel_type: &str) -> bool {
    // rdfs: / owl2: 前缀的关系 → 本体语义层
    if rel_type.starts_with("rdfs:") || rel_type.starts_with("owl2:") {
        return true;
    }
    // unified_mapping 中的核心 OWL/RDFS 常量
    ontology_storage::mapper::unified_mapping::ONTOLOGY_SEMANTIC_RELS.contains(&rel_type)
}

// ═══════════════════════════════════════════════════════════
// 推理边/推理属性前缀检测
// ═══════════════════════════════════════════════════════════

/// 检查字符串是否以推理语言前缀开头。
///
/// 覆盖全部七种前缀：`rdfs:` / `owl2:` / `swrl:` / `sh:` / `rule:` / `action:` / `function:`。
/// 用于判断图中的关系类型或属性键/值是否应被推理引擎处理。
/// 仅前缀无内容（如 `"swrl:"`）也返回 `true`。
pub fn is_inference_prefix(s: &str) -> bool {
    LanguagePrefix::ALL_PREFIXES
        .iter()
        .any(|p| s.starts_with(p))
}

/// 检查关系类型是否为推理边（`is_inference_prefix` 的语义别名）。
pub fn is_inference_relation(rel_type: &str) -> bool {
    is_inference_prefix(rel_type)
}

/// 提取推理前缀类型。
///
/// 返回 `Some(LanguagePrefix)` 如果字符串以任一推理前缀开头，
/// 不要求 body 非空。不匹配时返回 `None`。
///
/// # 示例
///
/// ```rust,ignore
/// assert_eq!(classify_inference_prefix("swrl:hasEnemy"), Some(LanguagePrefix::Swrl));
/// assert_eq!(classify_inference_prefix("action:"), Some(LanguagePrefix::Action));
/// assert_eq!(classify_inference_prefix("移动"), None);
/// ```
pub fn classify_inference_prefix(s: &str) -> Option<LanguagePrefix> {
    for prefix_str in LanguagePrefix::ALL_PREFIXES {
        if s.starts_with(prefix_str) {
            return LanguagePrefix::from_prefix_str(prefix_str);
        }
    }
    None
}

/// 按前缀分组所有输入的字符串（关系类型或属性键）。
///
/// 与 `group_expressions` 不同，此函数不要求输入是完整表达式，
/// 仅按前缀分类原始字符串。用于对图中的边/属性进行批量分类。
pub fn classify_strings_by_prefix(strings: &[String]) -> HashMap<LanguagePrefix, Vec<String>> {
    let mut map: HashMap<LanguagePrefix, Vec<String>> = HashMap::new();
    for s in strings {
        if let Some(prefix) = classify_inference_prefix(s) {
            map.entry(prefix).or_default().push(s.clone());
        }
    }
    map
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

    // ── rdfs: 解析 ──

    #[test]
    fn test_parse_rdfs() {
        let parsed = parse_language_expression("rdfs:subClassOf domain").unwrap();
        assert_eq!(parsed.prefix, LanguagePrefix::Rdfs);
        assert_eq!(parsed.body, "subClassOf domain");
    }

    // ── 原有 3 前缀解析 ──

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

    // ── 新增 3 前缀解析 ──

    #[test]
    fn test_parse_rule_forward() {
        let parsed = parse_language_expression("rule:forwardChain").unwrap();
        assert_eq!(parsed.prefix, LanguagePrefix::Rule);
        assert_eq!(parsed.body, "forwardChain");
    }

    #[test]
    fn test_parse_rule_backward() {
        let parsed = parse_language_expression("rule:backward").unwrap();
        assert_eq!(parsed.prefix, LanguagePrefix::Rule);
        assert_eq!(parsed.body, "backward");
    }

    #[test]
    fn test_parse_action() {
        let parsed = parse_language_expression("action:validate_patrol").unwrap();
        assert_eq!(parsed.prefix, LanguagePrefix::Action);
        assert_eq!(parsed.body, "validate_patrol");
    }

    #[test]
    fn test_parse_function() {
        let parsed =
            parse_language_expression(r#"function:{"id":"P8A_001","func":"calculate_threat"}"#)
                .unwrap();
        assert_eq!(parsed.prefix, LanguagePrefix::Function);
        assert_eq!(parsed.body, r#"{"id":"P8A_001","func":"calculate_threat"}"#);
    }

    #[test]
    fn test_empty_input() {
        assert!(parse_language_expression("").is_err());
    }

    #[test]
    fn test_unknown_prefix() {
        assert!(parse_language_expression("rdf:something").is_err());
        assert!(parse_language_expression("xml:data").is_err());
    }

    #[test]
    fn test_empty_body() {
        // 空 body 是合法的——仅前缀也视为有效推理表达式
        for prefix in &[
            "rdfs:",
            "owl2:",
            "swrl:",
            "sh:",
            "rule:",
            "action:",
            "function:",
        ] {
            let parsed = parse_language_expression(prefix).unwrap();
            assert_eq!(parsed.body, "");
        }
    }

    // ── is_inference_prefix ──

    #[test]
    fn test_is_inference_prefix() {
        assert!(is_inference_prefix("rdfs:subClassOf"));
        assert!(is_inference_prefix("owl2:SomeClass"));
        assert!(is_inference_prefix("swrl:hasEnemy(?x,?y)"));
        assert!(is_inference_prefix("sh:MinCount 1"));
        assert!(is_inference_prefix("rule:forwardChain"));
        assert!(is_inference_prefix("action:do_something"));
        assert!(is_inference_prefix("function:calculate"));
        // 仅前缀无内容
        for prefix in &[
            "rdfs:",
            "swrl:",
            "owl2:",
            "sh:",
            "rule:",
            "action:",
            "function:",
        ] {
            assert!(is_inference_prefix(prefix));
        }
        assert!(!is_inference_prefix("移动"));
        assert!(!is_inference_prefix("subClassOf"));
        assert!(!is_inference_prefix(""));
    }

    #[test]
    fn test_is_inference_relation() {
        assert!(is_inference_relation("rdfs:domain"));
        assert!(is_inference_relation("swrl:hasEnemy"));
        assert!(is_inference_relation("owl2:someValuesFrom"));
        assert!(is_inference_relation("sh:property"));
        assert!(is_inference_relation("rule:forwardChain"));
        assert!(is_inference_relation("action:"));
        assert!(is_inference_relation("function:calc"));
        assert!(!is_inference_relation("移动"));
        assert!(!is_inference_relation("INSTANCE_OF"));
    }

    #[test]
    fn test_classify_inference_prefix() {
        assert_eq!(
            classify_inference_prefix("rdfs:subClassOf"),
            Some(LanguagePrefix::Rdfs)
        );
        assert_eq!(
            classify_inference_prefix("owl2:Thing"),
            Some(LanguagePrefix::Owl2)
        );
        assert_eq!(
            classify_inference_prefix("swrl:hasEnemy"),
            Some(LanguagePrefix::Swrl)
        );
        assert_eq!(
            classify_inference_prefix("sh:pattern"),
            Some(LanguagePrefix::Shacl)
        );
        assert_eq!(
            classify_inference_prefix("rule:forwardChain"),
            Some(LanguagePrefix::Rule)
        );
        assert_eq!(
            classify_inference_prefix("action:do"),
            Some(LanguagePrefix::Action)
        );
        assert_eq!(
            classify_inference_prefix("function:f"),
            Some(LanguagePrefix::Function)
        );
        assert_eq!(
            classify_inference_prefix("rdfs:"),
            Some(LanguagePrefix::Rdfs)
        );
        assert_eq!(
            classify_inference_prefix("owl2:"),
            Some(LanguagePrefix::Owl2)
        );
        assert_eq!(
            classify_inference_prefix("swrl:"),
            Some(LanguagePrefix::Swrl)
        );
        assert_eq!(classify_inference_prefix("移动"), None);
        assert_eq!(classify_inference_prefix(""), None);
    }

    // ── group_expressions ──

    #[test]
    fn test_group_expressions() {
        let exprs = vec![
            "rdfs:subClassOf domain".to_string(),
            "owl2::Person".to_string(),
            "swrl:Person(?x) -> Adult(?x)".to_string(),
            "sh:MinCount 1".to_string(),
            "rule:forwardChain".to_string(),
            "action:check".to_string(),
            r#"function:{"id":"N1","func":"f"}"#.to_string(),
        ];
        let grouped = group_expressions(&exprs).unwrap();
        assert_eq!(grouped.rdfs, vec!["subClassOf domain"]);
        assert_eq!(grouped.owl2, vec![":Person"]);
        assert_eq!(grouped.swrl, vec!["Person(?x) -> Adult(?x)"]);
        assert_eq!(grouped.shacl, vec!["MinCount 1"]);
        assert_eq!(grouped.rule, vec!["forwardChain"]);
        assert_eq!(grouped.action, vec!["check"]);
        assert_eq!(grouped.function, vec![r#"{"id":"N1","func":"f"}"#]);
    }

    // ── classify_strings_by_prefix ──

    #[test]
    fn test_classify_strings_by_prefix() {
        let strings: Vec<String> = vec![
            "rdfs:domain".into(),
            "swrl:hasEnemy".into(),
            "移动".into(),
            "owl2:subClass".into(),
            "sh:minCount".into(),
            "action:do".into(),
            "打击".into(),
            "rule:forward".into(),
            "function:calc".into(),
        ];
        let map = classify_strings_by_prefix(&strings);
        assert_eq!(map.get(&LanguagePrefix::Rdfs).unwrap().len(), 1);
        assert_eq!(map.get(&LanguagePrefix::Swrl).unwrap().len(), 1);
        assert_eq!(map.get(&LanguagePrefix::Owl2).unwrap().len(), 1);
        assert_eq!(map.get(&LanguagePrefix::Shacl).unwrap().len(), 1);
        assert_eq!(map.get(&LanguagePrefix::Rule).unwrap().len(), 1);
        assert_eq!(map.get(&LanguagePrefix::Action).unwrap().len(), 1);
        assert_eq!(map.get(&LanguagePrefix::Function).unwrap().len(), 1);
    }

    // ── Display ──

    #[test]
    fn test_display_prefix() {
        assert_eq!(LanguagePrefix::Rdfs.to_string(), "rdfs");
        assert_eq!(LanguagePrefix::Owl2.to_string(), "owl2");
        assert_eq!(LanguagePrefix::Swrl.to_string(), "swrl");
        assert_eq!(LanguagePrefix::Shacl.to_string(), "sh");
        assert_eq!(LanguagePrefix::Rule.to_string(), "rule");
        assert_eq!(LanguagePrefix::Action.to_string(), "action");
        assert_eq!(LanguagePrefix::Function.to_string(), "function");
    }

    #[test]
    fn test_all_prefixes_count() {
        assert_eq!(LanguagePrefix::ALL_PREFIXES.len(), 7);
    }
}
