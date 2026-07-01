//! SWRL 文本规则解析器。
//!
//! 将人类可读的 SWRL 规则语法字符串解析为 `Rule` AST。
//!
//! ## 支持的语法
//!
//! ```text
//! [ruleName: C(?x) ^ P(?x, ?y) ^ ... -> C'(?x) ^ P'(?x, ?z) ^ ...]
//! ```
//!
//! 规则元素：
//! - 规则名（可选）：`ruleName:`
//! - 前提原子：`^` 分隔，AND 语义
//! - 分隔符：`->`
//! - 结论原子：`^` 分隔
//! - 方括号 `[...]` 包围整个规则
//!
//! ## 原子语法
//!
//! | 类型          | 语法                          | 示例                        |
//! |--------------|-------------------------------|-----------------------------|
//! | ClassAtom    | `iri(variable)`               | `:Person(?x)`               |
//! | PropertyAtom | `iri(var, var)`               | `hasParent(?x, ?y)`         |
//! | Builtin      | `builtin:name(arg, arg)`      | `swrlb:greaterThan(?x, 18)` |
//! | SameAs       | `sameAs(var, var)`            | `sameAs(?x, ?y)`            |
//! | DifferentFrom| `differentFrom(var, var)`     | `differentFrom(?x, ?y)`     |
//!
//! ## 变量约定
//!
//! 变量以 `?` 前缀标识（如 `?x`, `?y`）。
//! 不以 `?` 开头的参数被视为字面常量。
//!
//! 前缀约定：
//! - `:Name` → 展开为 `http://example.org#Name`（默认前缀）
//! - `prefix:Name` → 使用注册的前缀映射

use crate::error::ReasonerError;
use crate::swrl::ast::{Atom, Rule};

/// SWRL 规则解析器。
///
/// 支持从文本字符串解析单个规则或批量规则集。
#[derive(Debug, Clone, Default)]
pub struct SwrlParser {
    /// 前缀映射（如 "ex" → "http://example.org#"）
    prefixes: Vec<(String, String)>,
    /// 默认前缀展开
    default_prefix: String,
}

impl SwrlParser {
    /// 创建新解析器
    pub fn new() -> Self {
        Self {
            prefixes: Vec::new(),
            default_prefix: "http://example.org#".to_string(),
        }
    }

    /// 注册前缀映射
    pub fn register_prefix(&mut self, prefix: &str, namespace: &str) {
        self.prefixes
            .push((prefix.to_string(), namespace.to_string()));
    }

    /// 设置默认前缀
    pub fn set_default_prefix(&mut self, ns: &str) {
        self.default_prefix = ns.to_string();
    }

    /// 解析一条 SWRL 规则字符串。
    ///
    /// # 参数
    ///
    /// - `input`: 完整的规则字符串，必须被 `[...]` 包围
    ///
    /// # 返回
    ///
    /// - `Ok(Rule)` — 解析成功的 SAST
    /// - `Err(ReasonerError::SwrlParse)` — 语法错误
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let parser = SwrlParser::new();
    /// let rule = parser.parse(
    ///     "[parentChild: hasParent(?x, ?y) ^ hasBrother(?y, ?z) -> hasUncle(?x, ?z)]"
    /// )?;
    /// ```
    pub fn parse(&self, input: &str) -> Result<Rule, ReasonerError> {
        let s = input.trim();

        // 去除外层的方括号
        let inner = if s.starts_with('[') && s.ends_with(']') {
            &s[1..s.len() - 1]
        } else {
            return Err(ReasonerError::SwrlParse(format!(
                "Rule must be enclosed in brackets [...]: '{}'",
                s
            )));
        };

        // 按 -> 分割前提和结论
        let arrow_pos = inner.find("->").ok_or_else(|| {
            ReasonerError::SwrlParse(format!(
                "Missing '->' separator in rule: '{}'",
                inner
            ))
        })?;

        let before_arrow = &inner[..arrow_pos].trim();
        let after_arrow = inner[arrow_pos + 2..].trim();

        // 提取规则名（若有）
        let (name, ant_body) = match before_arrow.find(':') {
            Some(colon_pos) => {
                let n = before_arrow[..colon_pos].trim();
                let body = before_arrow[colon_pos + 1..].trim();
                (if n.is_empty() { None } else { Some(n.to_string()) }, body)
            }
            None => (None, *before_arrow),
        };

        // 解析前提和结论原子
        let antecedent = self.parse_atoms(ant_body)?;
        let consequent = self.parse_atoms(after_arrow)?;

        if antecedent.is_empty() {
            return Err(ReasonerError::SwrlParse(
                "Antecedent must contain at least one atom".into(),
            ));
        }
        if consequent.is_empty() {
            return Err(ReasonerError::SwrlParse(
                "Consequent must contain at least one atom".into(),
            ));
        }

        let rule = Rule {
            name: name.filter(|n| !n.is_empty()),
            antecedent,
            consequent,
            comment: None,
        };

        // 变量安全性检查
        if !rule.is_safe() {
            let unsafe_vars: Vec<&str> = rule
                .consequent_variables()
                .into_iter()
                .filter(|cv| !rule.antecedent_variables().contains(cv))
                .collect();
            return Err(ReasonerError::SwrlParse(format!(
                "Unsafe rule '{}': consequent variables {:?} not bound in antecedent",
                rule.name.as_deref().unwrap_or("<anonymous>"),
                unsafe_vars
            )));
        }

        Ok(rule)
    }

    /// 批量解析多条规则（换行或分号分隔）
    pub fn parse_all(&self, source: &str) -> Result<Vec<Rule>, ReasonerError> {
        let mut rules = Vec::new();
        let mut errors = Vec::new();

        for (i, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            match self.parse(trimmed) {
                Ok(rule) => rules.push(rule),
                Err(e) => {
                    errors.push(format!("line {}: {}", i + 1, e));
                }
            }
        }

        if rules.is_empty() && !errors.is_empty() {
            return Err(ReasonerError::SwrlParse(format!(
                "All rules failed to parse:\n{}",
                errors.join("\n")
            )));
        }

        // 非致命错误以 log 发出警告
        if !errors.is_empty() {
            log::warn!("Some SWRL rules failed to parse:\n{}", errors.join("\n"));
        }

        Ok(rules)
    }

    // ═══════════════════════════════════════════════════════
    // 内部方法
    // ═══════════════════════════════════════════════════════

    /// 解析原子列表（^ 分隔）
    fn parse_atoms(&self, body: &str) -> Result<Vec<Atom>, ReasonerError> {
        if body.is_empty() {
            return Ok(vec![]);
        }

        let mut atoms = Vec::new();
        // 按 ^ 分割，但注意括号内的 ^ 不作为分隔符
        for part in split_atoms(body) {
            let atom = self.parse_atom(&part)?;
            atoms.push(atom);
        }
        Ok(atoms)
    }

    /// 解析单个原子
    fn parse_atom(&self, text: &str) -> Result<Atom, ReasonerError> {
        let text = text.trim();

        // sameAs(?x, ?y) / differentFrom(?x, ?y)
        if text.starts_with("sameAs(") {
            let args = self.parse_args(text, "sameAs")?;
            if args.len() != 2 {
                return Err(ReasonerError::SwrlParse(format!(
                    "sameAs requires exactly 2 arguments, got {}",
                    args.len()
                )));
            }
            return Ok(Atom::SameAs(args[0].clone(), args[1].clone()));
        }

        if text.starts_with("differentFrom(") {
            let args = self.parse_args(text, "differentFrom")?;
            if args.len() != 2 {
                return Err(ReasonerError::SwrlParse(format!(
                    "differentFrom requires exactly 2 arguments, got {}",
                    args.len()
                )));
            }
            return Ok(Atom::DifferentFrom(args[0].clone(), args[1].clone()));
        }

        // builtin:name(args)
        if text.contains(":") && !text.starts_with('?') {
            let paren_pos = text.find('(').ok_or_else(|| {
                ReasonerError::SwrlParse(format!("Expected '(' in atom: '{}'", text))
            })?;
            let functor = &text[..paren_pos];
            if functor.starts_with("swrlb:") || functor.starts_with("builtin:") {
                let method_name = functor;
                let args = self.extract_args(text, paren_pos)?;
                return Ok(Atom::Builtin {
                    builtin_iri: self.expand_iri(method_name),
                    arguments: args,
                });
            }
        }

        // ClassAtom: iri(var) 或 PropertyAtom: iri(var, var)
        let paren_pos = text.find('(').ok_or_else(|| {
            ReasonerError::SwrlParse(format!("Expected '(' in atom: '{}'", text))
        })?;

        let iri = &text[..paren_pos];
        let expanded_iri = self.expand_iri(iri);
        let args = self.extract_args(text, paren_pos)?;

        match args.len() {
            1 => {
                // ClassAtom: iri(?x)
                Ok(Atom::ClassAtom {
                    class_iri: expanded_iri,
                    variable: args[0].clone(),
                })
            }
            2 => {
                // ObjectPropertyAtom 或 DataPropertyAtom
                // 第二个参数以 ? 开头 → ObjectPropertyAtom
                // 否则 → DataPropertyAtom
                if args[1].starts_with('?') {
                    Ok(Atom::ObjectPropertyAtom {
                        property_iri: expanded_iri,
                        subject: args[0].clone(),
                        object: args[1].clone(),
                    })
                } else {
                    Ok(Atom::DataPropertyAtom {
                        property_iri: expanded_iri,
                        subject: args[0].clone(),
                        value: args[1].clone(),
                    })
                }
            }
            n => Err(ReasonerError::SwrlParse(format!(
                "Atom '{}' has {} arguments; expected 1 or 2",
                text, n
            ))),
        }
    }

    /// 提取位置 paren_pos 后的括号内参数
    fn extract_args(&self, text: &str, paren_pos: usize) -> Result<Vec<String>, ReasonerError> {
        let args_str = &text[paren_pos..];
        self.parse_args(args_str, "")
    }

    /// 解析括号内参数列表 "(arg1, arg2, ...)"
    fn parse_args(&self, text: &str, _fn_name: &str) -> Result<Vec<String>, ReasonerError> {
        let open_paren = text.find('(').ok_or_else(|| {
            ReasonerError::SwrlParse(format!("Missing '(' in: {}", text))
        })?;
        let close_paren = text.rfind(')').ok_or_else(|| {
            ReasonerError::SwrlParse(format!("Missing ')' in: {}", text))
        })?;

        let inner = &text[open_paren + 1..close_paren];
        if inner.trim().is_empty() {
            return Ok(vec![]);
        }

        Ok(inner
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect())
    }

    /// 展开短 IRI 为完整 IRI
    fn expand_iri(&self, iri: &str) -> String {
        // 完整 IRI 直接返回
        if iri.starts_with("http://") || iri.starts_with("https://") {
            return iri.to_string();
        }

        // 短前缀展开
        if let Some(colon_pos) = iri.find(':') {
            let prefix = &iri[..colon_pos];
            let local = &iri[colon_pos + 1..];

            for (p, ns) in &self.prefixes {
                if p == prefix {
                    return format!("{}{}", ns, local);
                }
            }
        }

        // 默认前缀展开（如 `:Person` → `http://example.org#Person`）
        if iri.starts_with(':') {
            return format!("{}{}", self.default_prefix, &iri[1..]);
        }

        // 无法展开，原样返回
        iri.to_string()
    }
}

/// 按 ^ 分割原子字符串，正确处理括号嵌套
fn split_atoms(body: &str) -> Vec<String> {
    let mut atoms = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;

    for (i, ch) in body.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            '^' if depth == 0 => {
                atoms.push(body[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }

    // 最后一个原子
    let last = body[start..].trim().to_string();
    if !last.is_empty() {
        atoms.push(last);
    }

    atoms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_class_atom() {
        let parser = SwrlParser::new();
        let rule = parser.parse(
            "[test: :Person(?x) -> :Animal(?x)]"
        ).unwrap();

        assert_eq!(rule.name.as_deref().unwrap(), "test");
        assert_eq!(rule.antecedent.len(), 1);
        assert_eq!(rule.consequent.len(), 1);
        assert!(rule.is_safe());
    }

    #[test]
    fn test_parse_property_chain() {
        let parser = SwrlParser::new();
        let rule = parser.parse(
            "[parentChild: hasParent(?x, ?y) ^ hasBrother(?y, ?z) -> hasUncle(?x, ?z)]"
        ).unwrap();

        assert_eq!(rule.name.as_deref().unwrap(), "parentChild");
        assert_eq!(rule.antecedent.len(), 2);
        assert_eq!(rule.consequent.len(), 1);
        assert!(rule.is_safe());
    }

    #[test]
    fn test_parse_builtin() {
        let parser = SwrlParser::new();
        let rule = parser.parse(
            "[adult: :Person(?x) ^ hasAge(?x, ?age) ^ swrlb:greaterThan(?age, 18) -> :Adult(?x)]"
        ).unwrap();

        assert_eq!(rule.antecedent.len(), 3);
        assert!(rule.is_safe());
    }

    #[test]
    fn test_unsafe_rule_rejected() {
        let parser = SwrlParser::new();
        let result = parser.parse(
            "[bad: :Person(?x) -> hasFriend(?x, ?y)]"  // ?y not in antecedent
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_brackets() {
        let parser = SwrlParser::new();
        let result = parser.parse(":Person(?x) -> :Animal(?x)");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_same_as_different_from() {
        let parser = SwrlParser::new();
        let rule = parser.parse(
            "[eq: :Person(?x) ^ :Person(?y) ^ sameAs(?x, ?y) -> :EquivalentPerson(?x)]"
        ).unwrap();
        assert_eq!(rule.antecedent.len(), 3);
        assert!(rule.is_safe());
    }

    #[test]
    fn test_prefix_expansion() {
        let mut parser = SwrlParser::new();
        parser.register_prefix("ex", "http://example.org#");
        let rule = parser.parse(
            "[t: ex:Person(?x) -> ex:Animal(?x)]"
        ).unwrap();

        match &rule.antecedent[0] {
            Atom::ClassAtom { class_iri, .. } => {
                assert_eq!(class_iri, "http://example.org#Person");
            }
            _ => panic!("Expected ClassAtom"),
        }
    }
}
