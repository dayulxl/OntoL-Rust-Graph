# graph — 通用图遍历与推理模块

> **定位**：产品层代码（framework），不含任何领域知识。
> **业务代码**在 `ontology-server/src/routes/infer.rs`。

## 产品层 / 业务层隔离

```
┌───────────────────────────────────────────────────────┐
│  ontology-server (业务层)                              │
│  routes/infer.rs                                      │
│  ├─ HTTP 请求/响应 (serde_json + tiny_http)            │
│  ├─ MilitaryStateChangeDetector                       │
│  │   └─ Space_abs / haversine / 中文关系语义 / SWRL   │
│  └─ build_response() → JSON                          │
└──────────────────┬────────────────────────────────────┘
                   │ 调用
┌──────────────────▼────────────────────────────────────┐
│  ontology-reasoner::graph (产品层 / 框架)               │
│  ├─ GraphExplorer     ← 通用 BFS 图遍历引擎           │
│  ├─ StateChangeDetector trait ← 可插拔检测器接口       │
│  └─ util              ← 实体查找/关系汇总/类型层次/    │
│                         规则匹配/下一步预测             │
└───────────────────────────────────────────────────────┘
```

## 模块结构

| 文件 | 职责 |
|------|------|
| `explorer.rs` | `GraphExplorer` — 通用 BFS 多跳图遍历引擎 |
| `detector.rs` | `StateChangeDetector` trait — 可插拔状态变化检测器接口 |
| `util.rs` | 通用工具函数 — 实体查找、关系汇总、类型层次、规则匹配、下一步预测 |
| `mod.rs` | 模块声明 + re-exports |

## 核心类型

### GraphExplorer

```rust
pub struct GraphExplorer {
    repo: SharedRepo,  // Arc<dyn GraphRepository>
}

impl GraphExplorer {
    pub fn new(repo: SharedRepo) -> Self;
    pub fn explore(&self, config: &ExploreConfig) -> Result<ExploreResult, String>;
}
```

**输入** — `ExploreConfig`：

| 字段 | 类型 | 说明 |
|------|------|------|
| `start_id` | `String` | 起始实体 ID（必填） |
| `relation` | `String` | 要跟随的关系类型（必填） |
| `max_depth` | `usize` | 最大遍历深度（默认 3，上限 5） |
| `direction` | `Direction` | `Outgoing` / `Incoming` / `Both` |

**输出** — `ExploreResult`：

| 字段 | 类型 | 说明 |
|------|------|------|
| `source` | `Node` | 起始实体 |
| `relation` | `String` | 遍历使用的关系类型 |
| `direction` | `Direction` | 遍历方向 |
| `source_outgoing` | `Vec<RelCount>` | 源实体的出向关系摘要 |
| `source_incoming` | `Vec<RelCount>` | 源实体的入向关系摘要 |
| `chain` | `Vec<ExploreHop>` | 遍历链（BFS 顺序） |

**ExploreHop**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `hop` | `usize` | 跳数（从 1 开始） |
| `direction` | `String` | "outgoing" / "incoming" |
| `source_id` | `String` | 源节点 ID |
| `target_id` | `String` | 目标节点 ID |
| `rel_type` | `String` | 关系类型 |
| `source_node` | `Option<Node>` | 源节点数据 |
| `target_node` | `Option<Node>` | 目标节点数据 |
| `target_type_chain` | `Vec<String>` | 目标节点的 subClassOf 类型祖代链 |
| `matching_rules` | `Vec<RuleMatch>` | 匹配到的 SWRL 规则 |
| `target_outgoing` | `Vec<RelCount>` | 目标节点的出向关系（用于下一步预测） |

### StateChangeDetector trait

```rust
pub trait StateChangeDetector: Send + Sync {
    fn detect_changes(
        &self,
        source: Option<&Node>,
        target: Option<&Node>,
        relation: &str,
    ) -> Vec<String>;

    fn name(&self) -> &str;
}
```

**产品层只定义接口**，不实现。业务层实现 trait 注入领域知识。

内置 `DefaultStateChangeDetector` — 通用属性 diff，无领域知识。

### 工具函数 (`util.rs`)

| 函数 | 签名 | 说明 |
|------|------|------|
| `find_entity_any` | `(repo, id) -> Option<Node>` | 多标签多字段扫描查找实体 |
| `find_incoming_relationships` | `(repo, target_id, rel_type?) -> Vec<Relationship>` | 入向关系反向查询 |
| `summarize_relations` | `(&[Relationship]) -> Vec<RelCount>` | 关系分组计数 + 示例目标 |
| `get_type_ancestors` | `(repo, node?) -> Vec<String>` | subClassOf 类型祖代链 |
| `find_matching_rules` | `(repo, src_id, rel, tgt_id, dir) -> Vec<RuleMatch>` | SWRL 规则文件扫描匹配 |
| `predict_next_steps` | `(repo, node_id) -> Vec<RelCount>` | 目标节点出向关系预测 |
| `prop_as_f64` | `(Option<&PropertyValue>) -> Option<f64>` | 属性值 → f64 |
| `truncate_str` | `(&str, max) -> String` | 字符串截断 |

## 遍历流程

```
GraphExplorer::explore(config)
  │
  ├─ 1. 实体解析
  │     find_entity_any() → 扫描 Entity/Patrol/Strike/Type 标签
  │
  ├─ 2. 源上下文
  │     summarize_relations(source_outgoing / source_incoming)
  │
  ├─ 3. BFS 遍历
  │     queue → while 出队:
  │       ├─ 按 direction 收集关系
  │       ├─ 去重 visited set
  │       ├─ get_node(target_id) → 目标节点
  │       ├─ get_type_ancestors()    → 类型层次
  │       ├─ find_matching_rules()   → 规则匹配
  │       ├─ predict_next_steps()    → 下一步预测
  │       └─ 入队 target_id
  │
  └─ 4. 返回 ExploreResult
         (纯 Rust struct, 不依赖 serde_json)
```

## 业务方接入方式

只需实现 `StateChangeDetector` trait：

```rust
// 军事 ASW
struct MilitaryStateChangeDetector;
impl StateChangeDetector for MilitaryStateChangeDetector {
    fn detect_changes(&self, src, tgt, rel) -> Vec<String> {
        // 解析 Space_abs → haversine 距离
        // 检查 speed/power/confidence 语义
        // 识别 "移动"/"打击" 中文关系语义
    }
    fn name(&self) -> &str { "军事ASW" }
}

// 金融风控
struct FinanceDetector;
impl StateChangeDetector for FinanceDetector {
    fn detect_changes(&self, src, tgt, rel) -> Vec<String> {
        // account_balance 异常转账
        // risk_score 风险升级
    }
    fn name(&self) -> &str { "金融风控" }
}
```

**GraphExplorer 和其他 util 函数完全不用改。**

## 依赖约束

| 允许 | 禁止 |
|------|------|
| `ontology-storage` (Node/Relationship/GraphRepository) | `serde_json` |
| `std::fs` (读取规则文件) | `tiny_http` / `axum` |
| `std::collections` | 任何业务领域概念 |

## 与其他模块的关系

```
graph ←─ 不依赖任何 reasoner 模块（dwl2/swrl/timeline/confidence）
graph ←─ util.rs 中 find_matching_rules() 通过文件系统读取 rules/*.swrl
graph ←─ 不调用 SwrlEngine / Reasoner / TimelineEngine
```
