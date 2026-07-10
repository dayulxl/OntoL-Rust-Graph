# Ontologica Rust Graph — 架构文档 v0.3

> **名称约定**：项目名为 `ontologica_rust_graph`，各 crate 前缀 `ontology-`。

> **术语约定**：**"本体"** 即 **图节点**（Node）。Entity/Type/Patrol 统称本体。`(:Entity)`、`(:Type)`、`(:Patrol)` 是 label，`[:移动]`、`[:subClassOf]` 是 relationship。非 RDF/OWL 标准本体。

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
               │ Bolt Protocol (neo4rs → Memgraph) 📋
┌──────────────▼──────────────────────────────────┐
│        Memgraph / Vector DB / Config Store       │
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
| `ontology-storage` | 外部驱动库 (neo4rs)；标准库 | 反向依赖 engine 或 gateway |

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
| Memgraph 连接 | Bolt 协议 (neo4rs) | 同步桥接 async |
| 推理循环 | 同步 fixpoint | 无并行 |

### 3.2 异步迁移三阶段

#### Phase 1（当前可实施）：网关异步化

- `tiny_http` → `axum` (tokio)
- 推理调用仍为同步，通过 `tokio::task::spawn_blocking` 桥接
- 新增 `tower` 中间件：CORS、限流、超时
- 推理引擎使用**专用线程池**（`rayon` 或 `tokio blocking pool`），避免阻塞 I/O 工作线程

#### Phase 2（当前已实现）：存储已就绪

- `neo4rs` Bolt 驱动 → Memgraph（已实现）
- SWRL 规则并发执行（`std::thread::scope`，已实现）
- 场景版本隔离（全量克隆 + 副本守卫，已实现）
- `GraphRepository` trait 增加 `async fn` 方法（`add_async_*`，逐步迁移）
- 配置管理：Memgraph 连接信息、日志级别、策略默认值通过 `config` crate 或环境变量管理
- **认证**：网关加 API Key 验证中间件

#### Phase 3（工程优化）：推理异步化

- SWRL 规则循环内多规则并行匹配（`tokio::spawn`）
- 中间结果通过异步通道共享，读写冲突通过规则级隔离处理（每条规则独立事实集，merge 阶段处理冲突）
- 推理引擎 state 管理：事实集按规则隔离，fixpoint merge 时去重

### 3.3 配置管理

```rust
pub struct ServerConfig {
    pub port: u16,                     // ONTOLOGY_PORT, 默认 8085
    pub graph_uri: String,             // ONTOLOGY_GRAPH_URI, 默认 memgraph://localhost:7687
    pub graph_user: String,            // ONTOLOGY_GRAPH_USER, 默认空
    pub graph_password: String,        // ONTOLOGY_GRAPH_PASSWORD, 默认空
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
  2. 语义定位: 用户查询 → Embedding → Memgraph Vector Index 检索 top_k=5
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

- 运行时调用 `POST /confidence/policy` 的同时写入 Memgraph 配置节点 `(:Config { key: "policy", mode: "WarFighting" })`
- 启动时 `Reasoner::new()` 从 Memgraph 读取 `(:Config)` 节点，存在则恢复策略，不存在则用默认
- `GET /confidence/policy` 返回当前策略 + 是否持久化标记

---

## 8. 规则加载机制

### 8.1 当前

SWRL 规则通过 `Reasoner::load_swrl_rule(text)` 运行时加载，重启即失。

### 8.2 加载路径（优先级从高到低）

1. **Memgraph 存储**：`(:Rule { name, text, enabled, priority })` 节点。启动时自动加载 `enabled=true` 的规则
2. **文件**：`rules/` 目录下的 `.swrl` 文件。适用于开发阶段
3. **HTTP**：`POST /reason { "rules": ["..."] }` 临时加载，调用后生效

**热加载**：`POST /rules/reload` 从图数据库重新读取规则，不需重启。

---

## 9. 运维、安全与可观测性

### 9.1 安全

| 层级 | 措施 | 状态 |
|------|------|------|
| HTTP 网关 | API Key 验证中间件 (环境变量 `ONTOLOGY_API_KEY`) | 📋 |
| Memgraph | 密码通过环境变量注入，不硬编码 | ✅ |
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
ONTOLOGY_GRAPH_URI=memgraph://localhost:7687        # Memgraph Bolt API 地址
ONTOLOGY_GRAPH_USER=                                # 图数据库用户名，默认空
ONTOLOGY_GRAPH_PASSWORD=                            # 图数据库密码，默认空
ONTOLOGY_MODE=Exercise                             # 默认推理策略: Exercise|WarFighting|Training
RUST_LOG=info                                      # 日志级别 (tracing/env_logger)
```

> ⚠ 生产环境务必配置认证。Memgraph 默认无认证，本地 Docker 直连即可，部署时通过 `--auth` 启用密码。

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
| `ontology-server/` (Memgraph 后端) | E2E | 0 个 | 📋 需要 |

**总计**：61 个单元测试 (lib) + 1 个 doctest，0 个集成测试 (bin)。CI 待配置。

---

## 11. Python 脚本维护

| 脚本 | 用途 | 策略 |
|------|------|------|
| `import_all.py` | Entity 批量导入 | 标记为一次性迁移工具，纳入 `scripts/` 目录 |
| `import_types.py` | Type 层级导入 | 同上 |
> Python 脚本仅作为运维辅助，核心逻辑均在 Rust 中。未来可移植为 Rust CLI 子命令。

---

## 12. Feature Flags

| Feature | 当前依赖 | 目标依赖 | 说明 |
|---------|---------|---------|------|
| `in-memory` | 无（std only） | `dev-dependency` only | 仅用于单元测试 |
| `memgraph` | neo4rs, tokio, serde_json | **主力** | Bolt 异步驱动 |
| `llm` | serde, serde_json | 迁至 `ontology-gateway` | LLM 数据格式层 |

---

## 13. 目录结构（v0.3 现状）

```
ontologica_rust_graph/
├── Cargo.toml                              # workspace root
├── .env                                    # 环境变量配置（Memgraph 连接等）
├── src/main.rs                             # CLI 推理演示
├── scripts/                                # 运维脚本（Python）
│   ├── import_all.py                       #   Entity 批量导入
│   ├── import_types.py                     #   Type 层级导入
│   ├── import_props.py                     #   属性导入
│   ├── create_p8a.py                       #   P-8A 测试数据创建
│   ├── test_patrol.py                      #   巡逻 API 测试
│   └── verify.py                           #   数据验证
├── rules/                                  # SWRL 规则文件
│   └── asw_rules.swrl
├── logs/                                   # 推理日志输出
│
├── crates/
│   ├── ontology-storage/                   # 存储适配层
│   │   └── src/
│   │       ├── repository/                 # GraphRepository trait + Transaction
│   │       ├── adapters/                   # memgraph (Bolt) + in_memory
│   │       ├── mapper/                     # 属性图 IR (graph/llm)
│   │       └── ontology/                   # Entity/Type/Patrol/Relationship CRUD
│   │
│   ├── ontology-reasoner/                  # 核心推理引擎
│   │   ├── CRATE_CONSTRAINTS.md             #   ✅ 约束说明文档 (v1.0)
│   │   └── src/
│   │       ├── graph/                      # 通用图遍历 — 产品层框架 (v0.3)
│   │       │   ├── explorer.rs             #   GraphExplorer — BFS 多跳遍历
│   │       │   ├── detector.rs             #   StateChangeDetector trait — 可插拔
│   │       │   ├── util.rs                 #   实体查找/关系汇总/类型层次/规则匹配
│   │       │   ├── actions.rs              #   自定义逻辑函数 (ActionFunction trait)
│   │       │   └── README.md               #   模块文档
│   │       ├── dwl2/                       # DWL2 查询 (12 种构造子)
│   │       │   ├── ast.rs / query.rs       #   ClassExpression + Dwl2QueryEngine
│   │       │   └── README.md               #   模块文档
│   │       ├── swrl/                       # SWRL 推理 (7 种 Atom + fixpoint + behavior 并发)
│   │       │   ├── ast.rs / parser.rs      #   AST + 文本解析
│   │       │   ├── builtins.rs             #   内置函数（比较/数学/字符串）
│   │       │   ├── engine.rs               #   规则执行引擎（thread::scope 并发）
│   │       │   ├── behavior.rs             #   行为动作引擎（6 字段 + composedOf 递归）
│   │       │   └── README.md               #   模块文档
│   │       ├── shacl/                      # ✅ SHACL 图约束验证
│   │       │   ├── ast.rs                  #   Shape / Constraint / Target / PropertyPath
│   │       │   ├── engine.rs               #   ShaclEngine — 验证引擎
│   │       │   ├── result.rs               #   ValidationResult / ValidationReport
│   │       │   └── error.rs                #   ShaclError
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
│   │       │   ├── entity_update.rs        # POST /entity/update — 修改实体属性
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
| 3 | | 硬盘图主键 | `graph_id` | 字符串 | 50 | | Memgraph内存图的主键 |
| 4 | | 领域 | `domain` | 字符串 | 50 | | 领域ID，业务领域划分，给算法用的 |
| 5 | | 层级 | `leven` | INT | 4 | | 本体层级L0-L5，给算法用的 |
| 6 | | 编码 | `code` | 字符串 | 100 | | 业务编码 |
| 7 | | 名称 | `name` | 字符串 | 200 | | 名称 |
| 8 | | 类型 | `type` | 字符串 | 4 | | 本体类型：M1对象/M2行为/M3规则/M4场景/M5主体/M6异常补偿/M7质量约束/ME事件 |
| 9 | | 更新时间 | `update_time` | DateTime | 到秒即可 | | Memgraph默认支持的时间格式 |
| 10 | | 创建时间 | `create_time` | DateTime | 到秒即可 | | Memgraph默认支持的时间格式 |
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

**规则加载优先级**：Memgraph `(:Rule)` > 文件 `rules/*.swrl` > HTTP `POST /reason`

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
| GET\|POST | `/rules` | ✅ | GET 列出已加载规则；POST 从 Memgraph/文件热加载 SWRL 规则 |
| GET\|PUT | `/confidence/policy` | 📋 | 查询/持久化策略 |

---

## 17. 启动 & 测试

```bash
# 配置（首次使用）
cp .env.example .env              # 编辑 .env 填入 Memgraph 连接信息

# HTTP 服务
cargo run -p ontology-server                    # :8085 (Memgraph 后端，默认)
cargo run -p ontology-server --no-default-features --features in-memory  # 内存后端
ONTOLOGY_PORT=9090 cargo run -p ontology-server

# 测试
cargo test --workspace                           # 61 个单元测试 + 1 个 doctest
cargo run -p ontology-server                     # Memgraph 编译
```

---

## 18. 设计原则

1. **异步优先** — `axum` + `tokio` 替代同步模型
2. **存储抽象** — 推理层不直接构造 IR，通过 trait 访问
3. **空间独立** — Haversine 等抽离为独立 crate (`ontology-spatial`)
4. **置信度策略化** — 动态权重 + 外部 Policy 注入
5. **Point 渐进迁移** — 双写 → 切换 → 清理，可回滚
6. **规则热加载** — 从 Memgraph 读取，不需重启
7. **日志结构化** — `logs/` 文件 → `tracing` JSON
8. **安全内建** — API Key 认证，环境变量注入
9. **渐进重构** — 保持可用，逐步迁移
10. **分类层级** — `(:Type)-[:subClassOf]->(:Type)` 树
11. **UTF-8 编码** — 所有源文件（`.rs`/`.toml`/`.md`/`.cypher`/`.swrl`/`.py`/`.json`）统一 UTF-8（无 BOM），禁止其他编码
12. **非业务层不写业务代码** — `ontology-storage`（存储适配）和 `ontology-server`（HTTP 网关）只做协议/传输/持久化，不包含推理、置信度计算、规则解析等业务逻辑。业务代码唯一归宿是 `ontology-reasoner`。判断标准：如果代码里出现了 DWL2/SWRL/Confidence/Timeline/Spatial 任意一个词，它就不该在 storage 或 server crate 里
13. **产品/业务分层 (v0.3)** — `ontology-reasoner::graph` 是**产品层**（框架），定义 `GraphExplorer`（通用图遍历）、`StateChangeDetector` trait（可插拔检测器接口）和通用 util 函数。`ontology-server::routes::infer` 是**业务层**，通过实现 `StateChangeDetector` trait 的 `MilitaryStateChangeDetector` 注入领域知识（Space_abs/haversine/中文关系语义）。产品层不依赖 serde_json/tiny_http，不包含任何领域概念。换业务场景只需重新实现 trait，产品代码完全不动
14. **宽容执行 (Tolerant Execution v1.0)** — 所有设计必须是灵活的：**有这个字段，就用；没有这个字段，跳过继续执行。有这个值，就处理；没有这个值，用默认值兜底继续执行。** 系统在任何情况下都不因字段缺失或值为空而崩溃或中断链路。这是贯穿全栈的第一性约束：存储层、推理层、HTTP 层、LLM 交互层、序列化/反序列化、规则引擎、行为引擎，无一例外
15. **id 锚定 (Identity Anchor v1.0)** — 每个节点/实体的 `id` 字段是数据的**唯一技术标识符**，作为锁定数据的技术组件。所有 CRUD 操作（查询/更新/删除）均以 `id` 为锚点，`id` 由系统生成，前端不可修改。`code` 和 `name` 仅作业务语义字段，用于展示与搜索，不作为数据定位的技术锚点

---

## 19. 宽容执行约束（Tolerant Execution）

> **一句话**：有和没有，都继续执行。不因缺失而中断，不因空白而崩溃。

### 19.1 核心原则

```
          有字段/有值 ──→ 正常处理 ──→ 继续
         /
请求/节点 ──→ 无字段/无值 ──→ 跳过或默认值兜底 ──→ 继续
         \
          无字段/无值 ──→ 记录日志(warn/debug) ──→ 继续

禁止路径：无字段/无值 ──→ panic / Err 传播 / 链路中断
```

### 19.2 各层约束

#### 存储层 (`ontology-storage`)

| 场景 | 约束 | 示例 |
|------|------|------|
| 节点属性缺失 | `node.property("x")` 返回 `None` 时用默认值，不报错 | `node.property("speed").unwrap_or(0.0)` |
| 关系不存在 | `get_relationships()` 返回空列表时继续，不视为错误 | 推理层收到空列表 = "此节点无关系"，正常处理 |
| 查询无结果 | 空结果集，不是错误 | `vec![]` 而非 `Err(...)` |
| 类型转换失败 | `parse::<i64>()` 失败 → 用默认值 + log | `s.parse().unwrap_or(0)` |
| 连接断开 | 重试 + 降级（in-memory fallback），不 panic | Memgraph 挂了 → InMemory 兜底 + 空数据继续 |

#### 推理层 (`ontology-reasoner`)

| 场景 | 约束 | 示例 |
|------|------|------|
| SWRL 前置条件无绑定 | 阻断此条规则，继续下一条 | `binding_count == 0 → blocked_reason = "无匹配"` |
| DWL2 查询表达式中途无结果 | 返回空集，上游自行处理 | `ClassExpression` 评估 → `empty()` |
| 置信度计算参数缺失 | 用策略默认权重 | `ConfidencePolicy::default_weights()` |
| 行为引擎字段缺失 | 6 个行为字段任一为空 → 跳过该字段，其余继续 | `precondition` 为空 → 无前置条件，直接执行 effect |
| 规则解析失败 | 跳过该规则，继续解析下一条 | `parse_rule()` → `warn!("规则 {} 解析失败，跳过", name)` |

#### HTTP 层 (`ontology-server`)

| 场景 | 约束 | 示例 |
|------|------|------|
| 请求体 JSON 字段缺失 | `serde_json` 反序列化时用 `Option<T>` + `#[serde(default)]` | `field: Option<String>` 或 `#[serde(default)] field: String` |
| 查询参数缺失 | 用默认值 | `page.unwrap_or(1)`, `limit.unwrap_or(20)` |
| 推理结果为空 | 返回 200 + `{"results": [], "reason": "no_match"}` | 不返回 4xx/5xx |
| 下游服务超时 | 返回部分结果 + 降级标记 | `{"partial": true, "reason": "timeout"}` |
| LLM 返回解析失败 | 返回原始文本 + 解析错误信息，不中断 | `{"raw": "...", "parse_error": "..."}` |

#### 序列化 / 反序列化

| 场景 | 约束 |
|------|------|
| `ClassExpression::to_key()` | 任意表达式都能生成 key，不 panic |
| `ClassExpression::from_key()` | 解析失败 → `Err` 由调用方降级为默认表达式 |
| JSON 反序列化 | 未知字段忽略（`#[serde(deny_unknown_fields)]` **禁用**） |
| 枚举反序列化 | 未知 variant → fallback variant（如 `Other(String)`） |

### 19.3 代码模式

#### ✅ 正确模式

```rust
// 模式 1：缺失用默认值
let speed = node.property("speed").unwrap_or(0.0);
let name = node.property("name").unwrap_or_else(|| "未命名".to_string());
let priority = entity.get("priority").and_then(|v| v.as_i64()).unwrap_or(0);

// 模式 2：Option 链式处理
if let Some(effect) = entity.effect() {
    engine.execute_effect(&effect, &bindings)?;
}
// effect 为空 → 跳过，继续

// 模式 3：空集合正常返回
let relationships = repo.get_relationships(&node_id, None)?;
// 返回 vec![] 是正常情况，不是错误
for rel in relationships {
    // 有就处理，没有就跳过
}

// 模式 4：部分成功
let results: Vec<Result<Item, Error>> = items.iter().map(process).collect();
let successes: Vec<_> = results.iter().filter_map(|r| r.ok()).collect();
let failures: Vec<_> = results.iter().filter_map(|r| r.err()).collect();
log::warn!("{}/{} 处理成功，{} 跳过", successes.len(), items.len(), failures.len());
// 继续用 successes

// 模式 5：JSON 反序列化宽容
#[derive(Deserialize)]
struct QueryRequest {
    query: String,                        // 必填
    #[serde(default)]
    page: Option<usize>,                  // 可选，缺失 = None
    #[serde(default = "default_limit")]
    limit: usize,                         // 可选，缺失 = 20
}

// 模式 6：规则引擎宽容
for rule in rules {
    match engine.execute_rule(rule) {
        Ok(result) => results.push(result),
        Err(e) => log::warn!("规则 {} 执行失败，跳过: {}", rule.name, e),
    }
    // 继续下一条规则
}
```

#### ❌ 反模式（禁止）

```rust
// 反模式 1：unwrap 裸用
let speed = node.property("speed").unwrap();  // ❌ speed 为 None 时 panic

// 反模式 2：缺失即报错
let name = entity.name.ok_or(anyhow!("name 字段缺失"))?;  // ❌ 用默认值，不要传播 Err

// 反模式 3：空集合报错
if results.is_empty() {
    return Err(anyhow!("查询无结果"));  // ❌ 空结果是正常情况
}

// 反模式 4：严格要求字段存在
if entity.effect.is_none() {
    return Err(anyhow!("effect 字段缺失，无法执行"));  // ❌ 没有 effect 就跳过
}

// 反模式 5：拒绝未知字段
#[serde(deny_unknown_fields)]  // ❌ 禁止！新加字段会破坏旧客户端
struct Request { ... }
```

### 19.4 日志级别约定

| 情况 | 级别 | 说明 |
|------|------|------|
| 字段缺失，使用默认值 | `debug!` | 正常降级，不产生告警噪音 |
| 字段缺失，跳过处理逻辑 | `debug!` | 设计如此，不是异常 |
| 规则/表达式解析失败，跳过该条 | `warn!` | 值得关注但不影响整体 |
| 连接失败，降级到备用 | `warn!` | 运维需关注 |
| 所有备用路径耗尽的致命失败 | `error!` | 仅此情况用 error |

### 19.5 测试检查清单

每个模块的测试必须覆盖：

- [ ] 所有字段都存在 → 正常执行
- [ ] 关键字段缺失 → 不 panic，不返回 Err
- [ ] 所有字段缺失 → 不 panic，返回合理默认值
- [ ] 下游返回空 → 不 panic，空结果正常返回
- [ ] 下游返回错误 → 降级，不 panic
- [ ] JSON 多出未知字段 → 反序列化成功
- [ ] JSON 缺少可选字段 → 反序列化成功
- [ ] 规则/表达式语法错误 → 跳过该条，继续执行其余

---

## 20. 数据标识与访问约束（Identity Anchor）

> **一句话**：`id` 是唯一技术锚点，CRUD 以此锁定数据。`code`/`name` 仅供展示与搜索。

### 20.1 角色定义

```
┌─────────────────────────────────────────────────────────┐
│                    实体标识分层                          │
│                                                         │
│  ┌──────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │    id    │  │     code     │  │      name        │  │
│  │ 技术主键  │  │   业务编码    │  │     名称         │  │
│  │ 不可修改  │  │   可修改      │  │    可修改        │  │
│  │ 系统生成  │  │  语义搜索     │  │   展示用         │  │
│  └────┬─────┘  └──────┬───────┘  └────────┬─────────┘  │
│       │               │                   │            │
│       ▼               ▼                   ▼            │
│  ┌────────────┐  ┌────────────┐  ┌─────────────────┐   │
│  │ CRUD 锚点  │  │ 辅助定位   │  │  可读标识       │   │
│  │ 锁定/删除  │  │ 模糊搜索   │  │  日志/报告      │   │
│  └────────────┘  └────────────┘  └─────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

| 字段 | 角色 | 生成方 | 可修改 | 唯一性 | 用途 |
|------|------|--------|--------|--------|------|
| `id` | **技术主键** | 系统 | ❌ 不可修改 | 图全局唯一 | 查询/更新/删除的锁定锚点 |
| `code` | 业务编码 | 系统或用户 | ✅ 可修改 | 业务域内唯一 | 语义搜索、模糊匹配、前端展示 |
| `name` | 名称 | 用户 | ✅ 可修改 | 不保证唯一 | 可读标识、日志、报告 |

### 20.2 各层约束

#### 存储层 — 以 id 为唯一索引

```cypher
-- 唯一约束建立在 id 上
CREATE CONSTRAINT entity_id_constraint FOR (e:Entity) REQUIRE e.id IS UNIQUE

-- code 也可建约束，但不是技术锚点
CREATE CONSTRAINT entity_code_constraint FOR (e:Entity) REQUIRE e.code IS UNIQUE
```

| 操作 | 锚点 | 规则 |
|------|------|------|
| 查询单条 | `id` | `MATCH (n) WHERE n.id = $id` — 精确匹配 |
| 更新 | `id` | `MATCH (n {id: $id}) SET n += $props` — 锁定后修改 |
| 删除 | `id` | `MATCH (n {id: $id}) DETACH DELETE n` — 精确删除 |
| 搜索 | `code` / `name` | `WHERE n.code CONTAINS $kw OR n.name CONTAINS $kw` — 模糊匹配 |
| 关系创建 | `id` | `MATCH (a {id: $from_id}), (b {id: $to_id}) CREATE (a)-[:REL]->(b)` |

#### 推理层 — id 贯穿全链路

| 场景 | 约束 |
|------|------|
| SWRL 规则匹配 | 绑定变量使用 `id` 追踪实体，`code`/`name` 仅参与条件匹配 |
| 副本克隆 | 克隆体保留原 `id`，`cope_version` 区分版本 |
| 事实去重 | `HashSet<String>` 以 `id` (或 `id + 事实描述`) 为去重键 |
| BFS 遍历 | 每跳锁定目标节点均以 `id` 为锚点 |

#### HTTP 层 — id 不黑盒

| 端点 | 锚点 | 说明 |
|------|------|------|
| `GET /query` | `code` 或关键词 | 搜索入口，返回结果包含 `id` |
| `POST /entity/update` | `id` | `{"id": "P8A_001", "updates": {...}}` |
| `POST /context` | `code` 或 `id` | 接受两者，内部先解析为 `id` 再展开 |
| `POST /relationships/create` | `id` | `from_id` / `to_id` 定位节点 |
| 前端展示 | `code` + `name` | 渲染列表/详情，`id` 仅透传不回显 |

### 20.3 代码模式

#### ✅ 正确模式

```rust
// 模式 1：通过 id 精确定位单条记录
fn find_by_id(repo: &SharedRepository, id: &str) -> Option<Node> {
    repo.get_nodes_by_property("id", &PropertyValue::from(id))
        .ok()?
        .into_iter()
        .next()
}

// 模式 2：更新以 id 为锚点
fn update_entity(repo: &SharedRepository, id: &str, props: HashMap<String, PropertyValue>) {
    // id 不可被覆盖
    let mut props = props;
    props.remove("id");
    repo.update_node(id, &props);
}

// 模式 3：搜索用 code/name，返回带 id
fn search_by_code(repo: &SharedRepository, code: &str) -> Vec<Node> {
    repo.get_nodes_by_property("code", &PropertyValue::from(code))
        .unwrap_or_default()
    // 返回结果中 id 始终存在，前端可透过 id 做后续精确操作
}

// 模式 4：关系创建用 id 锚定
fn create_relation(repo: &SharedRepository, from_id: &str, to_id: &str, rel_type: &str) {
    // 先通过 id 确认两端节点存在
    let from = find_by_id(repo, from_id);
    let to = find_by_id(repo, to_id);
    if from.is_some() && to.is_some() {
        repo.create_relationship(from_id, to_id, rel_type);
    }
}
```

#### ❌ 反模式（禁止）

```rust
// 反模式 1：通过 code/name 执行更新/删除
repo.delete_node_by_property("code", code);  // ❌ code 可重复/可修改，可能误删

// 反模式 2：前端直接拼 id
let id = request.body.get("id").unwrap();     // ❌ id 应为系统透传，不从前端输入生成

// 反模式 3：用 name 做关系锚点
MATCH (a {name: $name})-[r]->(b) ...           // ❌ name 不保证唯一

// 反模式 4：搜索接口不返回 id
{ "results": [{"code": "P8A", "name": "P-8A"}] }  // ❌ 前端无法用此结果做后续精确操作
```

### 20.4 字段优先级（搜索链）

当查询接口同时收到 `id`、`code`、`name` 时，优先级如下：

```
1. id   ──→ 精确匹配，命中即返回（短路）
2. code ──→ 精确匹配，命中即返回
3. code ──→ 模糊匹配 (CONTAINS)
4. name ──→ 模糊匹配 (CONTAINS)
5. 其他搜索字段 (type / domain / leve 等)
```

**原则**：精确优先 → 业务编码其次 → 名称兜底。每一步有结果就短路返回，无结果则继续下一步。
