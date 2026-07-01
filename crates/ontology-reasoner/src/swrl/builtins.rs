//! SWRL 内置函数实现。
//!
//! 实现 SWRL Built-Ins 规范中的核心比较、数学、字符串、布尔函数。
//!
//! ## 内置函数分类
//!
//! | 类别            | 函数                                         |
//! |-----------------|----------------------------------------------|
//! | 比较            | equal, notEqual, greaterThan, lessThan, ... |
//! | 数学            | add, subtract, multiply, divide, abs, ...   |
//! | 字符串          | contains, startsWith, endsWith, concat, ... |
//! | 布尔            | booleanNot, matches, ...                    |
//! | 列表            | listContains, listLength, ...               |
//!
//! ## 参数约定
//!
//! 内置函数可以接受变量（`?x`）和字面常量。
//! 在执行时，变量已被绑定到具体值。
//! 带有解绑变量的 builtin 调用会被跳过（不产生绑定，也不失败）。

use std::collections::HashMap;

/// 内置函数参数类型
#[derive(Debug, Clone, PartialEq)]
pub enum BuiltinValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    /// 未绑定变量 — 执行时跳过
    Unbound,
}

/// 内置函数执行结果
#[derive(Debug, Clone, PartialEq)]
pub enum BuiltinResult {
    /// 返回布尔值（大多数比较函数）
    Boolean(bool),
    /// 返回计算结果值
    Value(BuiltinValue),
    /// 当前无法求值（变量未绑定）— 不视为失败
    Deferred,
}

/// 内置函数处理器类型
type BuiltinFn = fn(&[BuiltinValue]) -> Result<BuiltinResult, String>;

/// SWRL 内置函数注册表。
///
/// 管理内置函数的注册、查找和执行。
/// 推理引擎在执行规则时通过此注册表调用内置函数。
#[derive(Clone)]
pub struct BuiltinRegistry {
    functions: HashMap<String, BuiltinFn>,
}

impl BuiltinRegistry {
    /// 创建预装所有标准内置函数的新注册表
    pub fn new() -> Self {
        let mut registry = Self {
            functions: HashMap::new(),
        };
        registry.register_all_standard();
        registry
    }

    /// 执行内置函数调用
    ///
    /// # 参数
    ///
    /// - `builtin_iri` — 内置函数 IRI（如 `swrlb:greaterThan`）
    /// - `args` — 参数值列表
    ///
    /// # 返回
    ///
    /// - `Ok(BuiltinResult)` — 执行结果
    /// - `Err(String)` — 函数未注册或执行错误
    pub fn execute(
        &self,
        builtin_iri: &str,
        args: &[BuiltinValue],
    ) -> Result<BuiltinResult, String> {
        // 如果有未绑定变量，推迟执行
        if args.iter().any(|a| matches!(a, BuiltinValue::Unbound)) {
            return Ok(BuiltinResult::Deferred);
        }

        // 尝试完整 IRI 和短名称
        let fn_key = if builtin_iri.contains('#') {
            builtin_iri.rsplit('#').next().unwrap_or(builtin_iri)
        } else if builtin_iri.contains(':') {
            builtin_iri.rsplit(':').next().unwrap_or(builtin_iri)
        } else {
            builtin_iri
        };

        let func = self.functions.get(fn_key).or_else(|| {
            self.functions.get(builtin_iri)
        });

        match func {
            Some(f) => f(args),
            None => Err(format!("Unknown builtin: {}", builtin_iri)),
        }
    }

    /// 注册自定义内置函数
    pub fn register(
        &mut self,
        name: &str,
        func: BuiltinFn,
    ) {
        self.functions.insert(name.to_string(), func);
    }

    /// 检查内置函数是否已注册
    pub fn contains(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    // ═══════════════════════════════════════════════════════
    // 标准函数注册
    // ═══════════════════════════════════════════════════════

    fn register_all_standard(&mut self) {
        // ── 比较函数 ──
        self.register("equal", |args| {
            if args.len() < 2 { return Err("equal requires 2 args".into()); }
            Ok(BuiltinResult::Boolean(args[0] == args[1]))
        });

        self.register("notEqual", |args| {
            if args.len() < 2 { return Err("notEqual requires 2 args".into()); }
            Ok(BuiltinResult::Boolean(args[0] != args[1]))
        });

        self.register("greaterThan", |args| {
            if args.len() < 2 { return Err("greaterThan requires 2 args".into()); }
            let ord = compare_values(&args[0], &args[1])?;
            Ok(BuiltinResult::Boolean(ord.is_gt()))
        });

        self.register("lessThan", |args| {
            if args.len() < 2 { return Err("lessThan requires 2 args".into()); }
            let ord = compare_values(&args[0], &args[1])?;
            Ok(BuiltinResult::Boolean(ord.is_lt()))
        });

        self.register("greaterThanOrEqual", |args| {
            if args.len() < 2 { return Err("greaterThanOrEqual requires 2 args".into()); }
            let ord = compare_values(&args[0], &args[1])?;
            Ok(BuiltinResult::Boolean(ord.is_ge()))
        });

        self.register("lessThanOrEqual", |args| {
            if args.len() < 2 { return Err("lessThanOrEqual requires 2 args".into()); }
            let ord = compare_values(&args[0], &args[1])?;
            Ok(BuiltinResult::Boolean(ord.is_le()))
        });

        // ── 数学运算 ──
        self.register("add", |args| {
            let (a, b) = extract_two_numbers(args)?;
            match (a, b) {
                (BuiltinValue::Integer(x), BuiltinValue::Integer(y)) => {
                    Ok(BuiltinResult::Value(BuiltinValue::Integer(x + y)))
                }
                (x, y) => Ok(BuiltinResult::Value(BuiltinValue::Float(
                    as_f64(x) + as_f64(y),
                ))),
            }
        });

        self.register("subtract", |args| {
            let (a, b) = extract_two_numbers(args)?;
            match (a, b) {
                (BuiltinValue::Integer(x), BuiltinValue::Integer(y)) => {
                    Ok(BuiltinResult::Value(BuiltinValue::Integer(x - y)))
                }
                (x, y) => Ok(BuiltinResult::Value(BuiltinValue::Float(
                    as_f64(x) - as_f64(y),
                ))),
            }
        });

        self.register("multiply", |args| {
            let (a, b) = extract_two_numbers(args)?;
            match (a, b) {
                (BuiltinValue::Integer(x), BuiltinValue::Integer(y)) => {
                    Ok(BuiltinResult::Value(BuiltinValue::Integer(x * y)))
                }
                (x, y) => Ok(BuiltinResult::Value(BuiltinValue::Float(
                    as_f64(x) * as_f64(y),
                ))),
            }
        });

        self.register("divide", |args| {
            let (a, b) = extract_two_numbers(args)?;
            let divisor = as_f64(b);
            if divisor.abs() < f64::EPSILON {
                return Err("Division by zero".into());
            }
            Ok(BuiltinResult::Value(BuiltinValue::Float(as_f64(a) / divisor)))
        });

        self.register("abs", |args| {
            if args.is_empty() { return Err("abs requires 1 arg".into()); }
            match &args[0] {
                BuiltinValue::Integer(x) => Ok(BuiltinResult::Value(BuiltinValue::Integer(x.abs()))),
                BuiltinValue::Float(x) => Ok(BuiltinResult::Value(BuiltinValue::Float(x.abs()))),
                _ => Err("abs requires numeric argument".into()),
            }
        });

        // ── 字符串函数 ──
        self.register("contains", |args| {
            if args.len() < 2 { return Err("contains requires 2 args".into()); }
            let (s, substr) = extract_two_strings(args)?;
            Ok(BuiltinResult::Boolean(s.contains(&substr)))
        });

        self.register("startsWith", |args| {
            if args.len() < 2 { return Err("startsWith requires 2 args".into()); }
            let (s, prefix) = extract_two_strings(args)?;
            Ok(BuiltinResult::Boolean(s.starts_with(&prefix)))
        });

        self.register("endsWith", |args| {
            if args.len() < 2 { return Err("endsWith requires 2 args".into()); }
            let (s, suffix) = extract_two_strings(args)?;
            Ok(BuiltinResult::Boolean(s.ends_with(&suffix)))
        });

        self.register("concat", |args| {
            let result: String = args
                .iter()
                .map(|v| match v {
                    BuiltinValue::String(s) => s.clone(),
                    BuiltinValue::Integer(i) => i.to_string(),
                    BuiltinValue::Float(f) => f.to_string(),
                    BuiltinValue::Boolean(b) => b.to_string(),
                    BuiltinValue::Unbound => String::new(),
                })
                .collect();
            Ok(BuiltinResult::Value(BuiltinValue::String(result)))
        });

        // ── 布尔函数 ──
        self.register("booleanNot", |args| {
            if args.is_empty() { return Err("booleanNot requires 1 arg".into()); }
            match &args[0] {
                BuiltinValue::Boolean(b) => Ok(BuiltinResult::Boolean(!b)),
                _ => Err("booleanNot requires boolean argument".into()),
            }
        });

        // ── 列表函数 ──
        self.register("listContains", |args| {
            if args.len() < 2 { return Err("listContains requires 2 args".into()); }
            // 简化实现：列表以逗号分隔的字符串形式表示
            let list_str = match &args[0] { BuiltinValue::String(s) => s, _ => return Err("listContains: first arg must be string list".into()) };
            let item_str = match &args[1] { BuiltinValue::String(s) => s, _ => return Err("listContains: second arg must be string".into()) };
            let items: Vec<&str> = list_str.split(',').map(|s| s.trim()).collect();
            Ok(BuiltinResult::Boolean(items.contains(&item_str.as_str())))
        });

        self.register("listLength", |args| {
            if args.is_empty() { return Err("listLength requires 1 arg".into()); }
            let list_str = match &args[0] { BuiltinValue::String(s) => s, _ => return Err("listLength: arg must be string list".into()) };
            let count = list_str.split(',').filter(|s| !s.trim().is_empty()).count();
            Ok(BuiltinResult::Value(BuiltinValue::Integer(count as i64)))
        });
    }
}

impl Default for BuiltinRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════
// 辅助函数
// ═══════════════════════════════════════════════════════════

fn compare_values(a: &BuiltinValue, b: &BuiltinValue) -> Result<std::cmp::Ordering, String> {
    match (a, b) {
        (BuiltinValue::Integer(x), BuiltinValue::Integer(y)) => Ok(x.cmp(y)),
        (BuiltinValue::Float(x), BuiltinValue::Float(y)) => {
            x.partial_cmp(y).ok_or_else(|| "Cannot compare NaN".to_string())
        }
        (BuiltinValue::Integer(x), BuiltinValue::Float(y)) => {
            (*x as f64).partial_cmp(y).ok_or_else(|| "Cannot compare NaN".to_string())
        }
        (BuiltinValue::Float(x), BuiltinValue::Integer(y)) => {
            x.partial_cmp(&(*y as f64)).ok_or_else(|| "Cannot compare NaN".to_string())
        }
        (BuiltinValue::String(x), BuiltinValue::String(y)) => Ok(x.cmp(y)),
        _ => Err(format!("Cannot compare {:?} with {:?}", a, b)),
    }
}

fn as_f64(v: BuiltinValue) -> f64 {
    match v {
        BuiltinValue::Integer(i) => i as f64,
        BuiltinValue::Float(f) => f,
        _ => f64::NAN,
    }
}

fn extract_two_numbers(args: &[BuiltinValue]) -> Result<(BuiltinValue, BuiltinValue), String> {
    if args.len() < 2 {
        return Err("Expected 2 numeric arguments".into());
    }
    let a = ensure_numeric(&args[0])?;
    let b = ensure_numeric(&args[1])?;
    Ok((a, b))
}

fn ensure_numeric(v: &BuiltinValue) -> Result<BuiltinValue, String> {
    match v {
        BuiltinValue::Integer(_) | BuiltinValue::Float(_) => Ok(v.clone()),
        BuiltinValue::String(s) => {
            if let Ok(i) = s.parse::<i64>() {
                Ok(BuiltinValue::Integer(i))
            } else if let Ok(f) = s.parse::<f64>() {
                Ok(BuiltinValue::Float(f))
            } else {
                Err(format!("Cannot convert '{}' to number", s))
            }
        }
        _ => Err(format!("Not a numeric value: {:?}", v)),
    }
}

fn extract_two_strings(args: &[BuiltinValue]) -> Result<(String, String), String> {
    if args.len() < 2 {
        return Err("Expected 2 string arguments".into());
    }
    let a = as_string(&args[0]);
    let b = as_string(&args[1]);
    Ok((a, b))
}

fn as_string(v: &BuiltinValue) -> String {
    match v {
        BuiltinValue::String(s) => s.clone(),
        BuiltinValue::Integer(i) => i.to_string(),
        BuiltinValue::Float(f) => f.to_string(),
        BuiltinValue::Boolean(b) => b.to_string(),
        BuiltinValue::Unbound => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comparison() {
        let reg = BuiltinRegistry::new();

        let r = reg.execute("greaterThan", &[
            BuiltinValue::Integer(10),
            BuiltinValue::Integer(5),
        ]).unwrap();
        assert_eq!(r, BuiltinResult::Boolean(true));

        let r = reg.execute("lessThan", &[
            BuiltinValue::Integer(3),
            BuiltinValue::Integer(7),
        ]).unwrap();
        assert_eq!(r, BuiltinResult::Boolean(true));
    }

    #[test]
    fn test_arithmetic() {
        let reg = BuiltinRegistry::new();

        let r = reg.execute("add", &[
            BuiltinValue::Integer(3),
            BuiltinValue::Integer(4),
        ]).unwrap();
        assert_eq!(r, BuiltinResult::Value(BuiltinValue::Integer(7)));
    }

    #[test]
    fn test_string_contains() {
        let reg = BuiltinRegistry::new();

        let r = reg.execute("contains", &[
            BuiltinValue::String("hello world".into()),
            BuiltinValue::String("world".into()),
        ]).unwrap();
        assert_eq!(r, BuiltinResult::Boolean(true));
    }

    #[test]
    fn test_unbound_defers() {
        let reg = BuiltinRegistry::new();
        let r = reg.execute("equal", &[
            BuiltinValue::Unbound,
            BuiltinValue::Integer(5),
        ]).unwrap();
        assert_eq!(r, BuiltinResult::Deferred);
    }
}
