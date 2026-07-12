# ontologica_rust_graph — 项目开发规范 v2.1

> 最后更新: 2026-07-13 | 图原生推理引擎

---

## 1. 架构红线

### 1.1 依赖方向

```
server → reasoner → storage
  │         │          └── 永不依赖上层
  │         └── 只依赖 SharedRepository trait + QueryPlan 抽象
  └── 只调 Engine 公开 API
```

| 层 | 允许 | 禁止 |
|----|------|------|
| `ontology-server` | `ontology-reasoner` 公开 API | 直接引用 storage 类型 |
| `ontology-reasoner` | `SharedRepository` trait + `QueryPlan` | 直接构造 Cypher / GraphPattern |
| `ontology-storage` | neo4rs + 标准库 | 反向依赖上层 |

### 1.2 图原生推理（v2.1 核心）

- **没有 RDFS 文件** — 所有数据在 Memgraph 中
- **全部用原生 ID（id(n)）** — 不用属性 `id` 做锚点
- **推理机就是在图上走** — 沿边逐跳、读属性、写副本
- **RDFS/OWL/SWRL 语法是 DSL 输入** — 翻译成 Cypher 在图里执行
- **不依赖全文检索** — 全靠图索引（关系/主键/标签）

### 1.3 HTTP 接口（3 个）

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/health` | 健康检查 |
| GET | `/tools` | LLM 工具定义 |
| POST | `/infer-on-nodes-id-fc` | GIE 推理机（流式 NDJSON） |

---

## 2. 编码规范

### 2.1 宽容执行

- 字段缺失 → 跳过 + 默认值兜底，不 panic
- 空结果 → `vec![]` / `None`，不是错误
- JSON 反序列化 → `#[serde(default)]` / `Option<T>`
- 规则解析失败 → 跳过该条，继续执行

### 2.2 严禁裸 `unwrap()`

```rust
let speed = node.property("speed").unwrap_or(0.0);     // ✅
let name = node.property("name").unwrap_or("未命名");   // ✅
```

### 2.3 日志

| 级别 | 场景 |
|------|------|
| `debug!` | 字段缺失用默认值、规则未匹配 |
| `warn!` | 单条规则解析失败跳过、连接降级 |
| `error!` | 系统级故障 |

---

## 3. 数据规范

### 3.1 原生 ID

`id(n)`（Memgraph 内部 ID）是唯一技术锚点。`code`/`name` 仅展示搜索。

### 3.2 副本版本

- 原实体 `cope_version = ""`（不修改）
- 推理写入在副本上（`cope_version = UUID`）
- 副本 `graph_id` 存原节点原生 ID

---

## 4. 推理机三步流水线

```
输入: { node_ids: [原生ID], confidence: 0.8, cope_version: UUID }

Step 1 — 克隆 + RDFS链
┌─────────────────────────────────────────┐
│ 复制种子节点                             │
│ 沿 subClassOf/INSTANCE_OF 链递归爬顶层  │
│ 克隆所有祖先 + 拷贝属性                  │
│ subClassOf 语义: 顶层属性为基底 → 逐层覆盖│
└─────────────────────────────────────────┘

Step 2 — 下游递归克隆
┌─────────────────────────────────────────┐
│ 沿 actionType="inference" 边递归克隆    │
│ 所有下游节点 + 复制关系                  │
│ 不做推理，纯复制                         │
└─────────────────────────────────────────┘

Step 3 — 推理叙述
┌─────────────────────────────────────────┐
│ 逐节点: hasPrecondition → hasEffect     │
│ → hasCost → hasDuration → hasPriority  │
│ → composedOf → 置信度传播               │
│ → 输出叙述 → 跳下一节点                  │
└─────────────────────────────────────────┘
```

---

## 5. 关系处理规则

| 关系类型 | 克隆 | 入推理队列 | 说明 |
|---------|:---:|:---:|------|
| RDFS 边 (subClassOf/INSTANCE_OF) | ✅ | ❌ | 属性继承用 |
| 推理边 (actionType=inference) | ✅ | ✅ | 逐节点推理 |
| 领域边 (无前缀) | ❌ | ❌ | 跳过 |

---

## 6. 代码修改纪律（AI 必须遵守）

### 6.1 改前三件事

1. **追踪调用链**：从入口函数（HTTP handler）开始，追踪完整调用链到底层，不假设任何中间状态
2. **全局搜索引用**：`grep` 搜索所有引用点，确保没有遗漏的调用方
3. **理解现有约定**：读相关规范文档（CLAUDE.md、ARCHITECTURE.md），确认不违反架构红线

### 6.2 改后三件事

1. **回读检查**：从头读一遍修改后的完整文件，检查变量名、类型、引用是否一致
2. **编译验证**：`cargo check --workspace` 零告警
3. **重启服务并验证**：`curl` 测试端点，确认返回结果正确

### 6.3 提交规范

```bash
# 提交前必做
cargo fmt --all
cargo clippy --workspace -- -D warnings
cargo check --workspace
cargo test --workspace
```

提交格式: `feat(gie): xxx` / `fix(storage): xxx`

---

## 附录

### 工具链

`stable-x86_64-pc-windows-gnu` (GNU ABI)，锁定在 `rust-toolchain.toml`。

### Memgraph

```env
ONTOLOGY_GRAPH_URI=bolt://192.168.3.8:7687
ONTOLOGY_PORT=8085
ONTOLOGY_MODE=Balanced
```

### 语言前缀 (7 种)

`rdfs:` | `owl2:` | `swrl:` | `sh:` | `rule:` | `func:` | `$.`
