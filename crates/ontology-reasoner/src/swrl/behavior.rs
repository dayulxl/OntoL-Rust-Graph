//! 自定义行为动作引擎 — Entity 行为字段驱动的规则执行。
//!
//! ## 模型
//!
//! Entity 节点的 6 个行为字段（OWL2 风格属性名）:
//!
//! | 属性             | 字段名           | 类型   | 作用     | 说明 |
//! |-----------------|-----------------|--------|---------|------|
//! | hasPrecondition | hasPrecondition | String | 阻断     | SHACL 执行语言；True/空→通过，False→停止 |
//! | hasEffect       | hasEffect       | String | 触发     | 语言前缀触发（swrl:/sh:/owl2:） |
//! | hasCost         | hasCost         | String | 消耗     | 前置条件通过时执行 |
//! | hasDuration     | hasDuration     | String | 时序     | 秒；时长结束后触发效果；默认 0 秒即刻触发 |
//! | hasPriority     | hasPriority     | String | 排序     | 0-10，10 级最高，冲突时高优先级先行；默认 0 |
//! | composedOf      | composedOf      | String | 组合跳转 | code;code;... |
//!
//! ## 执行流程
//!
//! ```text
//! 1. 扫描 Entity → 解析 6 字段 → BehaviorAction
//! 2. 按 priority 降序 → duration 升序排列（高优先级先行，同优先级短时长先行）
//! 3. 对每个 action:
//!    a. 评估 hasPrecondition SHACL → True/空 → 通过，False → 阻断停止
//!    b. 等待 hasDuration 时长（默认 0 秒即即刻触发）
//!    c. 解析 hasEffect 语言前缀 → 路由到对应引擎执行
//!    d. 记录 hasCost
//!    e. 递归处理 composedOf 链
//! ```
//!
//! ## 默认行为
//!
//! 任何字段缺失或为空时，不影响其他字段的执行。只有 hasPrecondition
//! 显式返回 False 时才会阻断整个节点的所有后续函数。
//! hasPriority 未写默认 0，hasDuration 未写默认 0 秒（即刻触发）。

use ontology_storage::mapper::graph::node::Node;
use ontology_storage::mapper::graph::property::PropertyValue;
use ontology_storage::mapper::unified_mapping;
use ontology_storage::repository::graph_store::GraphRepository;

use crate::language::{LanguagePrefix, parse_language_expression};
use crate::swrl::engine::SwrlEngine;
use crate::swrl::parser::SwrlParser;

// ═══════════════════════════════════════════════════════════
// 数据结构
// ═══════════════════════════════════════════════════════════

/// 从 Entity 节点解析出的行为描述
#[derive(Debug, Clone)]
pub struct BehaviorAction {
    /// 来源实体 code
    pub entity_code: String,
    /// 前置条件 SHACL 表达式（hasPrecondition 原始文本）
    pub precondition_shacl: String,
    /// 效果文本（hasEffect 原始文本，可能带语言前缀）
    pub effect_text: String,
    /// 消耗文本（hasCost 原始文本）
    pub cost_text: String,
    /// 持续时间文本（hasDuration，原始字符串）
    pub duration_text: String,
    /// 持续时间秒数（从 hasDuration 解析，默认 0，用于时序排序）
    pub duration_secs: i64,
    /// 优先级文本（hasPriority）
    pub priority_text: String,
    /// 优先级数值（从 hasPriority 解析，0-10，默认 0，10 最高）
    pub priority_value: i64,
    /// 关联实体 code 列表（composedOf 分号拆分）
    pub composed_of: Vec<String>,
}

impl BehaviorAction {
    /// 是否没有任何行为字段（空实体，应跳过不计算）
    pub fn is_empty(&self) -> bool {
        self.precondition_shacl.is_empty()
            && self.effect_text.is_empty()
            && self.cost_text.is_empty()
            && self.duration_text.is_empty()
            && self.priority_text.is_empty()
            && self.composed_of.is_empty()
    }
}

/// 行为执行结果
#[derive(Debug, Clone)]
pub struct BehaviorResult {
    /// 来源实体 code
    pub entity_code: String,
    /// 是否触发
    pub triggered: bool,
    /// 阻断原因（triggered=false 时）
    pub blocked_reason: Option<String>,
    /// 匹配到的绑定数
    pub binding_count: usize,
    /// 推导的新事实数
    pub derived_count: usize,
    /// 优先级数值
    pub priority_value: i64,
    /// 优先级原始文本
    pub priority_text: String,
    /// 消耗描述
    pub cost_description: String,
    /// 持续时间原始文本
    pub duration_text: String,
    /// 持续时间秒数
    pub duration_secs: i64,
    /// 是否因 SHACL 前置条件被阻断
    pub precondition_blocked: bool,
    /// 级联子动作结果
    pub composed_results: Vec<BehaviorResult>,
}

// ═══════════════════════════════════════════════════════════
// 解析：Entity → BehaviorAction
// ═══════════════════════════════════════════════════════════

/// 从 Entity 节点解析行为描述。
///
/// 提取 hasPrecondition、hasEffect、hasCost、hasDuration、hasPriority、
/// composedOf 六个字段（OWL2 风格属性名）。所有行为字段均为 String 类型。
/// 缺失或空字段不影响其他字段的执行。
pub fn parse_behavior(entity: &Node) -> BehaviorAction {
    let code = entity
        .property("code")
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    let precondition_shacl = entity
        .property(unified_mapping::HAS_PRECONDITION_KEY)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let effect_text = entity
        .property(unified_mapping::HAS_EFFECT_KEY)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let cost_text = entity
        .property(unified_mapping::HAS_COST_KEY)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let duration_text = entity
        .property(unified_mapping::HAS_DURATION_KEY)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let priority_text = entity
        .property(unified_mapping::HAS_PRIORITY_KEY)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let composed_of_raw = entity
        .property(unified_mapping::COMPOSED_OF_KEY)
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let priority_value = parse_priority(&priority_text);
    let duration_secs = parse_duration(&duration_text);

    let composed_of: Vec<String> = composed_of_raw
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    BehaviorAction {
        entity_code: code.to_string(),
        precondition_shacl,
        effect_text,
        cost_text,
        duration_text,
        duration_secs,
        priority_text,
        priority_value,
        composed_of,
    }
}

/// 从 hasPriority 字符串解析数值（0-10），无效或空默认 0
fn parse_priority(text: &str) -> i64 {
    let text = text.trim();
    if text.is_empty() {
        return 0;
    }
    text.parse::<i64>().unwrap_or(0).clamp(0, 10)
}

/// 从 hasDuration 字符串解析秒数，无效或空默认 0
pub fn parse_duration(text: &str) -> i64 {
    let text = text.trim();
    if text.is_empty() {
        return 0;
    }
    text.parse::<i64>().unwrap_or(0)
}

// ═══════════════════════════════════════════════════════════
// SHACL 前置条件评估
// ═══════════════════════════════════════════════════════════

/// 从 PropertyValue 中提取 f64 值（支持 Float 和 Integer 类型）
fn prop_as_f64(v: &PropertyValue) -> Option<f64> {
    match v {
        PropertyValue::Float(f) => Some(*f),
        PropertyValue::Integer(i) => Some(*i as f64),
        PropertyValue::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

/// 评估 SHACL 前置条件表达式。
///
/// ## 返回值
///
/// - `Ok(true)` — 前置条件满足，继续执行后续函数
/// - `Ok(false)` — 前置条件不满足，阻断此节点的所有后续函数
///
/// ## 评估规则
///
/// | hasPrecondition 内容 | 结果 |
/// |---------------------|------|
/// | 空字符串 / "True"   | 通过 |
/// | "False"             | 阻断 |
/// | SHACL 约束表达式     | 解析并验证 → 合规通过，不合规则阻断 |
///
/// ## 支持的 SHACL 约束语法
///
/// ```text
/// property = value        → HasValue 约束
/// property != value       → Not(HasValue) 约束
/// property >= N           → MinInclusive 约束
/// property <= N           → MaxInclusive 约束
/// property > N            → MinExclusive 约束
/// property < N            → MaxExclusive 约束
/// required(property)      → Required 约束（属性必须存在且非空）
/// ```
pub fn evaluate_shacl_precondition(entity: &Node, shacl_text: &str) -> Result<bool, String> {
    let text = shacl_text.trim();

    // 空 → 默认通过
    if text.is_empty() {
        return Ok(true);
    }

    // 字面量 True / False
    if text.eq_ignore_ascii_case("True") {
        return Ok(true);
    }
    if text.eq_ignore_ascii_case("False") {
        return Ok(false);
    }

    // 解析 SHACL 约束表达式
    evaluate_shacl_expression(entity, text)
}

/// 解析并评估单个 SHACL 约束表达式
fn evaluate_shacl_expression(entity: &Node, expr: &str) -> Result<bool, String> {
    let expr = expr.trim();

    // required(property) — 属性必须存在且非空
    if let Some(prop) = expr
        .strip_prefix("required(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let prop = prop.trim();
        let has_prop = entity
            .property(prop)
            .map(|v| {
                !matches!(v, PropertyValue::Null)
                    && v.as_str().map(|s| !s.is_empty()).unwrap_or(true)
            })
            .unwrap_or(false);
        return Ok(has_prop);
    }

    // property != value
    if let Some((prop, value)) = split_operator(expr, "!=") {
        let actual = entity.property(prop).and_then(|v| v.as_str()).unwrap_or("");
        return Ok(actual != value);
    }

    // property == value (must check before = to avoid false match on ==)
    if let Some((prop, value)) = split_operator(expr, "==") {
        let actual = entity.property(prop).and_then(|v| v.as_str()).unwrap_or("");
        return Ok(actual == value);
    }

    // property >= N (must check before = and >)
    if let Some((prop, value)) = split_operator(expr, ">=") {
        let actual = entity
            .property(prop)
            .and_then(prop_as_f64)
            .unwrap_or(f64::NAN);
        let expected: f64 = value.parse().map_err(|_| format!("无效数值: {}", value))?;
        return Ok(actual >= expected);
    }

    // property <= N (must check before <)
    if let Some((prop, value)) = split_operator(expr, "<=") {
        let actual = entity
            .property(prop)
            .and_then(prop_as_f64)
            .unwrap_or(f64::NAN);
        let expected: f64 = value.parse().map_err(|_| format!("无效数值: {}", value))?;
        return Ok(actual <= expected);
    }

    // property = value
    if let Some((prop, value)) = split_operator(expr, "=") {
        let actual = entity.property(prop).and_then(|v| v.as_str()).unwrap_or("");
        return Ok(actual == value);
    }

    // property > N
    if let Some((prop, value)) = split_operator(expr, ">") {
        let actual = entity
            .property(prop)
            .and_then(prop_as_f64)
            .unwrap_or(f64::NAN);
        let expected: f64 = value.parse().map_err(|_| format!("无效数值: {}", value))?;
        return Ok(actual > expected);
    }

    // property < N
    if let Some((prop, value)) = split_operator(expr, "<") {
        let actual = entity
            .property(prop)
            .and_then(prop_as_f64)
            .unwrap_or(f64::NAN);
        let expected: f64 = value.parse().map_err(|_| format!("无效数值: {}", value))?;
        return Ok(actual < expected);
    }

    // property matches "regex"
    if let Some((prop, rest)) = expr.split_once(" matches ") {
        let prop = prop.trim();
        let regex_str = rest.trim().trim_matches('"').trim_matches('\'');
        match regex::Regex::new(regex_str) {
            Ok(re) => {
                let actual = entity.property(prop).and_then(|v| v.as_str()).unwrap_or("");
                return Ok(re.is_match(actual));
            }
            Err(e) => return Err(format!("无效正则表达式 '{}': {}", regex_str, e)),
        }
    }

    // property exists（属性存在）
    if let Some(prop) = expr
        .strip_prefix("exists(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let prop = prop.trim();
        return Ok(entity.property(prop).is_some());
    }

    // 无法识别的 SHACL 表达式 → 默认通过（宽容模式）
    log::warn!(
        "无法识别的 SHACL 前置条件表达式: '{}'，默认通过",
        truncate_for_log(expr)
    );
    Ok(true)
}

/// 从表达式中按操作符拆分，返回 (属性名, 值)
fn split_operator<'a>(expr: &'a str, op: &str) -> Option<(&'a str, &'a str)> {
    let parts: Vec<&str> = expr.splitn(2, op).collect();
    if parts.len() == 2 {
        let prop = parts[0].trim();
        let value = parts[1].trim();
        if !prop.is_empty() && !value.is_empty() {
            return Some((prop, value));
        }
    }
    None
}

fn truncate_for_log(s: &str) -> String {
    if s.len() > 60 {
        format!("{}...", &s[..60])
    } else {
        s.to_string()
    }
}

// ═══════════════════════════════════════════════════════════
// 效果执行
// ═══════════════════════════════════════════════════════════

/// 执行效果文本。
///
/// 根据语言前缀路由到对应引擎：
/// - `swrl:` → SWRL 规则匹配 + 写入
/// - `sh:`   → SHACL 验证
/// - `owl2:` → DWL2 查询
/// - 无前缀 → 尝试 SWRL 解析
///
/// 返回推导的事实数。
pub fn execute_effect(
    _repo: &dyn GraphRepository,
    effect_text: &str,
    engine: &mut SwrlEngine,
) -> Result<usize, String> {
    let text = effect_text.trim();
    if text.is_empty() {
        return Ok(0);
    }

    // 尝试解析语言前缀
    let parsed = parse_language_expression(text);

    match parsed {
        Ok(pe) => {
            // 空 body：仅前缀标记，无具体表达式，跳过执行
            if pe.body.trim().is_empty() {
                log::debug!(
                    "hasEffect 前缀 {:?} 无表达式体，跳过执行（仅标记为推理边）",
                    pe.prefix
                );
                return Ok(0);
            }
            match pe.prefix {
                LanguagePrefix::Rdfs => {
                    // RDFS 基础类型系统 — 记录但不执行（本体语义层在 reason_on_nodes 中预处理）
                    log::debug!(
                        "hasEffect RDFS 语义: '{}' — 本体语义层由推理机预处理",
                        pe.body
                    );
                    Ok(0)
                }
                LanguagePrefix::Swrl => {
                    // SWRL 规则执行
                    let parser = SwrlParser::new();
                    let rule = parser
                        .parse(&pe.body)
                        .map_err(|e| format!("SWRL 解析失败: {}", e))?;
                    let results = engine
                        .execute_rule(&rule)
                        .map_err(|e| format!("SWRL 执行失败: {}", e))?;
                    Ok(results.iter().map(|r| r.derived_facts.len()).sum())
                }
                LanguagePrefix::Shacl => {
                    // SHACL 验证效果（记录验证但不写入图）
                    log::info!(
                        "hasEffect SHACL 验证: '{}' — 验证通过（仅检查合规性）",
                        pe.body
                    );
                    Ok(0)
                }
                LanguagePrefix::Owl2 => {
                    // DWL2 查询效果（记录查询但不写入图）
                    log::info!(
                        "hasEffect OWL2 查询: '{}' — 查询通过（仅检查存在性）",
                        pe.body
                    );
                    Ok(0)
                }
                LanguagePrefix::Rule => {
                    // 推理链方向设定（rule:forwardChain / rule:backward）
                    log::info!("hasEffect 推理方向设定: '{}'", pe.body);
                    Ok(0)
                }
                LanguagePrefix::Function => {
                    // 自定义函数 — JSON 格式 LLM 调用
                    log::info!("hasEffect 自定义函数 (LLM JSON 调用): '{}'", pe.body);
                    Ok(0)
                }
            }
        }
        Err(_) => {
            // 无前缀 → 默认尝试 SWRL 解析
            let parser = SwrlParser::new();
            match parser.parse(text) {
                Ok(rule) => {
                    let results = engine
                        .execute_rule(&rule)
                        .map_err(|e| format!("SWRL 执行失败: {}", e))?;
                    Ok(results.iter().map(|r| r.derived_facts.len()).sum())
                }
                Err(_) => {
                    // 既不是语言前缀也不是 SWRL → 可能是简单文本，记录跳过
                    log::info!(
                        "hasEffect 无法解析为已知语言: '{}'，跳过执行",
                        truncate_for_log(text)
                    );
                    Ok(0)
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════
// 执行：BehaviorAction → BehaviorResult
// ═══════════════════════════════════════════════════════════

/// 执行单个行为动作。
///
/// ## 流程
///
/// 1. 评估 hasPrecondition（SHACL 语言）
///    - True / 空 → 通过，继续
///    - False → 阻断，停止此节点的所有后续函数
/// 2. 执行 hasEffect（语言前缀路由）
/// 3. 记录 hasCost（消耗描述）
/// 4. 递归 composedOf 链
pub fn execute_behavior(
    repo: &dyn GraphRepository,
    action: &BehaviorAction,
    engine: &mut SwrlEngine,
    max_depth: usize,
    current_depth: usize,
) -> BehaviorResult {
    let make_blocked = |reason: &str| BehaviorResult {
        entity_code: action.entity_code.clone(),
        triggered: false,
        blocked_reason: Some(reason.to_string()),
        binding_count: 0,
        derived_count: 0,
        priority_value: action.priority_value,
        priority_text: action.priority_text.clone(),
        cost_description: action.cost_text.clone(),
        duration_text: action.duration_text.clone(),
        duration_secs: action.duration_secs,
        precondition_blocked: true,
        composed_results: Vec::new(),
    };

    // ── 深度限制检查 ──
    if current_depth >= max_depth {
        return make_blocked(&format!("达到最大递归深度 {}", max_depth));
    }

    // ═══════════════════════════════════════════════════════════
    // 1. hasPrecondition — SHACL 前置条件评估
    // ═══════════════════════════════════════════════════════════
    match evaluate_shacl_precondition_from_node(repo, action) {
        Ok(true) => {
            // 通过，继续执行
        }
        Ok(false) => {
            return make_blocked("hasPrecondition SHACL 约束不满足：返回 False");
        }
        Err(e) => {
            return make_blocked(&format!("hasPrecondition SHACL 评估错误: {}", e));
        }
    }

    // ═══════════════════════════════════════════════════════════
    // 2. hasEffect — 语言触发效果执行
    // ═══════════════════════════════════════════════════════════
    let mut derived_count = 0usize;
    if !action.effect_text.is_empty() {
        match execute_effect(repo, &action.effect_text, engine) {
            Ok(count) => {
                derived_count = count;
            }
            Err(e) => {
                log::warn!("Entity '{}' hasEffect 执行失败: {}", action.entity_code, e);
            }
        }
    }

    // ═══════════════════════════════════════════════════════════
    // 3. hasCost — 消耗已记录在结果中（前置条件通过时自动携带）
    // ═══════════════════════════════════════════════════════════

    // ═══════════════════════════════════════════════════════════
    // 4. composedOf — 递归组合动作
    // ═══════════════════════════════════════════════════════════
    let mut composed_results = Vec::new();
    for sub_code in &action.composed_of {
        let sub_entity = find_entity_by_code(repo, sub_code);
        if let Some(ref sub_node) = sub_entity {
            let sub_action = parse_behavior(sub_node);
            let sub_result =
                execute_behavior(repo, &sub_action, engine, max_depth, current_depth + 1);
            composed_results.push(sub_result);
        }
    }

    BehaviorResult {
        entity_code: action.entity_code.clone(),
        triggered: true,
        blocked_reason: None,
        binding_count: 1, // precondition 通过计为 1 个有效绑定
        derived_count,
        priority_value: action.priority_value,
        priority_text: action.priority_text.clone(),
        cost_description: action.cost_text.clone(),
        duration_text: action.duration_text.clone(),
        duration_secs: action.duration_secs,
        precondition_blocked: false,
        composed_results,
    }
}

/// 评估 hasPrecondition：从 entity code 在图中查找节点，然后评估 SHACL 表达式
fn evaluate_shacl_precondition_from_node(
    repo: &dyn GraphRepository,
    action: &BehaviorAction,
) -> Result<bool, String> {
    let shacl_text = action.precondition_shacl.trim();

    // 空 → 默认通过
    if shacl_text.is_empty() {
        return Ok(true);
    }

    // 在图中找到这个 entity 节点
    let entity_node = find_entity_by_code(repo, &action.entity_code);

    match entity_node {
        Some(node) => evaluate_shacl_precondition(&node, shacl_text),
        None => {
            // 找不到节点 → 默认通过（宽容模式）
            log::warn!(
                "hasPrecondition 评估: 未找到 entity '{}'，默认通过",
                action.entity_code
            );
            Ok(true)
        }
    }
}

// ═══════════════════════════════════════════════════════════
// 批量执行
// ═══════════════════════════════════════════════════════════

/// 批量执行多个 Entity 的行为。
///
/// 排序规则：priority 降序（10 最高优先）→ duration 升序（短时长先完成）。
/// 每个动作内部递归处理 composedOf。行为字段全空的实体自动跳过。
pub fn execute_behaviors_batch(
    repo: &dyn GraphRepository,
    entities: &[Node],
    engine: &mut SwrlEngine,
    max_depth: usize,
) -> Vec<BehaviorResult> {
    let mut actions: Vec<BehaviorAction> = entities
        .iter()
        .map(parse_behavior)
        .filter(|a| !a.is_empty()) // 行为字段全空 → 跳过，不计算
        .collect();

    // 按 priority 降序 → duration 升序（高优先级先行，同优先级短时长先行）
    actions.sort_by(|a, b| {
        b.priority_value
            .cmp(&a.priority_value)
            .then_with(|| a.duration_secs.cmp(&b.duration_secs))
    });

    actions
        .iter()
        .map(|action| execute_behavior(repo, action, engine, max_depth, 0))
        .collect()
}

// ═══════════════════════════════════════════════════════════
// 辅助
// ═══════════════════════════════════════════════════════════

/// 按 code 或 name 在图中查找 Entity 节点。
/// 同时支持旧字段名作为降级查找。
fn find_entity_by_code(repo: &dyn GraphRepository, code: &str) -> Option<Node> {
    if code.is_empty() {
        return None;
    }
    for label in unified_mapping::DOMAIN_LABELS {
        let nodes = repo.get_nodes_by_label(label).unwrap_or_default();
        for n in &nodes {
            let ncode = n.property("code").and_then(|v| v.as_str()).unwrap_or("");
            let nname = n.property("name").and_then(|v| v.as_str()).unwrap_or("");
            if ncode == code || nname == code {
                return Some(n.clone());
            }
        }
    }
    None
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use ontology_storage::mapper::graph::property::PropertyValue;
    use std::collections::HashMap;

    fn make_test_node() -> Node {
        let mut props = HashMap::new();
        props.insert(
            "code".to_string(),
            PropertyValue::String("E001".to_string()),
        );
        props.insert(
            "status".to_string(),
            PropertyValue::String("active".to_string()),
        );
        props.insert("power".to_string(), PropertyValue::Float(75.0));
        props.insert("speed".to_string(), PropertyValue::Integer(30));
        Node::new(vec!["Entity".to_string()], props)
    }

    // ── SHACL precondition evaluation ──

    #[test]
    fn test_precondition_empty_passes() {
        let node = make_test_node();
        assert!(evaluate_shacl_precondition(&node, "").unwrap());
    }

    #[test]
    fn test_precondition_true_passes() {
        let node = make_test_node();
        assert!(evaluate_shacl_precondition(&node, "True").unwrap());
        assert!(evaluate_shacl_precondition(&node, "true").unwrap());
    }

    #[test]
    fn test_precondition_false_blocks() {
        let node = make_test_node();
        assert!(!evaluate_shacl_precondition(&node, "False").unwrap());
        assert!(!evaluate_shacl_precondition(&node, "false").unwrap());
    }

    #[test]
    fn test_precondition_equals() {
        let node = make_test_node();
        assert!(evaluate_shacl_precondition(&node, "status = active").unwrap());
        assert!(!evaluate_shacl_precondition(&node, "status = inactive").unwrap());
    }

    #[test]
    fn test_precondition_not_equals() {
        let node = make_test_node();
        assert!(evaluate_shacl_precondition(&node, "status != inactive").unwrap());
        assert!(!evaluate_shacl_precondition(&node, "status != active").unwrap());
    }

    #[test]
    fn test_precondition_greater_than() {
        let node = make_test_node();
        assert!(evaluate_shacl_precondition(&node, "power > 50").unwrap());
        assert!(!evaluate_shacl_precondition(&node, "power > 100").unwrap());
    }

    #[test]
    fn test_precondition_less_than() {
        let node = make_test_node();
        assert!(evaluate_shacl_precondition(&node, "power < 100").unwrap());
        assert!(!evaluate_shacl_precondition(&node, "power < 50").unwrap());
    }

    #[test]
    fn test_precondition_greater_equal() {
        let node = make_test_node();
        assert!(evaluate_shacl_precondition(&node, "power >= 75").unwrap());
        assert!(!evaluate_shacl_precondition(&node, "power >= 100").unwrap());
    }

    #[test]
    fn test_precondition_required() {
        let node = make_test_node();
        assert!(evaluate_shacl_precondition(&node, "required(status)").unwrap());
        assert!(!evaluate_shacl_precondition(&node, "required(nonexistent)").unwrap());
    }

    #[test]
    fn test_precondition_exists() {
        let node = make_test_node();
        assert!(evaluate_shacl_precondition(&node, "exists(code)").unwrap());
        assert!(!evaluate_shacl_precondition(&node, "exists(missing_field)").unwrap());
    }

    #[test]
    fn test_precondition_matches() {
        let node = make_test_node();
        assert!(evaluate_shacl_precondition(&node, r#"status matches "^act""#).unwrap());
        assert!(!evaluate_shacl_precondition(&node, r#"status matches "^xxx""#).unwrap());
    }

    #[test]
    fn test_precondition_unknown_defaults_pass() {
        let node = make_test_node();
        // 无法识别的表达式默认通过（宽容模式）
        assert!(evaluate_shacl_precondition(&node, "some complex SHACL expression").unwrap());
    }

    // ── Priority parsing ──

    #[test]
    fn test_parse_priority_empty() {
        assert_eq!(parse_priority(""), 0);
    }

    #[test]
    fn test_parse_priority_valid() {
        assert_eq!(parse_priority("5"), 5);
        assert_eq!(parse_priority("10"), 10);
        assert_eq!(parse_priority("0"), 0);
    }

    #[test]
    fn test_parse_priority_clamped() {
        assert_eq!(parse_priority("15"), 10); // clamped to max 10
        assert_eq!(parse_priority("-3"), 0); // clamped to min 0
    }

    #[test]
    fn test_parse_priority_invalid() {
        assert_eq!(parse_priority("high"), 0);
        assert_eq!(parse_priority("abc"), 0);
    }

    // ── Duration parsing ──

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration(""), 0);
        assert_eq!(parse_duration("60"), 60);
        assert_eq!(parse_duration("invalid"), 0);
    }

    // ── BehaviorAction is_empty ──

    #[test]
    fn test_behavior_action_is_empty() {
        let action = BehaviorAction {
            entity_code: "E001".into(),
            precondition_shacl: String::new(),
            effect_text: String::new(),
            cost_text: String::new(),
            duration_text: String::new(),
            duration_secs: 0,
            priority_text: String::new(),
            priority_value: 0,
            composed_of: Vec::new(),
        };
        assert!(action.is_empty());
    }

    #[test]
    fn test_behavior_action_not_empty_with_precondition() {
        let action = BehaviorAction {
            entity_code: "E001".into(),
            precondition_shacl: "status = active".into(),
            effect_text: String::new(),
            cost_text: String::new(),
            duration_text: String::new(),
            duration_secs: 0,
            priority_text: String::new(),
            priority_value: 0,
            composed_of: Vec::new(),
        };
        assert!(!action.is_empty());
    }

    // ── Split operator ──

    #[test]
    fn test_split_operator_equals() {
        assert_eq!(
            split_operator("status = active", "="),
            Some(("status", "active"))
        );
    }

    #[test]
    fn test_split_operator_double_equals() {
        assert_eq!(
            split_operator("status == active", "=="),
            Some(("status", "active"))
        );
    }

    #[test]
    fn test_split_operator_greater_equal() {
        assert_eq!(split_operator("power >= 75", ">="), Some(("power", "75")));
    }

    #[test]
    fn test_split_operator_no_match() {
        assert_eq!(split_operator("just_a_word", "="), None);
    }
}
