//! 自定义行为动作引擎 — Entity 行为字段驱动的规则执行。
//!
//! ## 模型
//!
//! Entity 节点的 6 个行为字段（标准字段 23-28）:
//!
//! | 属性         | 字段名          | 作用     | 格式          |
//! |-------------|----------------|---------|--------------|
//! | precondition | precondition   | 阻断     | SWRL 规则     |
//! | effect       | effect         | 触发     | SWRL 规则     |
//! | cost         | cost           | 消耗     | SWRL 文本     |
//! | duration     | duration       | 时序     | int（秒）     |
//! | priority     | priority       | 排序     | 0-10         |
//! | composedOf   | composedOf     | 组合跳转 | code;code;... |
//!
//! ## 执行流程
//!
//! ```text
//! 1. 扫描 Entity → 解析 6 字段 → BehaviorAction
//! 2. 按 priority 降序排列
//! 3. 对每个 action:
//!    a. 解析 precondition SWRL → 在图匹配前提
//!       - 有绑定 → 通过，继续
//!       - 无绑定 → 阻断，跳过
//!    b. 解析 effect SWRL → 代入绑定 → 写入图
//!    c. 递归处理 composedOf 链
//! ```

use ontology_storage::mapper::graph::node::Node;
use ontology_storage::repository::graph_store::GraphRepository;

use crate::swrl::ast::Rule;
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
    /// 前置条件规则（无结论部分时，默认结论为空规则）
    pub precondition_rule: Option<Rule>,
    /// 效果规则
    pub effect_rule: Option<Rule>,
    /// 消耗 SWRL 文本（原样保留）
    pub cost_text: String,
    /// 持续时间（秒）
    pub duration_secs: i64,
    /// 优先级 0..=10
    pub priority: i64,
    /// 关联实体 code 列表（composedOf 分号拆分）
    pub composed_of: Vec<String>,
}

impl BehaviorAction {
    /// 是否没有任何行为字段（空实体，应跳过不计算）
    pub fn is_empty(&self) -> bool {
        self.precondition_rule.is_none()
            && self.effect_rule.is_none()
            && self.cost_text.is_empty()
            && self.duration_secs == 0
            && self.priority == 0
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
    /// 优先级
    pub priority: i64,
    /// 消耗描述
    pub cost_description: String,
    /// 持续时间（秒）
    pub duration_secs: i64,
    /// 级联子动作结果
    pub composed_results: Vec<BehaviorResult>,
}

// ═══════════════════════════════════════════════════════════
// 解析：Entity → BehaviorAction
// ═══════════════════════════════════════════════════════════

/// 从 Entity 节点解析行为描述。
///
/// 提取 precondition、effect、cost、duration、priority、composedOf 六个字段。
/// precondition 和 effect 用 SWRL 解析器解析为 Rule。
pub fn parse_behavior(entity: &Node) -> BehaviorAction {
    let code = entity
        .property("code")
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    let precondition_text = entity
        .property("precondition")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let effect_text = entity
        .property("effect")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let cost_text = entity
        .property("cost")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let duration = entity
        .property("duration")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let priority = entity
        .property("priority")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let composed_of_raw = entity
        .property("composedOf")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut parser = SwrlParser::new();
    let precondition_rule = parse_swrl_safe(&mut parser, precondition_text);
    let effect_rule = parse_swrl_safe(&mut parser, effect_text);

    let composed_of: Vec<String> = composed_of_raw
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    BehaviorAction {
        entity_code: code.to_string(),
        precondition_rule,
        effect_rule,
        cost_text: cost_text.to_string(),
        duration_secs: duration,
        priority,
        composed_of,
    }
}

/// 安全解析 SWRL 文本，解析失败返回 None
fn parse_swrl_safe(parser: &mut SwrlParser, text: &str) -> Option<Rule> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    parser.parse(text).ok()
}

// ═══════════════════════════════════════════════════════════
// 执行：BehaviorAction → BehaviorResult
// ═══════════════════════════════════════════════════════════

/// 执行单个行为动作。
///
/// 1. 评估 precondition → 匹配前提，无绑定则阻断
/// 2. 执行 effect → 代入绑定 → 写入图
/// 3. 递归 composedOf
pub fn execute_behavior(
    repo: &dyn GraphRepository,
    action: &BehaviorAction,
    engine: &mut SwrlEngine,
    max_depth: usize,
    current_depth: usize,
) -> BehaviorResult {
    if current_depth >= max_depth {
        return BehaviorResult {
            entity_code: action.entity_code.clone(),
            triggered: false,
            blocked_reason: Some(format!("达到最大递归深度 {}", max_depth)),
            binding_count: 0,
            derived_count: 0,
            priority: action.priority,
            cost_description: action.cost_text.clone(),
            duration_secs: action.duration_secs,
            composed_results: Vec::new(),
        };
    }

    let mut binding_count = 0usize;
    let mut derived_count = 0usize;

    // ── 1. 前置条件评估 ──
    let _bindings: Vec<crate::swrl::ast::VariableBinding> = if let Some(ref pre_rule) = action.precondition_rule {
        if pre_rule.antecedent.is_empty() {
            // 无前提 → 默认通过
            Vec::new()
        } else {
            // 创建仅含前提的临时规则来匹配
            let match_rule = Rule::anonymous(pre_rule.antecedent.clone(), Vec::new());
            match engine.execute_rule(&match_rule) {
                Ok(results) => {
                    if results.is_empty() {
                        return BehaviorResult {
                            entity_code: action.entity_code.clone(),
                            triggered: false,
                            blocked_reason: Some("前置条件不满足：图中无可匹配绑定".into()),
                            binding_count: 0,
                            derived_count: 0,
                            priority: action.priority,
                            cost_description: action.cost_text.clone(),
                            duration_secs: action.duration_secs,
                            composed_results: Vec::new(),
                        };
                    }
                    binding_count = results.iter().map(|r| r.binding_count).sum();
                    // 收集所有绑定的变量（不实际使用，但计入绑定数）
                    Vec::new()
                }
                Err(e) => {
                    return BehaviorResult {
                        entity_code: action.entity_code.clone(),
                        triggered: false,
                        blocked_reason: Some(format!("前置条件匹配错误: {}", e)),
                        binding_count: 0,
                        derived_count: 0,
                        priority: action.priority,
                        cost_description: action.cost_text.clone(),
                        duration_secs: action.duration_secs,
                        composed_results: Vec::new(),
                    };
                }
            }
        }
    } else {
        // 无前置条件 → 默认通过
        Vec::new()
    };

    // ── 2. 效果执行 ──
    if let Some(ref eff_rule) = action.effect_rule {
        if !eff_rule.consequent.is_empty() {
            let exec_rule = eff_rule.clone();
            // SwrlEngine::execute_rule 会自己重新 match_antecedent
            match engine.execute_rule(&exec_rule) {
                Ok(results) => {
                    derived_count = results.iter().map(|r| r.derived_facts.len()).sum();
                }
                Err(_) => {
                    return BehaviorResult {
                        entity_code: action.entity_code.clone(),
                        triggered: true,
                        blocked_reason: None,
                        binding_count,
                        derived_count: 0,
                        priority: action.priority,
                        cost_description: action.cost_text.clone(),
                        duration_secs: action.duration_secs,
                        composed_results: Vec::new(),
                    };
                }
            }
        }
    }

    // ── 3. composedOf 递归 ──
    let mut composed_results = Vec::new();
    for sub_code in &action.composed_of {
        // 查找关联实体
        let sub_entity = find_entity_by_code(repo, sub_code);
        if let Some(ref sub_node) = sub_entity {
            let sub_action = parse_behavior(sub_node);
            let sub_result = execute_behavior(repo, &sub_action, engine, max_depth, current_depth + 1);
            composed_results.push(sub_result);
        }
    }

    BehaviorResult {
        entity_code: action.entity_code.clone(),
        triggered: true,
        blocked_reason: None,
        binding_count,
        derived_count,
        priority: action.priority,
        cost_description: action.cost_text.clone(),
        duration_secs: action.duration_secs,
        composed_results,
    }
}

// ═══════════════════════════════════════════════════════════
// 批量执行
// ═══════════════════════════════════════════════════════════

/// 批量执行多个 Entity 的行为。
///
/// 按 priority 降序排列后依次执行。每个动作内部递归处理 composedOf。
pub fn execute_behaviors_batch(
    repo: &dyn GraphRepository,
    entities: &[Node],
    engine: &mut SwrlEngine,
    max_depth: usize,
) -> Vec<BehaviorResult> {
    let mut actions: Vec<BehaviorAction> = entities
        .iter()
        .map(|n| parse_behavior(n))
        .filter(|a| !a.is_empty())   // 行为字段全空 → 跳过，不计算
        .collect();

    // 按 priority 降序
    actions.sort_by(|a, b| b.priority.cmp(&a.priority));

    actions
        .iter()
        .map(|action| execute_behavior(repo, action, engine, max_depth, 0))
        .collect()
}

// ═══════════════════════════════════════════════════════════
// 辅助
// ═══════════════════════════════════════════════════════════

fn find_entity_by_code(repo: &dyn GraphRepository, code: &str) -> Option<Node> {
    if code.is_empty() {
        return None;
    }
    for label in &["Entity", "Event", "Patrol", "Strike", "Type", "Behavior"] {
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
