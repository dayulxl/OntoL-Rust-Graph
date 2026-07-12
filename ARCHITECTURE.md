# Ontologica Rust Graph — 架构文档 v2.1

> 最后更新: 2026-07-13 | 图原生推理引擎

---

## 1. 架构分层

```
┌─────────────────────────────────────────────────────────┐
│              LLM / Agent / 前端                          │
│  POST /infer-on-nodes-id-fc  (NDJSON 流式)                 │
│  GET  /tools               (工具定义)                   │
│  GET  /health              (健康检查)                   │
└──────────────┬──────────────────────────────────────────┘
               │ HTTP (tiny_http)
┌──────────────▼──────────────────────────────────────────┐
│        ontology-server  [Gateway — 3 个端点]            │
│  - 流式路由 /infer-on-nodes-id-fc                          │
│  - 线程池隔离（每请求独立线程）                           │
└──────────────┬──────────────────────────────────────────┘
               │ Direct Call
┌──────────────▼──────────────────────────────────────────┐
│        ontology-reasoner  [推理引擎]                     │
│  ┌─ gie/engine/ — InferenceEngine ────────────────────┐ │
│  │  Step 1: 复制种子 + RDFS链递归爬顶层               │ │
│  │  Step 2: actionType="inference" 递归克隆下游       │ │
│  │  Step 3: 逐节点推理叙述输出                         │ │
│  └────────────────────────────────────────────────────┘ │
│  - SWRL 行为引擎 (6 字段 hasPrecondition/Effect/…)      │
│  - 置信度传播 + 阻断熔断                                │
│  - 副本版本控制 (cope_version)                          │
├──────────────────────────────────────────────────────────┤
│  QueryPlan (抽象) ──→ execute_plan()                     │
└──────────────┬──────────────────────────────────────────┘
               │ Bolt Protocol (neo4rs)
┌──────────────▼──────────────────────────────────────────┐
│        Memgraph (图数据库)                               │
│  - 原生 ID (id(n)) 作为唯一标识                          │
│  - openCypher 查询                                       │
│  - 所有数据已在图中，无 RDFS 文件                         │
└─────────────────────────────────────────────────────────┘
```

---

## 2. 接口列表

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/health` | 健康检查 + 节点统计 |
| GET | `/tools` | LLM Function Calling 工具定义 |
| POST | `/infer-on-nodes-id-fc` | GIE 推理机（流式 NDJSON） |

### `/infer-on-nodes-id-fc` 参数

```json
{
  "node_ids": ["3"],
  "confidence": 0.8,
  "cope_version": "550e8400-e29b-41d4-a716-446655440000"
}
```

响应: `Content-Type: application/x-ndjson`，逐行推送事件流。

---

## 3. 核心设计原则

### 3.1 图原生推理

- **没有 RDFS 文件** — 所有数据在图数据库中。RDFS/OWL/SWRL 语法是 DSL 输入，翻译为 Cypher 执行
- **全部依赖图索引** — 沿边走（关系索引）、原生 ID 点查（主键）、标签扫（label 索引）。无全文检索
- **推理机就是图** — 在图上游走、读属性、写回副本

### 3.2 原生 ID 体系

- **所有标识用 `id(n)`（Memgraph 原生内部 ID）**，不用属性字段 `id` 做锚点
- `get_node` / `get_relationships` / `insert_node` / `insert_relationship` / `delete_node` 全部支持原生 ID 匹配
- 副本节点 `graph_id` 存原节点的原生 ID，用于追溯

### 3.3 副本隔离

- 推理所有写操作在副本上执行 (`cope_version` = UUID)
- 副本自动生成新原生 ID，原数据不受影响
- 推理完成后可通过 `delete_by_cope_version` 清理

### 3.4 三步流水线

```
Step 1 — 事实关系: 递归爬RDFS链 → 继承属性 → 克隆
Step 2 — 推理节点: 沿 actionType="inference" 递归克隆下游
Step 3 — 推理叙述: 逐节点行为推理 + 置信度传播 + 输出说明
```

---

## 4. 目录结构

```
crates/
├── ontology-storage/         存储层
│   └── src/
│       ├── repository/       GraphRepository trait
│       ├── adapters/
│       │   └── memgraph/     MemgraphAdapter (Bolt)
│       └── mapper/           query_plan, unified_mapping
│
├── ontology-reasoner/        推理引擎
│   └── src/
│       ├── gie/              图推理机核心
│       │   ├── engine/       InferenceEngine (三步流水线)
│       │   ├── translator/   SWRL/DWL2/JSONPath 翻译器
│       │   ├── action/       动作路由器
│       │   ├── version.rs    版本控制
│       │   └── context.rs    推理上下文
│       ├── swrl/             SWRL 行为引擎
│       ├── confidence/       置信度系统
│       ├── graph/            图工具函数
│       └── language.rs       语言前缀解析
│
└── ontology-server/          HTTP 网关
    └── src/
        ├── server.rs         路由分发（3 个端点）
        ├── routes/
        │   ├── health.rs     GET /health
        │   ├── tools.rs      GET /tools
        │   └── infer_on_nodes.rs  POST /infer-on-nodes-id-fc
        ├── app.rs            AppState
        └── main.rs           服务入口
```

---

## 5. 语言前缀规范 (7 种)

| # | 前缀 | 名称 | GIE 处理 |
|---|------|------|---------|
| 1 | `rdfs:` | RDFS 语言 | Step 1 — 事实层（属性继承） |
| 2 | `owl2:` | OWL2 DL | Step 1 — 事实层（类层次） |
| 3 | `swrl:` | SWRL 规则 | Step 3 — 行为推理 |
| 4 | `sh:` | SHACL 约束 | Step 3 — 约束验证 |
| 5 | `rule:` | 规则设定 | 推理方向控制 |
| 6 | `func:` | 自定义函数 | JSON 函数调用 |
| 7 | `$.` | JSONPath (RFC 9535) | 属性/关系路径提取 |

---

## 6. 技术决策记录 (ADR)

| 编号 | 决策 | 日期 |
|------|------|------|
| ADR-001 | GNU ABI 工具链 | 2026-07-04 |
| ADR-002 | Memgraph 替代 Neo4j | 2026-07-05 |
| ADR-003 | Bolt 协议 | 2026-07-04 |
| ADR-004 | BehaviorAction 6 字段 | 2026-07-05 |
| ADR-005 | SWRL 并发执行 | 2026-07-05 |
| ADR-006 | cope_version 副本隔离 | 2026-07-05 |
| ADR-007 | unified_mapping SSOT | 2026-07-10 |
| ADR-008 | Snowflake ID (i64) | 2026-07-10 |
| ADR-009 | GIE 三层解耦 | 2026-07-12 |
| ADR-010 | 原生 ID (id(n)) 统一标识 | 2026-07-13 |
| ADR-011 | 完全摒弃 RDFS 文件，全走图原生推理 | 2026-07-13 |
