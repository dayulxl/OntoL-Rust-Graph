//! 自定义逻辑函数模块 — 可插拔的推理动作处理。
//!
//! ## 设计
//!
//! 业务方通过实现 [`ActionFunction`] trait 注入自定义逻辑。
//! 在 SWRL 规则推导出新事实后，匹配的动作函数被调用，
//! 可以执行图写入、属性更新、外部通知等任意操作。
//!
//! ## 与 BuiltinRegistry 的区别
//!
//! | 维度       | BuiltinRegistry          | ActionRegistry             |
//! |-----------|--------------------------|----------------------------|
//! | 用途       | 前提过滤（true/false）    | 推导后的副作用操作          |
//! | 调用时机   | 规则匹配阶段              | 规则结论写入后              |
//! | 返回值     | bool / Deferred          | Result<(), String>         |
//! | 访问权限   | 只读参数                  | 读写图仓库                  |
//!
//! ## 示例
//!
//! ```rust,ignore
//! use ontology_reasoner::graph::actions::{ActionFunction, ActionContext, ActionRegistry};
//!
//! struct MyDetector;
//! impl ActionFunction for MyDetector {
//!     fn execute(&self, ctx: &mut ActionContext) -> Result<(), String> {
//!         // 当推导出 hasUncle 关系时，自动更新置信度
//!         if ctx.rule_name.contains("uncle") {
//!             ctx.repo.update_entity_properties(...)?;
//!         }
//!         Ok(())
//!     }
//!     fn name(&self) -> &str { "my_detector" }
//! }
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use ontology_storage::mapper::graph::node::Node;
use ontology_storage::mapper::graph::property::PropertyValue;
use ontology_storage::repository::graph_store::GraphRepository;

// ═══════════════════════════════════════════════════════════
// ActionContext — 动作执行的运行时上下文
// ═══════════════════════════════════════════════════════════

/// 动作执行上下文。
///
/// 在推理引擎推导出新事实后，将上下文传给所有注册的动作函数。
pub struct ActionContext<'a> {
    /// 图仓库句柄（读写权限）
    pub repo: &'a dyn GraphRepository,

    /// 触发的规则名
    pub rule_name: &'a str,

    /// 本次推导出的新事实（以可读字符串表示）
    pub derived_facts: Vec<DerivedFact>,

    /// 置信度
    pub confidence: f64,

    /// 变量绑定（变量名 → 个体 IRI）
    pub bindings: Vec<HashMap<String, String>>,
}

/// 推导出的单条事实
#[derive(Debug, Clone)]
pub struct DerivedFact {
    /// 事实类型: "ClassAtom" | "ObjectPropertyAtom" | "DataPropertyAtom"
    pub fact_type: String,
    /// 主体 IRI
    pub subject: String,
    /// 属性/类 IRI
    pub predicate: String,
    /// 客体 IRI（ObjectPropertyAtom / ClassAtom 时有值）
    pub object: Option<String>,
    /// 数据值（DataPropertyAtom 时有值）
    pub value: Option<String>,
}

// ═══════════════════════════════════════════════════════════
// ActionFunction trait
// ═══════════════════════════════════════════════════════════

/// 自定义逻辑函数 trait。
///
/// 业务方实现此 trait 注入领域特定的推理后动作。
/// 每个注册的动作函数在规则推导出新事实后被依次调用。
pub trait ActionFunction: Send + Sync {
    /// 执行自定义动作。
    ///
    /// # 参数
    ///
    /// - `ctx` — 运行时上下文，包含图仓库句柄、推导事实、变量绑定等
    ///
    /// # 返回
    ///
    /// - `Ok(())` — 动作执行成功
    /// - `Err(String)` — 动作执行失败（触发推理中断）
    fn execute(&self, ctx: &mut ActionContext) -> Result<(), String>;

    /// 动作函数名称（用于日志和调试）
    fn name(&self) -> &str;
}

// ═══════════════════════════════════════════════════════════
// ActionRegistry — 动作函数注册表
// ═══════════════════════════════════════════════════════════

/// 动作函数注册表。
///
/// 管理已注册的 [`ActionFunction`] 实现，提供注册、查找和批量执行功能。
pub struct ActionRegistry {
    functions: Vec<Box<dyn ActionFunction>>,
}

impl ActionRegistry {
    /// 创建空注册表
    pub fn new() -> Self {
        Self { functions: Vec::new() }
    }

    /// 注册一个动作函数
    pub fn register(&mut self, func: Box<dyn ActionFunction>) {
        self.functions.push(func);
    }

    /// 获取已注册的函数数量
    pub fn len(&self) -> usize {
        self.functions.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }

    /// 对给定上下文依次执行所有已注册的动作函数。
    ///
    /// 按注册顺序依次调用。任一函数返回 `Err` 将立即中断后续执行。
    ///
    /// 返回执行的动作数和首个错误。
    pub fn execute_all(&self, ctx: &mut ActionContext) -> Result<usize, (usize, String)> {
        let mut executed = 0usize;
        for func in &self.functions {
            match func.execute(ctx) {
                Ok(()) => executed += 1,
                Err(e) => return Err((executed, format!("[{}] {}", func.name(), e))),
            }
        }
        Ok(executed)
    }
}

impl Default for ActionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════
// 上下文构造辅助
// ═══════════════════════════════════════════════════════════

impl<'a> ActionContext<'a> {
    /// 创建新的动作上下文
    pub fn new(
        repo: &'a dyn GraphRepository,
        rule_name: &'a str,
        confidence: f64,
    ) -> Self {
        Self {
            repo,
            rule_name,
            derived_facts: Vec::new(),
            confidence,
            bindings: Vec::new(),
        }
    }

    /// 添加一条推导事实
    pub fn add_fact(&mut self, fact: DerivedFact) {
        self.derived_facts.push(fact);
    }

    /// 添加一个变量绑定
    pub fn add_binding(&mut self, binding: HashMap<String, String>) {
        self.bindings.push(binding);
    }

    /// 在图中查找实体
    pub fn find_entity(&self, code: &str) -> Option<Node> {
        for label in &["Entity", "Event", "Patrol", "Strike", "Type", "Behavior"] {
            let nodes = self.repo.get_nodes_by_label(label).ok()?;
            for n in &nodes {
                if n.property("code").and_then(|v| v.as_str()) == Some(code) {
                    return Some(n.clone());
                }
            }
        }
        None
    }

    /// 更新实体属性（delete + insert 方式）
    pub fn update_entity(&self, code: &str, updates: HashMap<String, PropertyValue>) -> Result<(), String> {
        let entity = self.find_entity(code)
            .ok_or_else(|| format!("实体 '{}' 未找到", code))?;

        let mut new_props = entity.properties.clone();
        for (k, v) in updates {
            new_props.insert(k, v);
        }

        self.repo.delete_node(code)
            .map_err(|e| format!("删除失败: {}", e))?;
        let updated = Node::new(entity.labels, new_props);
        self.repo.insert_node(&updated)
            .map_err(|e| format!("写入失败: {}", e))?;
        Ok(())
    }
}
