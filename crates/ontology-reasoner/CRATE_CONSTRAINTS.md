# ontology-reasoner — 约束说明文档

> **版本**: 1.0 | **最后更新**: 2026-07-10 | **适用范围**: `crates/ontology-reasoner/` 全部模块

---

## 1. Crate 定位

`ontology-reasoner` 是本项目的**唯一业务逻辑归属**。所有推理、置信度计算、DL 查询、
规则解析、空间计算、SHACL 验证的逻辑必须在此 crate 内实现。

判断标准：如果代码里出现了 DWL2/SWRL/Confidence/Timeline/Spatial/SHACL 任意一个词，
它就不该在 `ontology-storage` 或 `ontology-server` 里。

---

## 2. 语言前缀约定

本 crate 的规则、查询、约束使用统一前缀区分不同语言和推理指令，前缀后紧跟语言表达式：

| 序号 | 前缀 | 名称 | 说明 |
|------|------|------|------|
| 1 | `owl2:` | OWL2-DL 语言 | 描述逻辑（Description Logic），用于 DWL2 查询模块的类表达式 |
| 2 | `swrl:` | SWRL 语言 | 语义 Web 规则语言，用于规则推理和行为引擎 |
| 3 | `sh:` | SHACL 语言 | 形状约束语言，用于图节点验证 |
| 4 | `rule:` | 规则设定 | 推理链方向控制（`forwardChain` / `backward`，默认前向） |
| 5 | `action:` | 自定义动作接口 | 对接大模型模糊推理，无内容时默认 `action:` |
| 6 | `func:` | 自定义函数 | JSON 格式 `{"id":"图ID","func":"函数名"}`，对接大模型 |

**示例**：
- `owl2:ObjectIntersectionOf(:Person :Employee)` — DWL2 类表达式
- `swrl:Person(?p) ^ hasAge(?p, ?age) ^ swrlb:greaterThan(?age, 18) -> Adult(?p)` — SWRL 规则
- `sh:property [ sh:path :name; sh:minCount 1 ]` — SHACL 形状约束
- `rule:forwardChain` — 前向链推理
- `action:validate_entity` — 自定义动作，LLM 模糊推理
- `func:{"id":"N1","func":"calc"}` — 自定义函数，JSON 格式

---

## 3. 模块依赖约束

### 3.1 依赖方向图

```
                    ┌──────────────┐
                    │  reasoner.rs │  ← 编排层（唯一有权协调所有模块）
                    └──────┬───────┘
           ┌───────────────┼───────────────────────────┐
           │               │                           │
    ┌──────▼──────┐ ┌──────▼──────┐ ┌──────────────────▼───┐
    │ dwl2/       │ │ swrl/       │ │ graph/                │
    │ (DL查询)    │ │ (规则推理)   │ │ (通用图遍历框架)       │
    │ 只读        │ │ 修改图       │ │ 只读                  │
    └──────┬──────┘ └──┬───┬──────┘ └──────────┬───────────┘
           │           │   │                    │
           │    ┌──────┘   │                    │
           │    │          │                    │
    ┌──────▼────▼──┐ ┌─────▼──────┐  ┌─────────▼──────────┐
    │ confidence/  │ │ spatial/   │  │ timeline/           │
    │ (置信度熔断)  │ │ (Haversine)│  │ (时序推演)           │
    │ 纯计算        │ │ 纯数学     │  │ 纯计算               │
    └──────────────┘ └────────────┘  └─────────────────────┘
                               │
    ┌──────────────────────────┼───────────────────────────┐
    │               ┌──────────▼──────────┐                │
    │               │ shacl/               │                │
    │               │ (图约束验证)          │                │
    │               │ 只读                  │                │
    │               └──────────────────────┘                │
    │                                                       │
    │  ┌──────────────┐                                     │
    │  │ language/     │  ← 语言前缀解析（6 种前缀: owl2:/swrl:/sh:/rule:/action:/func:） │
    │  │ 零依赖        │     reasoner.rs 路由表达式的依据     │
    │  └──────────────┘                                     │
    └──────────────────────────────────────────────────────┘
```

### 3.2 硬性规则

| 规则 | 说明 |
|------|------|
| **REASONER-001** | `graph/` 不依赖 reasoner 任何其他模块（`dwl2`、`swrl`、`confidence`、`timeline`）。它是纯框架层 |
| **REASONER-002** | `dwl2/` 和 `swrl/` 之间可通过 `swrl::ast::Atom::Query` 间接调用（`SwrlEngine` 持有 `Dwl2QueryEngine`），但 `dwl2/` 不能反向引用 `swrl/` |
| **REASONER-003** | `spatial/` 是纯数学库，零外部依赖，不可引入 `Node` / `GraphRepository` |
| **REASONER-004** | `confidence/` 不持有图仓库引用，仅接收 `ConfidenceInput` 做数值计算 |
| **REASONER-005** | `shacl/` 只读 `GraphRepository`，不写入，不调用推理引擎 |
| **REASONER-006** | `reasoner.rs` 是唯一有权同时持有 `Dwl2QueryEngine` + `SwrlEngine` + `ConfidencePolicy` 的模块 |
| **REASONER-007** | 所有模块禁止直接构造 `GraphPattern`/`Node`/`Relationship`（应通过 `unified_mapping` 常量 + `GraphRepository` trait 方法操作） |
| **REASONER-008** | 所有模块的图操作必须使用 `ontology_storage::mapper::unified_mapping` 中定义的词汇表常量，**禁止硬编码字符串**（如 `"Entity"`、`"INSTANCE_OF"`） |
| **REASONER-009** | SWRL / DWL2 / SHACL 引擎须支持 `cope_version` 过滤（通过 `with_cope_version()` 构建器）。设置为 `Some` 时只操作匹配版本的副本节点，`None` 时不过滤（向后兼容全局图） |
| **REASONER-010** | 语言前缀路由在 `reasoner.rs` 中完成：`owl2:` → DWL2、`swrl:` → SWRL、`sh:` → SHACL、`rule:` → 推理方向、`action:` → LLM 模糊推理、`func:` → LLM JSON 调用。前缀解析由 `language.rs` 模块提供，引擎本身不感知前缀 |

---

## 4. 产品层 / 业务层隔离 (graph 模块)

### 4.1 分层

| 层 | 位置 | 职责 |
|----|------|------|
| **产品层（框架）** | `ontology-reasoner::graph` | `GraphExplorer` + `StateChangeDetector` trait + util 函数 |
| **业务层** | `ontology-server::routes::infer` | `MilitaryStateChangeDetector` 实现，注入 ASW 领域知识 |

### 4.2 硬性规则

| 规则 | 说明 |
|------|------|
| **GRAPH-001** | `graph/` 不依赖 `serde_json`、`tiny_http`、`axum`，不含任何 HTTP/序列化概念 |
| **GRAPH-002** | `graph/` 不含领域概念 — 不出现 `Space_abs`、`haversine`、`声纳`、`反潜` 等 |
| **GRAPH-003** | `StateChangeDetector` trait 只在 `graph/detector.rs` 定义。业务方在 `server` 层实现 |
| **GRAPH-004** | 换业务场景（如从 ASW 换到金融风控）只需重新实现 `StateChangeDetector`，`GraphExplorer` 代码零改动 |
| **GRAPH-005** | `clone_nodes_selective` 用于选择性克隆：只克隆指定节点 + 通过关系发现的本体对象（而非 `clone_all_for_version` 的全量克隆）。克隆后的副本之间复制关系，与不同版本的节点保持隔离 |

### 4.3 允许/禁止

| 允许 | 禁止 |
|------|------|
| `ontology-storage` (`Node`/`Relationship`/`GraphRepository`) | 业务领域概念 |
| `std::fs`（读取 `rules/*.swrl` 文件） | `serde_json` / `tiny_http` |
| `std::collections` | 直接调用 SwrlEngine / Reasoner / TimelineEngine |

---

## 5. DWL2 DL 查询模块

### 5.1 定位

**只读**。将 `ClassExpression` AST 编译为图查询，检索个体集合。

### 5.2 硬性规则

| 规则 | 说明 |
|------|------|
| **DWL2-001** | 只读 `GraphRepository`，**绝不写入** |
| **DWL2-002** | 不直接调用 `SwrlEngine`（调用方向由 `SwrlEngine` → `Dwl2QueryEngine`） |
| **DWL2-003** | `ClassExpression::from_key()` 解析失败必须返回可读字符串错误，不可 panic |
| **DWL2-004** | 12 种构造子的语义必须符合 W3C OWL2 Direct Semantics |
| **DWL2-005** | DWL2 推理机用户只使用 DWL2-DL 语法，内部图翻译由 `unified_mapping` 封装 |
| **DWL2-006** | `Dwl2QueryEngine` 支持 `with_cope_version()` 过滤。设置后在 `get_all_individuals`、`get_instances_of_class`、`some_values_from` 中自动过滤版本不匹配的节点 |

### 5.3 允许/禁止

| 允许 | 禁止 |
|------|------|
| `GraphRepository::query_pattern()` / `get_nodes_by_label()` / `get_relationships()` | `insert_node` / `insert_relationship` / `delete_node` |
| `unified_mapping` 常量 | 硬编码 `"Individual"`、`"Class"`、`"INSTANCE_OF"` 等 |
| `GraphPattern` / `NodePattern` / `RelationshipPattern` 构造（注意 ADR-007 之后通过常量） | 直接写 Cypher 字符串 |

---

## 6. SWRL 规则推理模块

### 6.1 定位

**修改图**。Fixpoint 循环执行规则，匹配前提 → 绑定变量 → 推导新事实 → 写入图。

### 6.2 执行约束

| 规则 | 说明 |
|------|------|
| **SWRL-001** | 变量安全性：consequent 变量**必须全部**在 antecedent 中出现。`Rule::is_safe()` 在解析阶段校验 |
| **SWRL-002** | 规则匹配阶段（`execute_rule`）**只读** — 仅读 repo 产生绑定。写入只在 `insert_derived_facts` 阶段串行执行 |
| **SWRL-003** | 置信度熔断不终止整个 fixpoint — 只跳过当前规则链路，返回 `ConfidenceFuse` 错误 |
| **SWRL-004** | `derived_facts: HashSet<String>` 用于去重。`fact_key()` 生成唯一标识，重复事实跳过 |
| **SWRL-005** | 最大迭代次数默认 100，超限不 panic — 记录 warning 日志后返回 |
| **SWRL-006** | 并发推理（`thread::scope`）时每条规则的 `execute_rule` 不共享可变状态，唯一可变状态在 `insert_derived_facts` 中串行访问 |
| **SWRL-007** | `DwL2QueryEngine` 仅在 `Query` atom 匹配时调用，不是每条规则都调用 |
| **SWRL-008** | `SwrlEngine` 支持 `with_cope_version()` 过滤。设置后在 `match_class_atom`/`match_property_atom` 中只匹配版本一致的节点，`insert_derived_facts` 自动为新事实标记 `cope_version` |

### 6.3 内置函数约束 (builtins)

| 规则 | 说明 |
|------|------|
| **BUILTIN-001** | 未绑定参数返回 `Deferred`，不报错（fixpoint 延迟求值） |
| **BUILTIN-002** | 所有内置函数幂等 — `swrlb:greaterThan` / `swrlb:add` 不修改外部状态 |
| **BUILTIN-003** | 数值运算始终 `f64` 精度 |

### 6.4 行为引擎约束 (behavior)

| 规则 | 说明 |
|------|------|
| **BEH-001** | 6 个字段全空的 Entity 跳过，不参与计算 |
| **BEH-002** | `composedOf` 递归深度通过 `max_depth` 控制，默认上限 5 |
| **BEH-003** | `priority` 值范围 0-10，超出在解析阶段截断 |

---

## 7. 置信度模块

### 7.1 定位

**纯计算层**。推理的质量守护条件，不持有图引用。

### 7.2 硬性规则

| 规则 | 说明 |
|------|------|
| **CONF-001** | `ConfidenceCalculator` 不持有 `GraphRepository` 引用。仅通过 `ConfidenceInput` 计算 |
| **CONF-002** | 熔断阈值默认 0.30（Balanced 模式），Permissive 模式降至 0.15，Strict 模式升至 0.50 |
| **CONF-003** | `ConfidenceFuse` 是状态机：`Active` → `Tripped`。`reset()` 回到 `Active` |
| **CONF-004** | Policy 注入路径：`ReasonerConfig.policy` 或 `POST /confidence/policy` |

### 7.3 允许/禁止

| 允许 | 禁止 |
|------|------|
| `f64` 数值计算 + HashMap 覆盖 | 持有/引用 GraphRepository |
| 通过 `ReasonerConfig` / HTTP 端点注入策略 | 在 `swrl/` 或 `dwl2/` 中直接修改置信度 |

---

## 8. 空间计算模块

### 8.1 定位

独立的几何数学函数库。**零依赖**。

### 8.2 硬性规则

| 规则 | 说明 |
|------|------|
| **SPATIAL-001** | 纯函数，无副作用。输入 `f64`，输出 `f64`，单位：米 |
| **SPATIAL-002** | 不可依赖 `Node` / `GraphRepository` / `PropertyValue` |
| **SPATIAL-003** | 不可依赖 `ontology-storage` |
| **SPATIAL-004** | Haversine 公式常数 `EARTH_RADIUS_M = 6_371_000.0` |

---

## 9. 时序推演模块

### 9.1 定位

**纯计算层**。接收实体坐标+速度+航点，输出 TimelineResult。

### 9.2 硬性规则

| 规则 | 说明 |
|------|------|
| **TL-001** | 不持有 `GraphRepository` 引用。输入通过 `TimelineInput` 传入 |
| **TL-002** | 距离计算委托 `spatial::haversine_m` |
| **TL-003** | 日志同时写入文件 `logs/patrol_{code}_{ts}.log` 和返回 `log_lines` |
| **TL-004** | 不自行查询图 — 调用方负责传入坐标和速度 |

---

## 10. SHACL 验证模块

### 10.1 定位

**只读验证层**。对图节点执行 W3C SHACL 形状约束检查，不修改图。

### 10.2 硬性规则

| 规则 | 说明 |
|------|------|
| **SHACL-001** | 只读 `GraphRepository`，**绝不写入** |
| **SHACL-002** | 目标解析使用 `unified_mapping::DOMAIN_LABELS`，不硬编码标签数组 |
| **SHACL-003** | 可通过 `owl_axiom_for_relation()` 判断图关系是否编码了 OWL 公理（为语义级验证预留） |
| **SHACL-004** | 属性路径遍历必须 `visited` 防环（`ZeroOrMore` / `OneOrMore`） |
| **SHACL-005** | `Constraint::Pattern` 使用 `regex` crate 编译正则，编译失败时报告错误而非 panic |
| **SHACL-006** | 验证报告必须非空报告 — 所有节点都通过时 `conforms = true`, `results` 空数组 |
| **SHACL-007** | `ShaclEngine` 支持 `with_cope_version()` 过滤。设置后在 `resolve_targets` 中只选择版本匹配的焦点节点，`validate_node` 拒绝验证其他版本的节点 |

### 10.3 允许/禁止

| 允许 | 禁止 |
|------|------|
| `GraphRepository` 只读查询 | `insert_node` / `delete_node` |
| `unified_mapping` 词汇表常量 | 硬编码标签数组 |
| `regex::Regex` 编译 | panic（正则错误返回 ValidationResult） |

---

## 11. 错误处理约束

### 11.1 错误类型

所有模块共享 `ReasonerError` 枚举（定义于 `error.rs`）：

```
Dwl2Parse | Dwl2Query | SwrlParse | SwrlExecution |
ConfidenceFuse | ConfidenceError | Storage(StoreError) |
Timeout | NoMatch
```

### 11.2 硬性规则

| 规则 | 说明 |
|------|------|
| **ERR-001** | 禁止裸 `unwrap()` 在非测试代码中出现（参见 CLAUDE.md §4.1） |
| **ERR-002** | 错误信息必须包含上下文 — 哪条规则、哪个节点、什么条件触发 |
| **ERR-003** | `ConfidenceFuse` 变体携带 `{confidence, threshold, rule_name}` 三元组 |
| **ERR-004** | `Storage(StoreError)` 透传不二次封装 |
| **ERR-005** | SHACL 模块使用独立的 `ShaclError` 枚举，不混入 `ReasonerError` |

---

## 12. 并发与线程约束

| 规则 | 说明 |
|------|------|
| **THR-001** | SWRL 规则匹配阶段**每条规则只读** repo — 天然线程安全 |
| **THR-002** | 仅 `insert_derived_facts` 需要串行化 — 在 `std::thread::scope` 后由主线程执行 |
| **THR-003** | `SharedRepository = Arc<dyn GraphRepository>` 支持多线程读 |
| **THR-004** | DWL2 查询引擎不创建线程 — 所有递归调用在单线程上 |
| **THR-005** | SHACL 引擎不同线程间不共享可变状态 — 每次 `validate()` 在调用线程上完成 |

---

## 13. 存储访问约束

| 规则 | 说明 |
|------|------|
| **STORE-001** | 必须使用 `unified_mapping` 常量，禁止硬编码图词汇表字符串 |
| **STORE-002** | `get_nodes_by_label()` 参数必须是常量（如 `unified_mapping::ENTITY_LABEL`） |
| **STORE-003** | `get_relationships()` 的 `rel_type` 参数必须使用常量（如 `unified_mapping::INSTANCE_OF_REL`） |
| **STORE-004** | `node.property()` 参数必须使用常量（如 `unified_mapping::IRI_KEY`） |
| **STORE-005** | `unified_mapping::DOMAIN_LABELS` 切片用于需要迭代所有领域节点标签的场景 |

---

## 14. 查询抽象层 (QueryPlan)

### 14.1 当前状态

`query_plan.rs` 定义了 `QueryPlan` 枚举，但 **尚未完全集成**（参见 ARCHITECTURE.md §2.1 解耦计划）。

```rust
pub enum QueryPlan {
    GetByCode(String),
    GetByLabel(String),
    GetRelationships { source: String, rel_type: Option<String> },
    PatternMatch { ... },
}
```

### 14.2 迁移约束

| 规则 | 说明 |
|------|------|
| **QP-001** | 最终目标：推理层不直接 import `mapper::graph::pattern` |
| **QP-002** | 当前 `dwl2/query.rs` 仍直接构造 `GraphPattern` — 属于技术债，逐步迁移 |
| **QP-003** | 新模块（如 SHACL）应优先使用 `QueryPlan` 而非直接构造 `GraphPattern` |
| **QP-004** | `GraphRepository` 尚未实现 `execute_plan()` 方法 — 需要 adapter 层支持 |

---

## 15. 模块入口约束 (lib.rs)

`lib.rs` 是 crate 的公共 API 边界。**仅在此文件中 re-export**。

| 规则 | 说明 |
|------|------|
| **LIB-001** | 所有 `pub use` 集中在 `lib.rs`，其他模块不直接 re-export 兄弟模块的类型 |
| **LIB-002** | 新增模块必须注册到 `lib.rs`（`pub mod xxx;` + 选择性 `pub use xxx::...`） |
| **LIB-003** | `unified_mapping` 的类型通过 `ontology-storage` 导入，不在 `lib.rs` 中 re-export |

---

## 16. 测试约束

| 规则 | 说明 |
|------|------|
| **TEST-001** | 测试使用 `InMemoryAdapter`（无需 Memgraph 实例） |
| **TEST-002** | 测试中的图数据构建必须使用 `unified_mapping` 常量（见 ADR-007） |
| **TEST-003** | 每个模块的测试覆盖核心路径 + 一个错误路径 |
| **TEST-004** | SHACL 测试覆盖：形状构建、约束评估、验证报告汇总 |

---

## 17. 约束速查表

### 写权限

| 模块 | 写图 | 读图 | 纯计算 |
|------|:----:|:----:|:------:|
| `dwl2/` | ❌ | ✅ | — |
| `swrl/` | ✅ (仅 `insert_derived_facts`) | ✅ | — |
| `confidence/` | ❌ | ❌ | ✅ |
| `spatial/` | ❌ | ❌ | ✅ |
| `timeline/` | ❌ | ❌ | ✅ |
| `graph/` | ❌ | ✅ | — |
| `shacl/` | ❌ | ✅ | — |
| `reasoner.rs` | ✅ (编排层) | ✅ | — |

### 外部依赖

| 模块 | `ontology-storage` | `serde_json` | `regex` | 其他 |
|------|:---:|:---:|:---:|------|
| `dwl2/` | ✅ | ❌ | ❌ | |
| `swrl/` | ✅ | ❌ | ❌ | |
| `confidence/` | ❌ | ❌ | ❌ | `log` |
| `spatial/` | ❌ | ❌ | ❌ | |
| `timeline/` | ❌ | ❌ | ❌ | `log` |
| `graph/` | ✅ | ❌ | ❌ | `std::fs` |
| `shacl/` | ✅ | ❌ | ✅ | |
| `reasoner.rs` | ✅ | ❌ | ❌ | `log` |

---

## 18. 附录：模块文件清单

```
ontology-reasoner/src/
├── lib.rs                 # ✅ crate 公共 API
├── reasoner.rs            # ✅ Reasoner 编排层 (含 reason_on_nodes)
├── error.rs               # ✅ ReasonerError 统一错误
├── query_plan.rs          # ⚠ QueryPlan 抽象 (迁移中)
├── language.rs            # ✅ 语言前缀解析 (6 种前缀: owl2:/swrl:/sh:/rule:/action:/func:)
├── logger.rs              #   日志初始化
├── dwl2/
│   ├── mod.rs             #   模块声明
│   ├── ast.rs             #   12 种 ClassExpression + 模型结构体
│   ├── query.rs           #   Dwl2QueryEngine
│   └── README.md          #   模块文档
├── swrl/
│   ├── mod.rs             #   模块声明
│   ├── ast.rs             #   7 种 Atom + Rule + Binding
│   ├── parser.rs          #   文本规则解析
│   ├── builtins.rs        #   15+ 内置函数
│   ├── engine.rs          #   SwrlEngine (fixpoint + 并发)
│   ├── behavior.rs        #   BehaviorAction 行为引擎
│   └── README.md          #   模块文档
├── confidence/
│   ├── mod.rs             #   模块声明
│   ├── calculator.rs      #   4 维加权置信度
│   ├── fuse.rs            #   置信度熔断器
│   ├── policy.rs          #   策略引擎 (InferenceMode + SourceCategory)
│   └── README.md          #   模块文档
├── spatial/
│   ├── mod.rs             #   模块声明
│   ├── haversine.rs       #   Haversine 球面距离
│   └── README.md          #   模块文档
├── timeline/
│   ├── mod.rs             #   模块声明
│   ├── model.rs           #   TimelineInput / Segment / TimelineResult
│   ├── engine.rs          #   TimelineEngine
│   └── README.md          #   模块文档
├── graph/
│   ├── mod.rs             #   模块声明 + re-exports
│   ├── explorer.rs        #   GraphExplorer (BFS 遍历)
│   ├── detector.rs        #   StateChangeDetector trait
│   ├── util.rs            #   通用工具函数 (9 个)
│   └── README.md          #   模块文档
└── shacl/                 # ✅ 新增模块 (v0.3)
    ├── mod.rs             #   模块声明 + 文档 + re-exports
    ├── ast.rs             #   Shape / Constraint / Target / PropertyPath
    ├── engine.rs          #   ShaclEngine (验证引擎)
    ├── result.rs          #   ValidationResult / ValidationReport
    └── error.rs           #   ShaclError
```
