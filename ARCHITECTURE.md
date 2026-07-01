# Ontologica Rust Graph — 架构文档 v0.3

> **名称约定**：项目名为 `ontologica_rust_graph`，各 crate 前缀 `ontology-`。

> **术语约定**：**"本体"** 即 **Neo4j 节点**（Node）。Entity/Type/Patrol 统称本体。`(:Entity)`、`(:Type)`、`(:Patrol)` 是 label，`[:移动]`、`[:subClassOf]` 是 relationship。非 RDF/OWL 标准本体。

> **时间线标记**：✅ = 已实现 | ⬅ = 迁移中 | 📋 = 规划 | ⚠ = 已知风险

---

## 1. 目标架构分层

```
┌─────────────────────────────────────────────────┐
│              LLM / Agent (2026 Standard)         │
│  (Natural Language Input / ReactJSON Tool Use)  │
└──────────────┬──────────────────────────────────┘
               │ HTTPS (Async - Axum) + API Key
┌──────────────▼──────────────────────────────────┐
│        ontology-gateway/  [Async Entry Point]    │
│  - 自然语言解析 + 语义路由                       │
│  - LLM 辅助功能区 (tool/schema/prompt)  ⚠       │
│  - tower 中间件 (熔断/限流/认证)                 │
└──────────────┬──────────────────────────────────┘
               │ Direct Call (spawn_blocking → async)
┌──────────────▼──────────────────────────────────┐
│         ontology-engine/  [Core Logic]          │
│  - DWL2 查询引擎 (仅暴露 QueryPlan，不构造 IR)   │
│  - SWRL 固定点推理 (专用线程池)                  │
│  - ConfidencePolicy (动态策略注入)               │
│  - GraphRAG Retriever 📋                        │
│  - timeline 时序推演                             │
└──────────────┬──────────────────────────────────┘
               │ Bolt Protocol (neo4rs) 📋
┌──────────────▼──────────────────────────────────┐
│        Neo4j / Vector DB / Config Store          │
│  - Native Index + Spatial Point + Vector Index   │
│  - 规则持久化 + 策略持久化                       │
└─────────────────────────────────────────────────┘
```

---

## 2. 职责边界与耦合约束

| 层 | 允许依赖 | 禁止 |
|----|---------|------|
| `ontology-gateway` | `ontology-engine` 公开 API；`ontology-storage` 仅通过 engine 间接访问 | 直接引用 `ontology-storage` 内部类型；直接构造 Cypher |
| `ontology-engine` | `ontology-storage` 仅通过 `SharedRepository` trait | 直接构造 `GraphPattern` 或 `Node/Relationship` ⚠ |
| `ontology-storage` | 外部驱动库 (neo4rs/ureq)；标准库 | 反向依赖 engine 或 gateway |

### 2.1 ⚠ 解耦计划：推理层不直接构造 GraphPattern

**问题**：`dwl2/query.rs` 直接构造 `GraphPattern` 并通过 `repo.query_pattern()` 执行。这使推理层绑定存储层的内部 IR。

**方案**：推理层定义查询抽象 `QueryPlan`：

```rust
// ontology-engine 中定义
pub enum QueryPlan {
    GetByCode(String),
    GetByLabel(String),
    GetRelationships { source: String, rel_type: Option<String> },
    PatternMatch { start_label: Option<String>, rel_type: Option<String>, end_label: Option<String>, end_props: Vec<(String, PropertyValue)> },
}
```

`GraphRepository` 新增方法 `execute_plan(&self, plan: &QueryPlan) -> Result<QueryResult>`，各适配器自行实现。推理层不再直接 import `mapper::graph::pattern`。

**迁移路径**：先加新方法，标记旧方法 deprecated，逐步替换调用方后删除旧方法。

---

## 3. 运行时架构（异步迁移）

### 3.1 当前（v0.2）

| 组件 | 实现 | 问题 |
|------|------|------|
| HTTP Server | `tiny_http` 同步单线程 | 阻塞，无法高并发 |
| HTTP Client | `ureq` 同步 | 阻塞 |
| Neo4j 连接 | HTTP API `POST /tx/commit` | JSON 开销大，无流式 |
| 推理循环 | 同步 fixpoint | 无并行 |

### 3.2 异步迁移三阶段

#### Phase 1（当前可实施）：网关异步化

- `tiny_http` → `axum` (tokio)
- 推理调用仍为同步，通过 `tokio::task::spawn_blocking` 桥接
- 新增 `tower` 中间件：CORS、限流、超时
- 推理引擎使用**专用线程池**（`rayon` 或 `tokio blocking pool`），避免阻塞 I/O 工作线程

#### Phase 2（neo4rs 可用后）：存储异步化

- `ureq` → `neo4rs` Bolt 驱动
- `GraphRepository` trait 增加 `async fn` 方法（`add_async_*`，逐步迁移）
- 配置管理：Neo4j 连接信息、日志级别、策略默认值通过 `config` crate 或环境变量管理
- **认证**：网关加 API Key 验证中间件

#### Phase 3（工程优化）：推理异步化

- SWRL 规则循环内多规则并行匹配（`tokio::spawn`）
- 中间结果通过异步通道共享，读写冲突通过规则级隔离处理（每条规则独立事实集，merge 阶段处理冲突）
- 推理引擎 state 管理：事实集按规则隔离，fixpoint merge 时去重

### 3.3 配置管理

```rust
pub struct ServerConfig {
    pub port: u16,                     // ONTOLOGY_PORT, 默认 8085
    pub neo4j_uri: String,             // ONTOLOGY_NEO4J_URI, 默认 http://localhost:7474
    pub neo4j_user: String,            // ONTOLOGY_NEO4J_USER, 默认 neo4j
    pub neo4j_password: String,        // ONTOLOGY_NEO4J_PASSWORD, 默认 12345678
    pub default_mode: OperationMode,   // ONTOLOGY_MODE, 默认 Exercise
    pub log_level: String,             // RUST_LOG, 默认 info
}
```

> 配置注入方式：`ServerConfig::from_env()` 从环境变量读取（参见 [crates/ontology-server/src/config.rs](crates/ontology-server/src/config.rs)）。
> 支持 `.env` 文件（项目根目录），可通过 `dotenv` 或 IDE 加载。

---

## 4. 推理引擎异步化设计补充

### 4.1 Fixpoint 循环的并行化

```text
当前：for rule in rules { execute_rule(rule) }  // 串行
目标：parallel(rules.map(|r| execute_rule(r)))   // 并行，每条规则独立执行
```

- 每条规则的 `execute_rule` 不修改共享状态——只读取 repo，产生 `InferenceResult`
- `insert_derived_facts` 在并行阶段结束后串行执行（事实去重）
- 首次迭代串行（冷启动，事实少），后续迭代并行

### 4.2 状态隔离

| 组件 | 共享/隔离 | 说明 |
|------|----------|------|
| `derived_facts: HashSet<String>` | 串行化访问 | 合并阶段加锁去重 |
| `repo: SharedRepository` | 只读并行 | 规则匹配阶段不写入 |
| `confidence_calc` | 不可变 | 只读，无冲突 |
| `builtins` | 不可变 | 只读 |

---

## 5. LLM 交互

### 5.1 模块归属

| 功能 | 归属 | 说明 |
|------|------|------|
| Tool Definition / JsonSchema | `ontology-gateway` | LLM 调用入口，由网关生成 |
| Prompt 构建 | `ontology-gateway` | 读取 repository 构建 LLM 上下文 |
| Response 解析 | `ontology-gateway` | LLM 返回的工具调用/结构化输出解析 |
| NL 查询解析 | `ontology-gateway` | `/nl-query` 端点，关键词→DWL2 |
| 动态规则生成 | `ontology-engine` 📋 | 仅当 LLM 需要修改推理规则时 |

`mapper/llm/` 当前在 `ontology-storage` 中，迁移到 gateway crate。

### 5.2 GraphRAG 混合检索设计

```text
POST /context (目标形态)
  1. Embedding 选择: 本地 ONNX 模型 (如 all-MiniLM-L6-v2)，维度 384
  2. 语义定位: 用户查询 → Embedding → Neo4j Vector Index ANN 检索 top_k=5
  3. 图展开: 从命中节点沿 [:移动]/[:subClassOf] 等关系展开子图，depth=2
  4. 融合排序: score = α × semantic_similarity + (1-α) × graph_centrality，α=0.6
  5. 索引更新: 实体 create/update 时异步重算 Embedding
```

**参数配置**：

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `top_k` | 5 | ANN 检索返回数量 |
| `depth` | 2 | 图展开深度 |
| `alpha` | 0.6 | 语义权重比例 |
| `min_similarity` | 0.5 | 最低相似度阈值 |

---

## 6. Query 序列化稳定性

### 6.1 ⚠ 当前风险

`ClassExpression::to_key() / from_key()` 字符串序列化对空格、子表达式顺序敏感，无 Canonical 形式。

### 6.2 缓解措施

1. `to_key()` 输出时强制标准化：子表达式按字典序排列，去除空格
2. 添加 `ClassExpression::to_json()` (JSON) 作为备选格式
3. 为每个表达式分配哈希 ID：`let id = expr.to_key().hash()` 用于快速去重
4. 长期：改用结构化存储（如 `serde` derive），但需引入依赖

---

## 7. 数据模型迁移策略

### 7.1 Point 类型迁移（Space_abs → location）

| 阶段 | 操作 | 验证 |
|------|------|------|
| 1. 双写 | 写入 `Space_abs` 的同时写入 `location: Point` | 两字段值一致 |
| 2. 读取切换 | 查询优先读 `location`，fallback 到 `Space_abs` | 覆盖旧数据 |
| 3. 索引 | `CREATE INDEX entity_location` | 查询加速 |
| 4. 清理 | 确认无 reader 后删除 `Space_abs` | 数据完整性 |

**回滚**：任意阶段可回退——只需改查询优先级，数据无损。

### 7.2 置信度策略持久化

- 运行时调用 `POST /confidence/policy` 的同时写入 Neo4j 配置节点 `(:Config { key: "policy", mode: "WarFighting" })`
- 启动时 `Reasoner::new()` 从 Neo4j 读取 `(:Config)` 节点，存在则恢复策略，不存在则用默认
- `GET /confidence/policy` 返回当前策略 + 是否持久化标记

---

## 8. 规则加载机制

### 8.1 当前

SWRL 规则通过 `Reasoner::load_swrl_rule(text)` 运行时加载，重启即失。

### 8.2 加载路径（优先级从高到低）

1. **Neo4j 存储**：`(:Rule { name, text, enabled, priority })` 节点。启动时自动加载 `enabled=true` 的规则
2. **文件**：`rules/` 目录下的 `.swrl` 文件。适用于开发阶段
3. **HTTP**：`POST /reason { "rules": ["..."] }` 临时加载，调用后生效

**热加载**：`POST /rules/reload` 从 Neo4j 重新读取规则，不需重启。

---

## 9. 运维、安全与可观测性

### 9.1 安全

| 层级 | 措施 | 状态 |
|------|------|------|
| HTTP 网关 | API Key 验证中间件 (环境变量 `ONTOLOGY_API_KEY`) | 📋 |
| Neo4j | 密码通过环境变量注入，不硬编码 | ✅ |
| CORS | 开发环境全开，生产环境配置白名单 | ⬅ |

### 9.2 日志与监控

| 维度 | 当前 | 目标 |
|------|------|------|
| 推理日志 | 文件 `logs/` 纯文本 | `tracing` JSON 结构化日志 |
| 请求指标 | 无 | `tower_http::metrics` 请求计数 + 耗时 |
| 推理指标 | 无 | `metrics` crate 暴露 `reasoner.derived_facts`, `reasoner.fuse_trips` |

### 9.3 配置管理

```bash
# 环境变量清单（详见 .env 文件）
ONTOLOGY_PORT=8085                                # HTTP 端口，默认 8085
ONTOLOGY_NEO4J_URI=http://localhost:7474           # Neo4j HTTP API 地址
ONTOLOGY_NEO4J_USER=neo4j                          # Neo4j 用户名，默认 neo4j
ONTOLOGY_NEO4J_PASSWORD=12345678                   # Neo4j 密码，默认 12345678
ONTOLOGY_MODE=Exercise                             # 默认推理策略: Exercise|WarFighting|Training
RUST_LOG=info                                      # 日志级别 (tracing/env_logger)
```

> ⚠ 生产环境务必修改默认密码。Neo4j AuraDB 用户将 `ONTOLOGY_NEO4J_URI` 设为 `https://<instance-id>.databases.neo4j.io`。

---

## 10. 测试策略

| 层级 | 类型 | 当前覆盖 | 目标 |
|------|------|---------|------|
| `spatial/` | 单元测试 | 3 个 | ✅ 已完成 |
| `confidence/` (calculator + fuse + policy) | 单元测试 | 7 个 | ✅ 已完成 |
| `swrl/` (engine + parser + builtins) | 单元测试 | 8 个 | ✅ 已完成 |
| `dwl2/` (query) | 单元测试 | 3 个 | ✅ 已完成 |
| `timeline/` (engine) | 单元测试 | 1 个 | ✅ 已完成 |
| `ontology-storage/` (adapter + ontology) | 单元测试 | 26 个 | ✅ 已完成 |
| `ontology-server/` (routes) | 单元测试 | 0 个 | 📋 适配层无需独立测试 |
| `ontology-server/` (neodj 后端) | E2E | 0 个 | 📋 需要 |

**总计**：61 个单元测试 (lib) + 1 个 doctest，0 个集成测试 (bin)。CI 待配置。

---

## 11. Python 脚本维护

| 脚本 | 用途 | 策略 |
|------|------|------|
| `import_all.py` | Entity 批量导入 | 标记为一次性迁移工具，纳入 `scripts/` 目录 |
| `import_types.py` | Type 层级导入 | 同上 |
| `patrol_sim.py` | 巡逻推演演示 | 功能已由 Rust `TimelineEngine` 替代，可归档 |

> Python 脚本仅作为运维辅助，核心逻辑均在 Rust 中。未来可移植为 Rust CLI 子命令。

---

## 12. Feature Flags

| Feature | 当前依赖 | 目标依赖 | 说明 |
|---------|---------|---------|------|
| `in-memory` | 无（std only） | `dev-dependency` only | 仅用于单元测试 |
| `neo4j` | ureq, serde_json | **neo4rs, tokio** 📋 | Bolt 异步驱动 |
| `llm` | serde, serde_json | 迁至 `ontology-gateway` | LLM 数据格式层 |

---

## 13. 目录结构（v0.3 现状）

```
ontologica_rust_graph/
├── Cargo.toml                              # workspace root
├── .env                                    # 环境变量配置（Neo4j 连接等）
├── src/main.rs                             # CLI 推理演示
├── scripts/                                # 运维脚本（Python）
│   ├── import_all.py                       #   Entity 批量导入
│   ├── import_types.py                     #   Type 层级导入
│   ├── import_props.py                     #   属性导入
│   ├── create_p8a.py                       #   P-8A 测试数据创建
│   ├── test_patrol.py                      #   巡逻 API 测试
│   ├── patrol_sim.py                       #   巡逻推演演示（已由 Rust TimelineEngine 替代）
│   └── verify.py                           #   数据验证
├── rules/                                  # SWRL 规则文件
│   └── asw_rules.swrl
├── logs/                                   # 推理日志输出
│
├── crates/
│   ├── ontology-storage/                   # 存储适配层
│   │   └── src/
│   │       ├── repository/                 # GraphRepository trait + Transaction
│   │       ├── adapters/                   # neo4j (HTTP) + in_memory
│   │       ├── mapper/                     # 属性图 IR (graph/cypher/llm)
│   │       └── ontology/                   # Entity/Type/Patrol/Relationship CRUD
│   │
│   ├── ontology-reasoner/                  # 核心推理引擎
│   │   └── src/
│   │       ├── graph/                      # 通用图遍历 — 产品层框架 (v0.3)
│   │       │   ├── explorer.rs             #   GraphExplorer — BFS 多跳遍历
│   │       │   ├── detector.rs             #   StateChangeDetector trait — 可插拔
│   │       │   └── util.rs                 #   实体查找/关系汇总/类型层次/规则匹配
│   │       ├── dwl2/                       # DWL2 查询 (12 种构造子)
│   │       ├── swrl/                       # SWRL 推理 (7 种 Atom + fixpoint)
│   │       ├── confidence/                 # 置信度计算 + 策略切换
│   │       ├── spatial/                    # Haversine 空间计算
│   │       ├── timeline/                   # 时序推演 + 打击决策
│   │       ├── reasoner.rs                 # Reasoner 主入口
│   │       ├── query_plan.rs               # QueryPlan 抽象
│   │       └── logger.rs                   # 日志初始化
│   │
│   ├── ontology-server/                    # HTTP 网关 (tiny_http → axum 📋)
│   │   └── src/
│   │       ├── routes/                     # 15 个端点（HTTP 适配层，薄包装）
│   │       │   ├── health.rs               # GET /health
│   │       │   ├── schema.rs               # GET /schema
│   │       │   ├── tools.rs                # GET /tools — LLM 工具定义（12 个 function）
│   │       │   ├── tools_call.rs           # POST /tools/call — LLM 统一调度入口
│   │       │   ├── query.rs                # POST /query — Entity 搜索
│   │       │   ├── reason.rs               # POST /reason — SWRL 推理
│   │       │   ├── context.rs              # POST /context — 图上下文
│   │       │   ├── patrol.rs               # GET|POST /patrol — 巡逻推演
│   │       │   ├── strike.rs               # GET|POST /strike — 打击决策推演
│   │       │   ├── infer.rs                # POST /infer-forward — 向前推理 (v0.3 业务层)
│   │       │   ├── nl_query.rs             # POST /nl-query — 自然语言查询
│   │       │   ├── ontology_create.rs      # POST /ontology/create — 本体创建
│   │       │   ├── ontology_relationship.rs # POST /relationships/create — 关系创建
│   │       │   ├── confidence_policy.rs    # POST /confidence/policy — 策略切换
│   │       │   └── rules.rs                # GET|POST /rules — 规则管理+热加载
│   │       ├── server.rs                   # tiny_http 事件循环 + 路由分发
│   │       ├── app.rs                      # AppState (Reasoner + SharedRepository)
│   │       ├── config.rs                   # ServerConfig::from_env()
│   │       └── main.rs                     # 服务入口
│   │
│   └── ontology-spatial/                   # 📋 独立空间 crate
│
└── ARCHITECTURE.md                         # 本文件
```

---

## 14. 知识图谱数据模型（ASW）

### 节点标签

| 标签 | 说明 |
|------|------|
| `Entity` | 实体节点（装备/舰船/传感器），24 个标准字段 |
| `Type` | 分类层级节点，`subClassOf` 关系 |
| `Patrol` | 巡逻任务节点 |
| `Strike` | 打击决策节点（攻击方/目标/武器/命中概率/毁伤等级） |

### Entity 标准字段

| 序号 | 分类 | 名称/注释 | 编码 | 字段类型 | 长度 | 格式 | 备注 |
|------|------|-----------|------|----------|------|------|------|
| 1 | 基础字段 | 技术主键 | `id` | 字符串 | 50 | | 图全局唯一，静态 |
| 2 | | 关联业务主键 | `unit_id` | 字符串 | 50 | | mysql绑定持久化数据的唯一主键 |
| 3 | | 硬盘图主键 | `graph_id` | 字符串 | 50 | | neo4j内存图的主键 |
| 4 | | 领域 | `domain` | 字符串 | 50 | | 领域ID，业务领域划分，给算法用的 |
| 5 | | 层级 | `leven` | INT | 4 | | 本体层级L0-L5，给算法用的 |
| 6 | | 编码 | `code` | 字符串 | 100 | | 业务编码 |
| 7 | | 名称 | `name` | 字符串 | 200 | | 名称 |
| 8 | | 类型 | `type` | 字符串 | 4 | | 本体类型：M1对象/M2行为/M3规则/M4场景/M5主体/M6异常补偿/M7质量约束/ME事件 |
| 9 | | 更新时间 | `update_time` | DateTime | 到秒即可 | | neo4j默认支持的时间格式 |
| 10 | | 创建时间 | `create_time` | DateTime | 到秒即可 | | neo4j默认支持的时间格式 |
| 11 | | 置信度 | `confidence` | Double | 4 | | 百分数 |
| 12 | | 静态约束 | `static_rule_id` | | | 对对象本身 |
| 13 | | 动态约束 | `dynamic_rule_id` | | | |
| 14 | | 速度 | `speed` | FLOAT | | | m/s |
| 15 | | 电量 | `power` | FLOAT | | | |
| 16 | | 描述 | `description` | 字符串 | 500 | | |
| 17 | | 状态 | `status` | 字符串 | 4 | | 枚举值：有效/无效 |
| 18 | | 版本 | `version` | 字符串 | 10 | | |
| 19 | | 副本版本 | `cope_version` | 字符串 | 10 | | 推演环境的副本版本 |
| 20 | | 来源 | `source` | 字符串 | 50 | | |
| 21 | | 维护人员 | `owner` | 字符串 | 50 | | |
| 22 | | 父本体 | `parent_id` | 字符串 | 50 | | |
| 23 | 行为字段 | 前置条件/约束 | `precondition` | 字符串 | 500 | | SWRL语法 |
| 24 | | 执行效果 | `effect` | 字符串 | 500 | | SWRL语法 |
| 25 | | 资源消耗 | `cost` | 字符串 | 200 | | SWRL语法 |
| 26 | | 持续时间 | `duration` | INT | 200 | 5000 | 默认是s |
| 27 | | 优先级 | `priority` | INT | 200 | | 行为等级 |
| 28 | | 子动作组合 | `composedOf` | 字符串 | 500 | | 关系类型，用分号区分 |
| 29 | 空间字段 | 绝对位置 | `Space_abs` | 数组[纬度/经度/深度/高度] | 5 | [23.1291,12.8,-30,] | |
| 30 | 边属性 | 红蓝方 | `command_side` | INT | 4 | | 0红方，1蓝方，2中立，3不确定 |

### 约束

```cypher
CREATE CONSTRAINT entity_code_constraint FOR (e:Entity) REQUIRE e.code IS UNIQUE
CREATE CONSTRAINT entity_id_constraint   FOR (e:Entity) REQUIRE e.id   IS UNIQUE
```

### Point 迁移脚本

```cypher
-- 并行写入 (Phase 1)
MATCH (n:Entity) WHERE n.Space_abs IS NOT NULL
SET n.location = point({
  latitude: n.Space_abs[0], longitude: n.Space_abs[1], z: n.Space_abs[2]
})

-- 空间索引 (Phase 3)
CREATE INDEX entity_location FOR (n:Entity) ON (n.location)

-- 回滚
MATCH (n:Entity) SET n.location = null
```

---

## 15. DWL2 DL + SWRL

### DWL2（实体查询，只读）

12 种构造子。`ClassExpression::to_key()/from_key()` 字符串序列化。JSON 备选格式规划中。

### SWRL（推理函数，修改图）

7 种 Atom（含 `Query` DWL2 子查询）。Fixpoint 循环，置信度熔断。

**规则加载优先级**：Neo4j `(:Rule)` > 文件 `rules/*.swrl` > HTTP `POST /reason`

---

## 16. HTTP API 端点

| 方法 | 路径 | 状态 | 作用 |
|------|------|------|------|
| GET | `/health` | ✅ | 健康检查 + Entity/Type 计数 |
| GET | `/schema` | ✅ | 知识图谱数据模型定义 |
| GET | `/tools` | ✅ | LLM Function Calling 定义（12 个工具） |
| POST | `/tools/call` | ✅ | LLM 统一调度入口（OpenAI FC 兼容） |
| POST | `/query` | ✅ | Entity 搜索（code/type/红蓝方/空间/关键词/层级） |
| POST | `/reason` | ✅ | SWRL 推理（含置信度熔断 422） |
| POST | `/context` | ✅ | Entity 图上下文（关系+Type 层级链+摘要） |
| GET\|POST | `/patrol` | ✅ | 巡逻任务查询/提交 + 时序推演 |
| GET\|POST | `/strike` | ✅ | 打击决策推演（射程/命中/毁伤） |
| POST | `/nl-query` | ✅ | 自然语言 → DWL2 查询 |
| POST | `/infer-forward` | ✅ | 向前推理 — 自动遍历+状态推断+规则匹配+下一步预测 |
| POST | `/ontology/create` | ✅ | LLM 创建本体（Entity/Type/Patrol + 批量） |
| POST | `/relationships/create` | ✅ | LLM 创建关系（自动节点解析） |
| POST | `/confidence/policy` | ✅ | 切换作战模式（Exercise/WarFighting/Training） |
| GET\|POST | `/rules` | ✅ | GET 列出已加载规则；POST 从 Neo4j/文件热加载 SWRL 规则 |
| GET\|PUT | `/confidence/policy` | 📋 | 查询/持久化策略 |

---

## 17. 启动 & 测试

```bash
# 配置（首次使用）
cp .env.example .env              # 编辑 .env 填入 Neo4j 连接信息

# HTTP 服务
cargo run -p ontology-server                    # :8085 (neo4j 后端，默认)
cargo run -p ontology-server --no-default-features --features in-memory  # 内存后端
ONTOLOGY_PORT=9090 cargo run -p ontology-server

# 测试
cargo test --workspace                           # 61 个单元测试 + 1 个 doctest
cargo build --features neo4j                     # Neo4j 编译
```

---

## 18. 设计原则

1. **异步优先** — `axum` + `tokio` 替代同步模型
2. **存储抽象** — 推理层不直接构造 IR，通过 trait 访问
3. **空间独立** — Haversine 等抽离为独立 crate (`ontology-spatial`)
4. **置信度策略化** — 动态权重 + 外部 Policy 注入
5. **Point 渐进迁移** — 双写 → 切换 → 清理，可回滚
6. **规则热加载** — 从 Neo4j 读取，不需重启
7. **日志结构化** — `logs/` 文件 → `tracing` JSON
8. **安全内建** — API Key 认证，环境变量注入
9. **渐进重构** — 保持可用，逐步迁移
10. **分类层级** — `(:Type)-[:subClassOf]->(:Type)` 树
11. **UTF-8 编码** — 所有源文件（`.rs`/`.toml`/`.md`/`.cypher`/`.swrl`/`.py`/`.json`）统一 UTF-8（无 BOM），禁止其他编码
12. **非业务层不写业务代码** — `ontology-storage`（存储适配）和 `ontology-server`（HTTP 网关）只做协议/传输/持久化，不包含推理、置信度计算、规则解析等业务逻辑。业务代码唯一归宿是 `ontology-reasoner`。判断标准：如果代码里出现了 DWL2/SWRL/Confidence/Timeline/Spatial 任意一个词，它就不该在 storage 或 server crate 里
13. **产品/业务分层 (v0.3)** — `ontology-reasoner::graph` 是**产品层**（框架），定义 `GraphExplorer`（通用图遍历）、`StateChangeDetector` trait（可插拔检测器接口）和通用 util 函数。`ontology-server::routes::infer` 是**业务层**，通过实现 `StateChangeDetector` trait 的 `MilitaryStateChangeDetector` 注入领域知识（Space_abs/haversine/中文关系语义）。产品层不依赖 serde_json/tiny_http，不包含任何领域概念。换业务场景只需重新实现 trait，产品代码完全不动
