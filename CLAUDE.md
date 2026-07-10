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

   默认策略: Exercise, 阈值: 0.30

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
ONTOLOGY_MODE=Exercise
RUST_LOG=info
```

### 6.2 协议说明

Memgraph 兼容 Neo4j Bolt 协议，本项目通过 `ontology-storage` 的 `memgraph` feature
使用 `neo4rs` 驱动，走原生 Bolt 协议连接。

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
| POST | `/infer-forward` | 前向推理 |
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
| 预组合切片 | 循环遍历 | `DOMAIN_LABELS`, `OWL_NODE_LABELS` |

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

---

## 10. 缺陷追踪

- 已知问题在代码中使用 `// FIXME(<编号>): <描述>` 标注
- 临时方案使用 `// HACK(<编号>): <描述> — <为什么这样做>`
- 待办事项使用 `// TODO(<编号>): <描述>`

> CMMI5 过程域对齐：CAR（因果分析与解决）— 每个阻断性缺陷必须有根因分析记录。

---

## 11. 附录：参考文档

- [Rust 官方编码规范](https://doc.rust-lang.org/1.0.0/style/)
- [Rust API 设计指南](https://rust-lang.github.io/api-guidelines/)
- [CMMI 5 级过程域概览](https://cmmiinstitute.com/cmmi/dev)
