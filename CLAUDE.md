# ontologica_rust_graph — 项目工程规范

> 版本: 1.3 | 最后更新: 2026-07-05 | 适用范围: 全部 crate

---

## 1. 质量目标（可度量）

| 指标 | 目标值 | 度量方式 |
|------|--------|----------|
| 编译通过率（`cargo check`） | 100%，提交前零告警 | `cargo check --workspace` |
| 测试通过率 | 100%，不允许失败用例 | `cargo test --workspace` |
| Clippy 告警 | 0 个 warning | `cargo clippy --workspace -- -D warnings` |
| `rustfmt` 一致性 | 100%，CI/enforce 级别 | `cargo fmt --check --all` |
| 构建时间（debug, ontology-server） | 基准待采集 | `cargo build -p ontology-server --timings` |

> CMMI5 过程域对齐：OPP（组织过程性能）— 基线建立后写入上方表格，每季度回顾。

---

## 2. 工具链

### 2.1 标准工具链

**`stable-x86_64-pc-windows-gnu`（GNU ABI）**

工具链版本锁定在 [`rust-toolchain.toml`](rust-toolchain.toml)，任何构建前先读取该文件确认版本。

MinGW-w64 通过 WinLibs 提供，路径已写入系统 PATH：

```
C:\Users\admin\AppData\Local\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin
```

切换命令：

```bash
rustup default stable-x86_64-pc-windows-gnu
```

### 2.2 为什么是 GNU 不是 MSVC？

MSVC 工具链需要完整 Visual Studio BuildTools（C++ workload + Windows SDK）。当前机器
VS BuildTools 安装不完整（缺 `link.exe` 和 `kernel32.lib`），GNU 工具链经 WinLibs
一站配齐（gcc + binutils + dlltool），无需外部依赖。

### 2.3 换机 / 换人时的环境搭建

```bash
# 1. 安装 Rust (rustup)
# 2. 添加 GNU 目标
rustup target add x86_64-pc-windows-gnu
rustup toolchain install stable-x86_64-pc-windows-gnu

# 3. 安装 MinGW-w64
winget install --id BrechtSanders.WinLibs.POSIX.UCRT --source winget

# 4. 把 MinGW bin 目录加入 PATH
setx PATH "%PATH%;C:\path\to\mingw64\bin"

# 5. 验证
rustup default stable-x86_64-pc-windows-gnu
gcc --version
cargo build -p ontology-server
```

> **Linux 部署**：直接使用对应 `x86_64-unknown-linux-gnu` 工具链，neo4rs 原生编译无差异。

---

## 3. 项目结构

```
ontologica_rust_graph/
├── Cargo.toml                  # workspace root
├── Cargo.lock                  # 锁定依赖（提交到版本控制）
├── rust-toolchain.toml         # 锁定工具链版本
├── .env.example                # 配置模板（敏感信息不入库）
├── crates/
│   ├── ontology-storage/       # 存储层：GraphRepository trait + adapters
│   │   └── src/
│   │       ├── mapper/
│   │       │   └── unified_mapping.rs  # 统一映射层：图 ↔ OWL2 词汇表 SSOT
│   │       └── adapters/
│   │           ├── memgraph/          # MemgraphAdapter（Bolt 协议，内存图，主力）
│   │           └── in_memory/         # InMemoryAdapter（测试用）
│   ├── ontology-reasoner/      # 推理引擎：DWL2 + SWRL + Behavior + Graph + SHACL + 置信度
│   └── ontology-server/        # HTTP API 服务（tiny_http）
└── src/                        # 演示用 main.rs
```

---

## 4. 编码规范

### 4.1 通用 Rust 规范

- 所有代码在提交前必须通过 `cargo fmt` 和 `cargo clippy -- -D warnings`
- 禁止 `unsafe` 代码，确需使用时必须在 MR 中写明理由并经过 review
- 错误处理：使用 `anyhow`（应用层）/ `thiserror`（库层），禁止裸 `unwrap()` 在非测试代码中出现
- 所有 `pub` 项必须有文档注释（`///`）
- 模块文件使用 `mod.rs` 而非同目录同名文件
- **编码前**：通读目标文件全部代码 + 所有关联函数/调用方的源码，确认没有遗漏再动手
- **编码后**：从头读一遍完整文件，`grep` 检查所有引用点，`cargo check` 确认零遗漏
- **冲突提醒**：如果新增/修改的代码与已有逻辑存在冲突，必须在写代码前或写完代码后立即指出。**禁止擅自写补偿代码绕过去**，由用户决定如何处理

### 4.2 命名约定

| 元素 | 约定 | 示例 |
|------|------|------|
| Crate | snake_case | `ontology-storage` |
| 类型 / Trait | PascalCase | `GraphRepository`, `MemgraphAdapter` |
| 函数 / 方法 | snake_case | `get_entity_by_id()` |
| 常量 / 静态 | SCREAMING_SNAKE_CASE | `MAX_POOL_SIZE` |
| 私有字段 | snake_case（无前缀） | `connection_pool` |

### 4.3 架构约束

- 存储层（`ontology-storage`）通过 `GraphRepository` trait 对外暴露，调用方不直接依赖 adapter 实现（依赖倒置）
- 跨 crate 依赖方向：`server → storage ← reasoner`（`storage` 不依赖其他业务 crate）
- 新增 adapter 只需在 `ontology-storage` 内部实现 `GraphRepository` trait，不应修改调用方代码（开闭原则）

---

## 5. 构建与测试

### 5.1 构建方式

```bash
# === 推荐：PowerShell（PATH 能正确传给 rustc 子进程）===
powershell -NoProfile -Command '
  $env:PATH = "C:\path\to\mingw64\bin;$env:PATH"
  cargo build -p ontology-server
'

# === 或：打开新的终端窗口（setx 后重启的终端）===
# 新终端会自动继承系统 PATH，MinGW 已就位
cargo build -p ontology-server

# === 不推荐：Git Bash ===
# Git Bash 的 export PATH 不会传给 cargo 的 rustc 子进程
```

### 5.1.1 启动服务

Git Bash 环境下 `cargo` 不在 `PATH` 中，必须通过 PowerShell 启动，同时传入 Rust 和 MinGW 路径：

```powershell
powershell -NoProfile -Command '
  $env:PATH = "C:\Users\admin\.cargo\bin;C:\Users\admin\AppData\Local\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin;$env:PATH"
  cargo run -p ontology-server
'
```

> **为什么是 PowerShell？** Git Bash 的 `export PATH` 不会传给 `cargo` 的 `rustc` 子进程，导致链接阶段找不到 MinGW 工具链。PowerShell 的 `$env:PATH` 能正确传递到整个进程树。

启动成功标志（标准输出）：

```
╔══════════════════════════════════════════╗
║  Ontology Server — LLM API Gateway       ║
╚══════════════════════════════════════════╝

🔌 连接 Memgraph ( @ memgraph://localhost:7687)...
   ✅ Memgraph 连接成功

   推理策略: Balanced, 阈值: 0.30

🚀 启动 HTTP 服务: http://0.0.0.0:8085
```

完整端点列表见 [6.3 启动验证](#63-启动验证)。

### 5.2 常用命令

```bash
cargo build -p ontology-server      # 构建
cargo run -p ontology-server        # 运行（默认 memgraph feature）
cargo check -p ontology-server      # 检查编译（不产生二进制，速度快）
cargo test --workspace              # 全量测试
cargo fmt --check --all             # 格式检查
cargo clippy --workspace -- -D warnings  # Lint 检查
cargo clean                         # 清理
```

### 5.3 提交前检查清单

```bash
cargo fmt --all          # 1. 格式化
cargo clippy --workspace -- -D warnings  # 2. Lint（零告警）
cargo check --workspace   # 3. 编译检查
cargo test --workspace    # 4. 全量测试通过
```

---

## 6. Memgraph 连接

### 6.1 配置

项目根目录 `.env` 文件（`dotenvy` 自动加载），模板见 `.env.example`：

```env
ONTOLOGY_GRAPH_URI=memgraph://localhost:7687
ONTOLOGY_GRAPH_USER=
ONTOLOGY_GRAPH_PASSWORD=
ONTOLOGY_PORT=8085
ONTOLOGY_MODE=Balanced
RUST_LOG=info
```

### 6.2 协议说明

Memgraph 兼容 Neo4j Bolt 协议，本项目通过 `ontology-storage` 的 `memgraph` feature
使用 `neo4rs` 驱动，走原生 Bolt 协议连接。

> ⚠️ **核心约束**：Memgraph 边属性仅支持标量类型
> （String / Int / Float / Bool / DateTime / Duration / Point / List）。
> **不支持 Map / JSON 嵌套**。
> 所有复合语义必须通过「扁平化 key-value」或「独立节点 + 关系」表达。

通过 Docker 启动 Memgraph：

```bash
docker run -d --name memgraph -p 7687:7687 -p 7444:7444 memgraph/memgraph-platform
```

> Memgraph 默认无认证，用户名密码留空即可。7444 是 Memgraph Lab Web UI 端口。

### 6.3 启动验证

启动命令见 [5.1.1 启动服务](#511-启动服务)。

**健康检查**：

```bash
curl http://localhost:8085/health
# 预期: {"backend":"memgraph","counts":{"entities":126,...},"status":"ok"}
# entities > 0 表示 Memgraph 连接正常
```

**完整端点列表**：

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/health` | 健康检查 |
| GET | `/tools` | 可用工具列表 |
| GET | `/schema` | 图结构信息 |
| POST | `/query` | Cypher 查询 |
| POST | `/reason` | 推理请求 |
| POST | `/context` | 上下文获取 |
| GET/POST | `/patrol` | 巡逻 |
| GET/POST | `/strike` | 打击 |
| POST | `/confidence/policy` | 置信度策略 |
| POST | `/nl-query` | 自然语言查询 |
| POST | `/ontology/create` | 创建本体实体 |
| POST | `/relationships/create` | 创建关系 |
| POST | `/tools/call` | 工具调用 |
| GET/POST | `/rules` | 规则管理 |
| POST | `/infer-forward` | 前向推理（BFS 图遍历） |
| POST | `/infer-on-nodes` | 推理机流水线（继承→克隆→推理→校验，逐层处理） |
| POST | `/entity/update` | 修改实体属性 |

### 6.4 行为动作引擎（BehaviorAction）

Entity 节点有 6 个**行为字段**（OWL2 风格属性名），全部为 String 类型：

| 字段 | 属性名 | 类型 | 作用 | 说明 |
|------|--------|------|------|------|
| 触发前约束 | `hasPrecondition` | String (SHACL) | **阻断** | SHACL 执行语言；True/空→通过，False→停止此节点所有函数 |
| 效果 | `hasEffect` | String | **触发** | 语言前缀路由（swrl:/sh:/owl2:），无前缀默认尝试 SWRL |
| 消耗 | `hasCost` | String | 记录 | 前置条件通过时记录消耗描述 |
| 持续时间 | `hasDuration` | String | 时序 | 秒；时长结束后触发效果；未写默认 0 秒即刻触发 |
| 优先级 | `hasPriority` | String (0-10) | **排序** | 10 级最高；冲突时高优先级先行；未写默认 0 |
| 组合动作 | `composedOf` | String | **跳转** | 分号分隔的 Entity code/名称列表，递归执行其行为 |

**默认行为**：任何字段缺失或为空时，不影响其他字段的执行。
只有 `hasPrecondition` 显式返回 False 时才会阻断整个节点的所有后续函数。

#### hasPrecondition SHACL 约束语法

| 表达式 | 含义 | 示例 |
|--------|------|------|
| (空) / `True` | 默认通过 | `""` 或 `"True"` |
| `False` | 阻断停止 | `"False"` |
| `prop = value` | 属性等于 | `"status = active"` |
| `prop != value` | 属性不等于 | `"status != destroyed"` |
| `prop >= N` | 数值大于等于 | `"power >= 50"` |
| `prop <= N` | 数值小于等于 | `"speed <= 100"` |
| `prop > N` | 数值大于 | `"confidence > 0.5"` |
| `prop < N` | 数值小于 | `"power < 30"` |
| `required(prop)` | 属性存在且非空 | `"required(code)"` |
| `exists(prop)` | 属性存在 | `"exists(name)"` |
| `prop matches "re"` | 正则匹配 | `"status matches \"^act\""` |

#### hasEffect 语言前缀路由

| 前缀 | 引擎 | 说明 |
|------|------|------|
| `swrl:` | SWRL 规则引擎 | 匹配前提 → 写入推导事实 |
| `sh:` | SHACL 验证引擎 | 合规性检查 |
| `owl2:` | DWL2 查询引擎 | 存在性检查 |
| (无前缀) | SWRL (默认) | 尝试 SWRL 解析，失败则跳过 |

**执行流程**：

```
1. 扫描所有 Entity → 解析 6 个字段 → BehaviorAction
2. 过滤：6 个字段全空的实体跳过，不参与计算
3. 按 hasPriority 降序 → hasDuration 升序排列（高优先级先行，同优先级短时长先行）
4. 对每个 action:
   a. 评估 hasPrecondition SHACL → True/空→通过，False→阻断停止
   b. 等待 hasDuration 时长（默认 0 秒即即刻触发）
   c. 解析 hasEffect 语言前缀 → 路由到对应引擎执行
   d. 记录 hasCost
   e. 递归处理 composedOf 链
```

**Reasoner 调用**：

```rust
let results = reasoner.execute_behaviors(max_depth)?;
// max_depth: composedOf 递归深度，0 表示不递归
```

**返回**：`Vec<BehaviorResult>`

```rust
struct BehaviorResult {
    entity_code: String,              // 实体 code
    triggered: bool,                  // 是否触发
    blocked_reason: Option<String>,   // 阻断原因
    binding_count: usize,             // 匹配绑定数
    derived_count: usize,             // 推导事实数
    priority_value: i64,              // 优先级数值
    priority_text: String,            // 优先级原始文本
    cost_description: String,         // 消耗描述
    duration_text: String,            // 持续时间原始文本
    duration_secs: i64,               // 持续时间秒数（解析后）
    precondition_blocked: bool,       // 是否因 hasPrecondition 阻断
    composed_results: Vec<BehaviorResult>, // 级联子动作
}
```

---

---

## 7. 推理机核心技术细节

### 7.1 场景版本与副本隔离（cope_version）

`infer_forward` 等推理操作**绝不修改原数据**。原实体 `cope_version` 为空。

**副本克隆流程**（`clone_all_for_version`）：

```
1. 全量扫描：收集所有标签（Entity/Event/Patrol/...）的节点
2. 筛选原实体：cope_version 为空的节点
3. 批量克隆：每个原实体创建版本副本（code 追加 _v{version} 后缀）
4. 关系复制：复制原实体间的所有关系到副本之间（A→B 变为 副本A→副本B）
5. 版本守卫：BFS 遍历时跳过 cope_version 不匹配的节点
```

**关键约束**：
- 关系只在副本之间创建，绝不指向原实体
- BFS 遍历每跳检查 `cope_version`，不同版本跳过
- 同一版本内的副本互相可见，与外部完全隔离

### 7.2 并发推理

SWRL 规则执行分为两阶段：

| 阶段 | 方式 | 实现 | 说明 |
|------|------|------|------|
| 规则匹配 | **并发** | `std::thread::scope` | N 条规则同时执行 `execute_rule`（只读 repo） |
| 事实写入 | **串行** | 主线程 | `insert_derived_facts` 去重 + 写图，避免并发写冲突 |

`execute_rule` 只通过 `query_pattern` 和 `get_relationships` 读取图，不写入。无共享可变状态，天然线程安全。

### 7.3 属性修改与副本保护

**HTTP 端点**：`POST /entity/update`

```json
{ "id": "P8A_001", "updates": {"status": "无效"}, "cope_version": "v2.0" }
```

**副本保护规则**：如果修改的目标实体 `cope_version` 为空（原实体），先调用 `ensure_cope_version` 克隆副本，在副本上修改，原实体不动。

### 7.4 图工具函数一览

| 函数 | 位置 | 职责 |
|------|------|------|
| `ensure_cope_version` | util.rs | 克隆单个实体到指定版本（只克隆节点） |
| `clone_all_for_version` | util.rs | 全量克隆图中所有原实体+关系到指定版本 |
| `delete_by_cope_version` | util.rs | 按版本号删除所有副本实体及关系 |
| `update_entity_properties` | util.rs | 修改实体属性，原实体自动先克隆副本 |
| `find_entity_by_id_code` | util.rs | 按 id+code 组合检索实体 |
| `find_entity_any` | util.rs | 分层匹配检索实体（iri→code→id→name→模糊） |

## 8. 技术决策记录（ADR）

所有对架构、依赖、协议的实质性决策必须记录为 ADR，存放在 `docs/adr/` 目录。

| 编号 | 决策 | 日期 | 状态 |
|------|------|------|------|
| ADR-001 | 选用 GNU ABI 工具链而非 MSVC | 2026-07-04 | 生效中 |
| ADR-002 | Memgraph 替代 Neo4j 作为主力图数据库 | 2026-07-05 | 生效中 |
| ADR-003 | storage 层使用 Bolt 协议而非 HTTP | 2026-07-04 | 生效中 |
| ADR-004 | Entity 行为引擎（BehaviorAction）用 SWRL 驱动 6 字段 | 2026-07-05 | 生效中 |
| ADR-005 | SWRL 规则并发执行（thread::scope） | 2026-07-05 | 生效中 |
| ADR-006 | 场景版本隔离：全量克隆+副本守卫，不修改原数据 | 2026-07-05 | 生效中 |
| ADR-007 | 统一映射层：W3C OWL2→属性图 SSOT，收拢~150处分散硬编码 | 2026-07-10 | 生效中 |
| ADR-008 | 图节点与边的全局唯一 ID 采用雪花算法（Snowflake ID），纯数字 i64，64-bit | 2026-07-10 | 生效中 |

## 8.1 统一映射层 (unified_mapping)

自 ADR-007 起，所有图词汇表（节点标签、关系类型、属性键）定义为
`ontology_storage::mapper::unified_mapping` 中的统一常量。禁止在其他模块中使用
硬编码字符串字面量（如 `"Entity"`、`"INSTANCE_OF"`、`"iri"`）。

**核心常量分组**：

| 分组 | 用途 | 示例 |
|------|------|------|
| 节点标签 | `get_nodes_by_label()` | `ENTITY_LABEL`, `CLASS_LABEL`, `INDIVIDUAL_LABEL` |
| 关系类型 | `get_relationships()`, `Relationship::simple()` | `INSTANCE_OF_REL`, `SUB_CLASS_OF_REL`, `HAS_PROPERTY_REL` |
| 属性键 | `node.property()` | `IRI_KEY`, `LABEL_KEY`, `COMMENT_KEY` |
| 预组合切片 | 循环遍历 | `DOMAIN_LABELS`, `OWL_NODE_LABELS`, `ONTOLOGY_SEMANTIC_RELS` |
| 标准审计字段 | 所有节点/关系通用 | `STD_ID_KEY`, `STD_CODE_KEY`, `CREATE_TIME_KEY`, `DELETE_FLAG_KEY`, `IS_SYSTEM_KEY` 等 9 个 |
| 边属性字段 | 自定义动作接口 | `ACTION_TYPE_KEY`, `REQUIRED_KEY`, `VALIDATION_TYPE_KEY`, `RULE_ID_KEY`, `FUNC_KEY` 等 9 个 |

### 8.1.1 标准审计字段（STD_AUDIT_KEYS, 9 个）

所有节点和关系表的通用审计字段，定义在 `unified_mapping.rs`：

| 常量 | 字段名 | 类型 | 约束 | 说明 |
|------|--------|------|------|------|
| `STD_ID_KEY` | `id` | TEXT | PRIMARY KEY, NOT NULL | UUID 技术主键 |
| `STD_NAME_KEY` | `name` | TEXT | — | 名称 |
| `STD_CODE_KEY` | `code` | TEXT | NOT NULL, 表内唯一 | 编码 |
| `CREATE_TIME_KEY` | `create_time` | DateTime | NOT NULL, 自动填充 | 记录创建时间 |
| `CREATE_USER_KEY` | `create_user` | VARCHAR(64) | NOT NULL | 记录创建人标识 |
| `UPDATE_TIME_KEY` | `update_time` | DateTime | NOT NULL, 每次更新自动刷新 | 记录最后更新时间 |
| `UPDATE_USER_KEY` | `update_user` | VARCHAR(64) | NOT NULL | 记录最后更新人标识 |
| `DELETE_FLAG_KEY` | `delete_flag` | INT | NOT NULL, DEFAULT 0 | 0=未删除, 1=已删除 |
| `IS_SYSTEM_KEY` | `is_system` | TEXT | — | "0"=自定义, "1"=系统预设不可修改 |

### 8.1.2 边属性 — 自定义动作接口（EDGE_ACTION_KEYS, 9 个）

| 常量 | 字段名 | 说明 |
|------|--------|------|
| `ACTION_TYPE_KEY` | `actionType` | 路由标识，如 `"inference"` 走推理机 |
| `REQUIRED_KEY` | `required` | 阻断控制，校验失败时是否中断 |
| `VALIDATION_TYPE_KEY` | `validationType` | `"Strong"` 强校验阻断 / `"Weak"` 弱校验提醒 |
| `RULE_ID_KEY` | `ruleId` | 规则锚点，指向图数据库中的规则本体节点 |
| `FUNC_KEY` | `func` | 执行指令，映射底层函数名 |
| `FIELD_ID_KEY` | `id` | 数据锚点，当前被校验的业务数据节点 |
| `MSG_KEY` | `msg` | 详细说明 |
| `SYNONYM_KEY` | `synonym` | 同义词 |
| `QUERY_VARIANT_KEY` | `queryVariant` | 错意词 |

### 8.1.3 关系创建审计注入

`create_relationship()` 自动注入审计默认值（`relationship.rs`），用户通过 `properties` 传入同名字段时覆盖：

| 字段 | 默认值 |
|------|--------|
| `create_time` | 当前时间 |
| `update_time` | 当前时间 |
| `delete_flag` | `0` |
| `is_system` | `"0"` |
| `create_user` | `""` |
| `update_user` | `""` |

**双向查找 API**（SHACL/LLM 用）：

```rust
use ontology_storage::mapper::unified_mapping;

let entity = owl_entity_for_label("Class");        // Some(OwlEntityType::Class)
let axiom  = owl_axiom_for_relation("INSTANCE_OF"); // Some(ClassAssertion)
let cat    = categorize_property_key("label");     // Some(Annotation)
```

> 使用 `/architecture-decision-records` skill 在决策发生时自动记录。

---

## 9. 变更管理

### 8.1 分支策略

- `master` — 始终可构建、可运行
- 功能分支命名：`feat/<描述>` / `fix/<描述>` / `refactor/<描述>`
- 合并前必须通过第 5.3 节提交前检查清单

### 8.2 Git 操作约定（AI 助手约束）

| 用户指令 | AI 行为 |
|----------|---------|
| 只说改代码，没提提交 | **只改代码**，不做任何 git 操作 |
| "提交" / "commit" | 执行 `git add -A && git commit -m '...'` |
| "push" / "推上去" | **先** `git commit`（含 add），**再** `git push` |
| "提交代码" | 同"提交"，只 commit 不 push |

> 核心：AI 不主动 commit/push。用户说 push 时，commit 和 push 一起做。

---

## 10. 缺陷追踪

- 已知问题在代码中使用 `// FIXME(<编号>): <描述>` 标注
- 临时方案使用 `// HACK(<编号>): <描述> — <为什么这样做>`
- 待办事项使用 `// TODO(<编号>): <描述>`

> CMMI5 过程域对齐：CAR（因果分析与解决）— 每个阻断性缺陷必须有根因分析记录。

---

## 11. 图数据库 ID 设计标准 — 雪花算法 (Snowflake ID)

> **核心原则**：图数据库中所有节点和边的唯一标识符统一使用雪花算法生成 **64-bit 纯数字**（`i64`）。
> 禁止使用数据库自增 ID、随机 UUID、字符串包装、hex 编码或无协调的自定义序列方案。

### 11.1 为什么是雪花算法？

| 对比维度 | 雪花算法 | UUID v4 | 数据库自增 | 纳秒时间戳 |
|----------|----------|---------|------------|------------|
| 全局唯一 | ✅ 是 | ✅ 是 | ❌ 单机唯一 | ❌ 并发冲突 |
| 趋势递增 | ✅ 时间序 | ❌ 随机（B+树分裂严重） | ✅ 递增 | ✅ 递增 |
| 分布式友好 | ✅ 无需协调 | ✅ 无需协调 | ❌ 需要全局锁 | ❌ 需要全局时钟 |
| 存储效率 | ✅ 64-bit (8B) | ❌ 128-bit (16B) | ⚠️ 取决于类型 | ⚠️ 64-bit |
| 可读性 | ⚠️ i64 数值 | ❌ 无意义 hex | ✅ 简单 | ❌ 大数值 |
| 图数据库适配 | ✅ 属性图天然适合 | ⚠️ 索引膨胀 | ❌ 分布式图不可用 | ❌ 时钟漂移风险 |

> 雪花算法由 Twitter 于 2012 年提出，是分布式系统 ID 生成的事实标准。

### 11.2 位布局 (64-bit)

```
┌─┬─────────────────────────────────────────────┬────────────────────┬──────────────────────┐
│0│              41-bit 时间戳 (ms)              │   10-bit 节点 ID    │   12-bit 序列号       │
└─┴─────────────────────────────────────────────┴────────────────────┴──────────────────────┘
 63  62                                      22  21                12  11                  0
```

| 字段 | 位数 | 范围 | 说明 |
|------|------|------|------|
| 保留位 | 1 bit | 固定为 0 | 保证 ID 为正整数（i64 兼容） |
| 时间戳 | 41 bits | 0 ~ 2^41-1 | 相对自定义 Epoch 的毫秒偏移，可用 ~69 年 |
| 节点 ID | 10 bits | 0 ~ 1023 | 机器/进程/worker 标识，支持 1024 个节点并发 |
| 序列号 | 12 bits | 0 ~ 4095 | 同毫秒内自增，单节点每秒最多 409.6 万 ID |

### 11.3 Rust 参考实现

```rust
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 自定义 Epoch：2026-01-01 00:00:00 UTC（毫秒）
pub const SNOWFLAKE_EPOCH_MS: i64 = 1_767_225_600_000;

const NODE_ID_BITS: i64 = 10;
const SEQUENCE_BITS: i64 = 12;
const MAX_NODE_ID: i64 = (1 << NODE_ID_BITS) - 1;     // 1023
const MAX_SEQUENCE: i64 = (1 << SEQUENCE_BITS) - 1;   // 4095
const TIMESTAMP_SHIFT: i64 = NODE_ID_BITS + SEQUENCE_BITS; // 22
const NODE_ID_SHIFT: i64 = SEQUENCE_BITS;              // 12

pub struct SnowflakeIdGenerator {
    node_id: i64,
    last_timestamp: AtomicI64,
    sequence: AtomicU64,   // u64 避免 AtomicI64 的负数回绕
}

impl SnowflakeIdGenerator {
    /// 创建生成器。`node_id` 必须在 0..=1023 范围内。
    pub fn new(node_id: i64) -> Result<Self, anyhow::Error> {
        anyhow::ensure!(
            (0..=MAX_NODE_ID).contains(&node_id),
            "node_id 超出范围 [0, {MAX_NODE_ID}]，当前值: {node_id}"
        );
        Ok(Self {
            node_id,
            last_timestamp: AtomicI64::new(0),
            sequence: AtomicU64::new(0),
        })
    }

    /// 生成下一个雪花 ID，返回 i64（图数据库属性兼容）。
    pub fn next_id(&self) -> i64 {
        loop {
            let now = current_epoch_ms();
            let last = self.last_timestamp.load(Ordering::Acquire);

            if now == last {
                // 同毫秒内：序列号自增
                let seq = self.sequence.fetch_add(1, Ordering::Relaxed) as i64;
                if seq <= MAX_SEQUENCE {
                    return self.compose(now, seq);
                }
                // 序列号耗尽：自旋等待下一毫秒
                while current_epoch_ms() <= last {
                    std::hint::spin_loop();
                }
                // 进入下一毫秒，重置序列号（由下一轮 last_timestamp 更新触发）
            } else {
                // 新毫秒：尝试 CAS 更新时间戳并重置序列号
                if self
                    .last_timestamp
                    .compare_exchange(last, now, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok()
                {
                    self.sequence.store(0, Ordering::Release);
                    return self.compose(now, 0);
                }
                // CAS 失败，重试
            }
        }
    }

    fn compose(&self, timestamp: i64, sequence: i64) -> i64 {
        (timestamp << TIMESTAMP_SHIFT)
            | (self.node_id << NODE_ID_SHIFT)
            | sequence
    }

    /// 从 ID 中反向解析各字段（调试/审计用）。
    pub fn decompose(id: i64) -> (i64, i64, i64) {
        let timestamp = (id >> TIMESTAMP_SHIFT) & ((1 << 41) - 1);
        let node_id = (id >> NODE_ID_SHIFT) & MAX_NODE_ID;
        let sequence = id & MAX_SEQUENCE;
        (timestamp, node_id, sequence)
    }
}

fn current_epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
        - SNOWFLAKE_EPOCH_MS
}
```

### 11.4 图中节点与边的 ID 约定

| 图元素 | ID 字段 | 类型 | 生成策略 |
|--------|---------|------|----------|
| 节点 (Node) | `snowflake_id` (属性) | **纯数字 `i64`** | 创建时由 `SnowflakeIdGenerator` 生成 |
| 边 (Relationship) | `snowflake_id` (属性) | **纯数字 `i64`** | 创建时由 `SnowflakeIdGenerator` 生成 |

> **重要**：`snowflake_id` 是图中节点和边的**主键标识**。已有的 `code`、`iri`、`id` 等业务标识字段继续保留作为业务查找键，
> 但它们不再是唯一标识符。所有内部遍历、索引、关系引用统一使用 `snowflake_id`。

### 11.5 工程约束

1. **单例生成器**：每个进程只创建一个 `SnowflakeIdGenerator` 实例（通过 `lazy_static!` 或 `OnceCell`），保证同进程内 ID 唯一
2. **node_id 分配**：多实例部署时，每个实例通过环境变量 `SNOWFLAKE_NODE_ID` 配置不同的 `node_id`（0-1023），避免跨实例碰撞
3. **索引**：Memgraph/Neo4j 中 `snowflake_id` 属性必须建立索引（`CREATE INDEX ON :Entity(snowflake_id)` 等所有标签）
4. **类型强制**：`snowflake_id` 在内存、数据库、序列化全链路统一为 **纯数字 `i64`**（64-bit signed integer）。禁止在任何环节转为 `String`、hex、base64 或带前缀的字符串（如 `"sf_748291..."`）。API 响应中直接输出 JSON number：`{"snowflake_id": 748291234567890123}`，不得输出 `{"snowflake_id": "748291234567890123"}`
5. **禁用场景**：`snowflake_id` 不作为业务排序依据（时间序只是趋势，不严格保证）；需要精确时序的场景使用独立的 `created_at` 时间戳字段

### 11.6 与现有 `code`/`iri` 的共存

```
现有模型                              新增后模型
┌──────────────────────┐      ┌──────────────────────────────┐
│ Entity 节点           │      │ Entity 节点                   │
│  • code: "P8A_001"   │  →   │  • snowflake_id: 748291...   │ ← 主键（雪花 ID）
│  • iri: "#P8A_001"   │      │  • code: "P8A_001"           │ ← 业务键（保留）
│  • label: "歼-20"    │      │  • iri: "#P8A_001"           │ ← 语义标识（保留）
└──────────────────────┘      │  • label: "歼-20"            │ ← 展示名（保留）
                              └──────────────────────────────┘

关系查询迁移：
  OLD: MATCH (n {code: "P8A_001"})-[r]->(m {code: "YJ-12"})
  NEW: MATCH (n {snowflake_id: 748291...})-[r]->(m {snowflake_id: 932145...})
  
  // code 仍用于面向用户的查找：
  MATCH (n {code: "P8A_001"})  // 返回 snowflake_id + 所有属性
```

### 11.7 跨实例部署拓扑

```
                    ┌──────────────────┐
                    │   Load Balancer   │
                    └──┬──────────┬──┬──┘
                       │          │  │
          ┌────────────▼──┐  ┌───▼──────────┐
          │ App Instance  │  │ App Instance  │  ...
          │ node_id = 0   │  │ node_id = 1   │
          │ generator ────│  │ generator ────│
          │ snowflake IDs │  │ snowflake IDs │
          └──────┬────────┘  └──┬────────────┘
                 │              │
          ┌──────▼──────────────▼──────┐
          │       Memgraph 集群         │
          │  (snowflake_id 全局唯一)    │
          └────────────────────────────┘

配置方式：
  .env:
    SNOWFLAKE_NODE_ID=0    # 每个实例不同，0-1023

  Docker:
    docker run -e SNOWFLAKE_NODE_ID=0 ...
```

---

## 12. 附录：参考文档

- [Rust 官方编码规范](https://doc.rust-lang.org/1.0.0/style/)
- [Rust API 设计指南](https://rust-lang.github.io/api-guidelines/)
- [CMMI 5 级过程域概览](https://cmmiinstitute.com/cmmi/dev)
- [Twitter Snowflake ID 原始公告](https://blog.twitter.com/engineering/en_us/a/2010/announcing-snowflake)

---

## 13. 推理边/推理属性前缀规范

> **核心原则**：
> 1. 图中关系类型以 6 种前缀开头 → 推理边，走推理引擎处理
> 2. 不带前缀的关系 → 非推理边，仅展示/结构遍历
> 3. 边属性（9 个标准字段）控制路由、校验和阻断行为

### 13.1 对象属性 — 关系类型前缀（6 种）

| 序号 | 作用域 | 编码前缀 | 名称 | 格式示例 | 备注 |
|------|--------|----------|------|----------|------|
| 1 | 对象属性 | `rdfs:` | RDFS 语言 | `rdfs:subClassOf domain` | 也支持 RDFS 核心常量，不写前缀 |
| 2 | 对象属性 | `owl2:` | OWL2 DL 语言 | `owl2:ObjectIntersectionOf(:Person :Employee)` | OWL2 DL 为主 |
| 3 | 对象属性 | `swrl:` | SWRL 语言 | `swrl:Person(?x) ^ hasAge(?x, ?a) -> Adult(?x)` | SWRL 语法 |
| 4 | 对象属性 | `sh:` | SHACL 语言 | `sh:property [ sh:path :name; sh:minCount 1 ]` | |
| 5 | 对象属性 | `rule:` | 规则设定 | `rule:forwardChain` / `rule:backwardChain` | 默认前链推理 |
| 6 | 对象属性 | `func:` | 自定义动态函数 | `func:{"id":"图ID","func":"函数名"}` | 不对接大模型，用 JSON 调用函数实现 |

> 1-4 是 W3C 标准语义网语言，5-6 是系统自定义的扩展前缀。
> 此约定与 `crates/ontology-reasoner/src/language.rs` 中的 `LanguagePrefix` 枚举一致。

### 13.1.1 边属性 — 自定义动作接口（9 个标准字段）

任何图关系（边）上可附加以下 9 个属性键，控制路由、校验和阻断行为：

| 字段 | 类型 | 说明 |
|------|------|------|
| `actionType` | String | **路由标识**：指定执行分支（如 `"inference"` 表示走推理机逻辑） |
| `required` | String/Bool | **阻断控制**：校验失败时是否强制中断当前业务流程 |
| `validationType` | String | **规则级别**：`"Strong"` 强校验（阻断）/ `"Weak"` 弱校验（提醒不阻断） |
| `ruleId` | String | **规则锚点**：指向图数据库中的规则本体节点，用于元数据管理和错误溯源 |
| `func` | String | **执行指令**：映射底层要调用的具体函数名 |
| `id` | String | **数据锚点**：当前需要被校验的具体业务数据节点 |
| `msg` | String | 详细说明作用 |
| `synonym` | String | 同义词 |
| `queryVariant` | String | 错意词 |

> 边属性均为标量类型（String 为主），遵循 Memgraph 约束（§6.2）。
> 复合语义通过扁平化 key-value 表达，不嵌套 JSON。

### 13.2 关系类型（边标签）前缀规则

```
图中关系（边）:
  swrl:hasEnemy      ──→ 推理边，SWRL 引擎处理
  owl2:someValuesFrom  ──→ 推理边，DWL2 引擎处理
  sh:property         ──→ 推理边，SHACL 引擎处理
  rule:forwardChain   ──→ 推理边，推理方向控制
  func:calc            ──→ 推理边，LLM JSON 调用
  移动                ──→ 非推理边，仅展示/结构遍历，不触发推理
  打击                ──→ 非推理边，同上
```

规则：
- **带前缀** → 推理引擎捡起处理，按前缀路由到对应引擎
- **不带前缀** → 推理引擎跳过，不处理
- **只有前缀没有内容**（如关系类型就是 `swrl:`）→ 仍是有效的推理边，继续推理（body 为空时不执行具体规则，但边本身参与遍历/克隆/发现）

### 13.3 属性前缀规则

图中属性键和属性值同样适用 6 种前缀约定。属性键或值以上表任一前缀开头时，视为推理相关属性。

```
节点属性:
  hasEffect: "swrl:Person(?x) ^ hasAge(?x, ?a) -> Adult(?x)"
             └── SWRL 语言表达式，路由到 SWRL 引擎

  rule:forwardChain    └── 推理方向设定
  func:{"id":"N1","func":"f"}  └── JSON 函数调用
```

规则同关系类型：带前缀处理，不带前缀跳过。

### 13.4 空 body 规则

前缀后内容为空时**不报错**，仍为有效推理边/属性：

| 输入 | 处理 |
|------|------|
| `swrl:` | 有效推理边 → SWRL 引擎，body 为空，无规则执行，但参与遍历 |
| `owl2:` | 有效推理边 → DWL2 引擎，body 为空 |
| `sh:` | 有效推理边 → SHACL 引擎，body 为空 |
| `rule:` | 有效推理边 → 默认前向链推理 |
| `func:` | 有效推理边 → LLM JSON 调用 |

> 实现：`parse_language_expression("swrl:")` 返回 `Ok(ParsedExpression { prefix: Swrl, body: "" })`，
> 各引擎收到空 body 时跳过执行返回 `Ok(0)`，不中断链路。这是宽容执行原则（§19）的体现。

### 13.5 工具函数

`language.rs` 提供以下公共函数：

```rust
/// 检查字符串是否以任一推理前缀开头（6 种）
pub fn is_inference_prefix(s: &str) -> bool;

/// 检查关系类型是否为推理边（语义别名）
pub fn is_inference_relation(rel_type: &str) -> bool;

/// 提取推理前缀类型（不要求 body 非空）
pub fn classify_inference_prefix(s: &str) -> Option<LanguagePrefix>;

/// 按前缀分类字符串（批量处理边/属性）
pub fn classify_strings_by_prefix(strings: &[String]) -> HashMap<LanguagePrefix, Vec<String>>;

/// 判断关系是否属于本体语义层 (owl2: 前缀 + ONTOLOGY_SEMANTIC_RELS)
pub fn is_ontology_relation(rel_type: &str) -> bool;
```

`graph/util.rs` 提供属性继承核心函数：

```rust
/// 获取节点的全部关系 (出向 + 入向)
pub fn get_all_relationships(repo, node_id) -> Vec<Relationship>;

/// 沿类型链向上收集父类型全部属性
pub fn collect_parent_properties(repo, entity) -> HashMap<String, PropertyValue>;

/// 属性继承展开: 父底 + 子覆盖, 返回合并后的新 Node (不写入图)
pub fn inherit_entity_properties(repo, entity) -> Node;

/// 克隆到场景版本, 使用给定的合并后属性
pub fn ensure_cope_version_with_props(repo, original_code, version, labels, merged_props) -> Result<String>;
```

`unified_mapping.rs` 提供本体语义层常量：

```rust
/// 本体语义层关系常量合集 (subClassOf, INSTANCE_OF, ... 共 15 个)
pub const ONTOLOGY_SEMANTIC_RELS: &[&str];
```

### 13.6 与 unified_mapping 核心 OWL 常量的关系

`unified_mapping.rs` 中定义的核心 OWL 关系常量（如 `subClassOf`、`INSTANCE_OF`、`HAS_PROPERTY` 等）是**隐式推理边**：
- 它们是 OWL2 语义的基础关系，由 `OwlAxiomPredicate` 枚举和 `owl_axiom_for_relation()` 双向查找表直接识别
- 不需要也不使用前缀（保持与 W3C OWL2 规范一致）
- DWL2/SWRL/SHACL 引擎通过 `unified_mapping` 常量直接引用它们

**领域自定义关系**（如 `hasEnemy`、`移动`、`打击`）：
- 如果要被推理引擎处理 → 必须带 6 种前缀之一（如 `swrl:hasEnemy`、`rule:forwardChain`）
- 如果不带前缀 → 仅作为展示/结构边，不触发推理

```
OWL 语义层（隐式推理边，无需前缀）:
  subClassOf, INSTANCE_OF, HAS_PROPERTY, equivalentClass, disjointWith, ...

领域扩展层（必须带前缀才触发推理）:
  swrl:hasEnemy, owl2:移动, sh:validate, rule:forwardChain, func:calc
  移动, 打击, 侦察, ...  ← 不带前缀，仅展示用
```

### 13.7 与 BehaviorAction 的关系

Entity 的 `hasEffect` 字段已使用 6 种语言前缀路由（参见 §6.4），路由规则如下：

| 前缀 | 路由 |
|------|------|
| `swrl:` | SWRL 规则解析 + 执行 |
| `sh:` | SHACL 验证（仅检查合规性） |
| `owl2:` | DWL2 查询（仅检查存在性） |
| `rule:` | 推理方向设定（记录日志） |
| `func:` | LLM JSON 调用（记录日志） |
| 无前缀 | 默认尝试 SWRL 解析 |

`hasPrecondition` 字段使用 SHACL 语法但不要求 `sh:` 前缀（因为前置条件语义已由字段名 `hasPrecondition` 确定）。

### 13.8 代码约束

| 规则 | 说明 |
|------|------|
| **INFER-001** | BFS 遍历未指定关系类型时，默认只跟随推理边（`is_inference_relation()` 过滤） |
| **INFER-002** | `clone_nodes_selective()` 只跟随推理边进行本体对象发现 |
| **INFER-003** | `predict_next_steps()` 默认只汇总推理边 |
| **INFER-004** | 不带前缀的领域关系不被推理引擎处理，不参与克隆/发现/预测 |
| **INFER-005** | 新创建的领域关系若要触发推理，必须用 6 种推理前缀之一起名 |
| **INFER-006** | `parse_language_expression()` 接受空 body，不报错 |
| **INFER-007** | 各引擎收到空 body 时跳过执行，返回 Ok(0)，不中断链路 |
| **INFER-008** | 本体语义层关系（`ONTOLOGY_SEMANTIC_RELS` + `owl2:` 前缀）优先于其他推理层处理 |
| **INFER-009** | `is_ontology_relation()` 判断关系是否属于本体语义层 |
| **INFER-010** | 推理流水线：先继承后克隆，先本体后关系，先推理后校验；一层一层推理，一层一层复制 |

### 13.9 两层处理体系

推理引擎将所有关系分为两层，**本体语义层始终优先处理**：

```
┌──────────────────────────────────────────────────────────┐
│ 第 1 层 — 本体语义层 (最先处理)                            │
│   1. rdfs: ─ RDFS 基础类型系统 (domain、range 等)         │
│   2. owl2: ─ OWL2 DL 本体语义 (主力)                      │
│         + ONTOLOGY_SEMANTIC_RELS (15 个核心常量):          │
│           subClassOf、INSTANCE_OF、HAS_PROPERTY、          │
│           equivalentClass、disjointWith、sameAs、          │
│           differentFrom、inverseOf、complementOf、         │
│           intersectionOf、unionOf、oneOf、                 │
│           subPropertyOf、HAS_RANGE、HAS_VALUE              │
│   用途: 属性继承、类层次解析、实例归属判断                  │
├──────────────────────────────────────────────────────────┤
│ 第 2 层 — 推理执行层 (本体就绪后)                          │
│   3. swrl:     ─ 推理规则表达                             │
│   4. sh:       ─ 数据校验约束                             │
│   5. rule:     ─ 前链/后链推理方向                         │
│   6. func:     ─ LLM JSON 调用                            │
│   用途: 规则执行、约束验证、LLM 集成                       │
└──────────────────────────────────────────────────────────┘
```

**判断函数**：

```rust
/// 本体语义层: rdfs:/owl2: 前缀 + ONTOLOGY_SEMANTIC_RELS 核心常量
pub fn is_ontology_relation(rel_type: &str) -> bool;

/// 推理层: 全部 6 种前缀 (rdfs:/owl2:/swrl:/sh:/rule:/func:)
pub fn is_inference_relation(rel_type: &str) -> bool;
```

### 13.10 推理流水线 (reason_on_nodes)

```
输入: node_names + expressions + cope_version

Step 1 ── 按名称查找原始实体
Step 2 ── 解析表达式前缀 + 预加载 SWRL 规则
Step 3 ── 逐层 BFS 处理:
  对本层每个实体:
    a. 获取全部关系 (出向 + 入向)
    b. 属性继承展开 (OWL2/RDFS 层优先, 父属性 ∪ 子属性, 子覆盖父)
    c. 复制到场景版本 (含继承后属性)
    d. 对该实体独立推理 (行为引擎 + SWRL + DWL2)
    e. 沿推理边发现下游本体 → 加入下一层
Step 4 ── 全部本体就绪后, 复制副本间的关系
Step 5 ── SHACL 校验 (推理后验证合规性)
```

**原则**：
- **一层一层推理，一层一层复制** — 一层有几个本体就复制几个本体，关联几个本体就复制几个本体
- **每个本体独立推理** — 不是所有本体一起推，而是每个本体独立走完 inherit → copy → reason
- **先继承后克隆** — OWL2/RDFS 关系先解析继承，属性合并完成后再克隆副本
- **先推理后校验** — SWRL/DWL2 推理全部完成后，SHACL 最后验证合规性
