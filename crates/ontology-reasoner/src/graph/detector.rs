//! 状态变化检测器 — 可插拔 trait。
//!
//! 产品层定义 trait，业务层（ontology-server）实现具体检测逻辑。

use ontology_storage::mapper::graph::node::Node;

/// 状态变化检测器 trait。
///
/// 对图遍历中的每一跳，对比源节点与目标节点的属性，产出状态变化描述。
/// 业务方可实现此 trait 注入领域知识：
/// - 军事 ASW：解析 `Space_abs` 坐标、计算 haversine 距离、识别"移动"/"打击"等关系语义
/// - 金融风控：检测账户余额变化、交易模式异常
/// - 知识图谱：追踪概念层级变化、属性继承
pub trait StateChangeDetector: Send + Sync {
    /// 检测一对节点间的状态变化
    fn detect_changes(
        &self,
        source: Option<&Node>,
        target: Option<&Node>,
        relation: &str,
    ) -> Vec<String>;

    /// 检测器名称（用于日志/调试）
    fn name(&self) -> &str;
}

/// 默认检测器 — 做通用属性差异对比。
///
/// 不包含任何领域知识，仅报告：
/// - 基本的 `A -[关系]-> B` 连接
/// - 同名属性值变化
pub struct DefaultStateChangeDetector;

impl StateChangeDetector for DefaultStateChangeDetector {
    fn detect_changes(
        &self,
        source: Option<&Node>,
        target: Option<&Node>,
        relation: &str,
    ) -> Vec<String> {
        let mut changes = Vec::new();
        let (src, tgt) = match (source, target) {
            (Some(s), Some(t)) => (s, t),
            _ => return changes,
        };

        let src_code = src
            .property("code")
            .and_then(|v| v.as_str())
            .or_else(|| src.property("iri").and_then(|v| v.as_str()))
            .unwrap_or("?");
        let tgt_code = tgt
            .property("code")
            .and_then(|v| v.as_str())
            .or_else(|| tgt.property("iri").and_then(|v| v.as_str()))
            .unwrap_or("?");

        // 基本连接报告
        changes.push(format!("{} -[{}]-> {}", src_code, relation, tgt_code));

        // 同名属性值变化
        for (k, sv) in &src.properties {
            if let Some(tv) = tgt.properties.get(k)
                && sv != tv
            {
                changes.push(format!("属性 '{}' 变化: {:?} → {:?}", k, sv, tv));
            }
        }

        // 目标独有属性
        for k in tgt.properties.keys() {
            if !src.properties.contains_key(k) {
                changes.push(format!("目标引入新属性: '{}'", k));
            }
        }

        changes
    }

    fn name(&self) -> &str {
        "default"
    }
}
