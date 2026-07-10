//! 通用多跳图遍历引擎。
//!
//! `GraphExplorer` 提供与领域无关的 BFS 图遍历能力。
//! 状态变化检测通过 [`StateChangeDetector`](super::detector::StateChangeDetector) trait
//! 由业务层注入。

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use ontology_storage::mapper::graph::node::Node;
use ontology_storage::repository::graph_store::GraphRepository;

use super::util::{self, RelCount, RuleMatch};

/// 共享仓库类型
pub type SharedRepo = Arc<dyn GraphRepository>;

// ═══════════════════════════════════════════════════════════
// 配置与枚举
// ═══════════════════════════════════════════════════════════

/// 遍历方向
#[derive(Debug, Clone, Default)]
pub enum Direction {
    /// 仅跟随出向边
    #[default]
    Outgoing,
    /// 仅跟随入向边
    Incoming,
    /// 出向 + 入向
    Both,
}

impl std::str::FromStr for Direction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "incoming" => Ok(Self::Incoming),
            "both" => Ok(Self::Both),
            _ => Ok(Self::Outgoing),
        }
    }
}

impl Direction {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Outgoing => "outgoing",
            Self::Incoming => "incoming",
            Self::Both => "both",
        }
    }
}

/// 遍历配置
pub struct ExploreConfig {
    /// 起始实体 ID（按 id/iri 检索）
    pub start_id: String,
    /// 起始实体 code（按 code 检索），与 start_id 同时传入时两者都匹配才命中
    pub start_code: Option<String>,
    /// 要跟随的关系类型（必填）
    pub relation: String,
    /// 最大遍历深度（默认 3，最大 5）
    pub max_depth: usize,
    /// 遍历方向（默认 outgoing）
    pub direction: Direction,
    /// 全局置信度阈值。传播时读取目标节点的 confidence 属性：
    /// - 无此属性 → 不阻断，继续传播
    /// - 有值且 ≥ 阈值 → 继续传播
    /// - 有值且 < 阈值 → 停止从这个节点继续传播
    pub confidence_threshold: Option<f64>,
    /// 副本版本号：遍历过程中遇到 Entity 节点时，
    /// 如果节点的 cope_version 不匹配，则克隆副本
    pub cope_version: Option<String>,
}

// ═══════════════════════════════════════════════════════════
// 遍历结果
// ═══════════════════════════════════════════════════════════

/// 单跳结果
#[derive(Debug, Clone)]
pub struct ExploreHop {
    /// 跳数（从 1 开始）
    pub hop: usize,
    /// 方向 "outgoing" | "incoming"
    pub direction: String,
    /// 源节点 ID
    pub source_id: String,
    /// 目标节点 ID
    pub target_id: String,
    /// 关系类型
    pub rel_type: String,
    /// 源节点（可能为 None，如果是从中间节点发起的遍历）
    pub source_node: Option<Node>,
    /// 目标节点
    pub target_node: Option<Node>,
    /// 目标节点的类型层次链（沿 subClassOf 向上）
    pub target_type_chain: Vec<String>,
    /// 匹配到的 SWRL 规则
    pub matching_rules: Vec<RuleMatch>,
    /// 目标节点的出向关系（用于预测下一步）
    pub target_outgoing: Vec<RelCount>,
    /// 目标节点自身的 confidence 属性值（None = 该节点无置信度数据）
    pub confidence: Option<f64>,
    /// 是否因低于全局阈值而停止从此节点继续传播
    pub stop_propagation: bool,
}

/// 遍历结果
pub struct ExploreResult {
    /// 起始实体
    pub source: Node,
    /// 遍历使用的关系类型
    pub relation: String,
    /// 遍历方向
    pub direction: Direction,
    /// 源实体的出向关系摘要
    pub source_outgoing: Vec<RelCount>,
    /// 源实体的入向关系摘要
    pub source_incoming: Vec<RelCount>,
    /// 遍历链（按 BFS 顺序）
    pub chain: Vec<ExploreHop>,
}

// ═══════════════════════════════════════════════════════════
// 引擎
// ═══════════════════════════════════════════════════════════

/// 通用图遍历引擎。
///
/// 对 `GraphRepository` 执行多跳 BFS 遍历，产出一组领域无关的遍历结果。
/// 业务层通过 `StateChangeDetector` trait 注入领域知识来做状态变化分析。
pub struct GraphExplorer {
    repo: SharedRepo,
}

impl GraphExplorer {
    pub fn new(repo: SharedRepo) -> Self {
        Self { repo }
    }

    /// 执行图遍历。
    ///
    /// # 步骤
    /// 1. 实体解析 — 在所有标签类型中查找起始实体
    /// 2. 源上下文 — 汇总源实体的出/入关系
    /// 3. BFS 遍历 — 按配置的深度和方向沿指定关系逐跳遍历
    /// 4. 逐跳分析 — 对每跳获取目标节点、类型层次、SWRL 规则匹配、下一步预测
    pub fn explore(&self, config: &ExploreConfig) -> Result<ExploreResult, String> {
        let max_depth = config.max_depth.clamp(1, 5);

        // ── 1. 实体解析 ──
        let source_orig = util::find_entity_by_id_code(
            self.repo.as_ref(),
            &config.start_id,
            config.start_code.as_deref(),
        )
        .ok_or_else(|| {
            let desc = if !config.start_id.is_empty() && config.start_code.is_some() {
                format!(
                    "id='{}' + code='{}'",
                    config.start_id,
                    config.start_code.as_deref().unwrap()
                )
            } else if !config.start_id.is_empty() {
                format!("id='{}'", config.start_id)
            } else {
                format!("code='{}'", config.start_code.as_deref().unwrap_or(""))
            };
            format!("实体 '{}' 未找到", desc)
        })?;

        // 副本版本：推理开始前全量克隆图中所有原实体及关系
        let _code_map;
        let source_eff;
        if let Some(ref cv) = config.cope_version {
            let (map, new_source) = util::clone_all_for_version(
                self.repo.as_ref(),
                &config.start_id,
                config.start_code.as_deref(),
                cv,
            )?;
            _code_map = map;
            source_eff = new_source;
        } else {
            _code_map = Default::default();
            source_eff = source_orig
                .property("code")
                .and_then(|v| v.as_str())
                .unwrap_or(&config.start_id)
                .to_string();
        };

        // 用克隆后的副本为源实体
        let source = self
            .repo
            .get_node(&source_eff)
            .ok()
            .flatten()
            .unwrap_or(source_orig);

        let source_id = source
            .property("code")
            .and_then(|v| v.as_str())
            .unwrap_or(&source_eff)
            .to_string();

        // ── 2. 源上下文 ──
        let source_outgoing = util::summarize_relations(
            &self
                .repo
                .get_relationships(&source_id, None)
                .unwrap_or_default(),
        );
        let source_incoming_rel =
            util::find_incoming_relationships(self.repo.as_ref(), &source_id, None);
        let source_incoming = util::summarize_relations(&source_incoming_rel);

        let rel_filter = if config.relation.is_empty() {
            None
        } else {
            Some(config.relation.as_str())
        };

        // ── 3. BFS 遍历 ──
        let mut chain = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        visited.insert(source_id.clone());

        let mut queue: VecDeque<(String, usize)> = VecDeque::new();
        queue.push_back((source_id.clone(), 0));

        while let Some((current_id, current_depth)) = queue.pop_front() {
            if current_depth >= max_depth {
                continue;
            }

            let current_node = self.repo.get_node(&current_id).ok().flatten();

            // 按方向收集关系
            let mut relations: Vec<(
                bool,
                ontology_storage::mapper::graph::relationship::Relationship,
            )> = Vec::new();

            if matches!(config.direction, Direction::Outgoing | Direction::Both) {
                for r in self
                    .repo
                    .get_relationships(&current_id, rel_filter)
                    .unwrap_or_default()
                {
                    relations.push((true, r));
                }
            }
            if matches!(config.direction, Direction::Incoming | Direction::Both) {
                for r in
                    util::find_incoming_relationships(self.repo.as_ref(), &current_id, rel_filter)
                {
                    relations.push((false, r));
                }
            }

            for (is_out, rel) in &relations {
                let target_id = if *is_out {
                    &rel.end_node_id
                } else {
                    &rel.start_node_id
                };

                if visited.contains(target_id.as_str()) {
                    continue;
                }

                let target_node = self.repo.get_node(target_id).ok().flatten();

                // 副本版本守卫：只允许在同一版本内遍历
                if let Some(ref cv) = config.cope_version {
                    let target_ver = target_node
                        .as_ref()
                        .and_then(|n| n.property("cope_version").and_then(|v| v.as_str()))
                        .unwrap_or("");
                    if target_ver != cv.as_str() {
                        continue; // 跳过不同版本或原实体
                    }
                }

                let effective_target_id = target_id.clone();

                if visited.contains(&effective_target_id) {
                    continue;
                }
                visited.insert(effective_target_id.clone());

                // ── 4. 逐跳分析 ──
                let target_type_chain =
                    util::get_type_ancestors(self.repo.as_ref(), target_node.as_ref());

                let matching_rules = util::find_matching_rules(
                    self.repo.as_ref(),
                    &current_id,
                    &rel.rel_type,
                    &effective_target_id,
                    "rules",
                );

                let target_outgoing =
                    util::predict_next_steps(self.repo.as_ref(), &effective_target_id);

                // ── 置信度检查（读取节点自身 confidence 属性） ──
                let node_confidence =
                    util::prop_as_f64(target_node.as_ref().and_then(|n| n.property("confidence")));
                let stop_propagation = match (node_confidence, config.confidence_threshold) {
                    (Some(v), Some(t)) => v < t,
                    _ => false, // 无置信度数据 或 未设阈值 → 不阻断
                };

                chain.push(ExploreHop {
                    hop: current_depth + 1,
                    direction: if *is_out {
                        "outgoing".into()
                    } else {
                        "incoming".into()
                    },
                    source_id: current_id.clone(),
                    target_id: effective_target_id.clone(),
                    rel_type: rel.rel_type.clone(),
                    source_node: current_node.clone(),
                    target_node,
                    target_type_chain,
                    matching_rules,
                    target_outgoing,
                    confidence: node_confidence,
                    stop_propagation,
                });

                // 只有未阻断时才继续传播
                if !stop_propagation {
                    queue.push_back((effective_target_id.clone(), current_depth + 1));
                }
            }
        }

        Ok(ExploreResult {
            source,
            relation: config.relation.clone(),
            direction: config.direction.clone(),
            source_outgoing,
            source_incoming,
            chain,
        })
    }
}
