//! # gie — 通用推理引擎 (General Inference Engine)
//!
//! `gie` 是 `ontology-reasoner` 的推理引擎核心，负责编排复杂推理逻辑：
//!
//! - **语言翻译**：将 7 种语言前缀表达式（`rdfs:`/`owl2:`/`swrl:`/`sh:`/`rule:`/`func:`/`$.`）
//!   翻译为可执行的查询计划（[`QueryPlan`]）
//! - **场景版本控制**：管理 `cope_version`，含快照/回滚/清理完整生命周期
//! - **推理调度**：Fixpoint 不动点迭代循环（在 Engine 内部）
//! - **动作路由**：将解析后的动作按前缀路由到对应的执行引擎
//!
//! ## 架构分层
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │  L1: Gateway (ontology-server / Axum)                                   │
//! │  - 接收 HTTP 请求 (POST /reason, /infer-forward)                        │
//! │  - 解析 JSON -> 调用 Reasoner API                                       │
//! └───────────────────────┬─────────────────────────────────────────────────┘
//!                         │ (Direct Call / spawn_blocking)
//!                         ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │  L2: Reasoner Core (ontology-reasoner) <--- GIE 所在地                  │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐   │
//! │  │  Translator │  │   Version   │  │   Engine    │  │   Action    │   │
//! │  │  (Adapter)  │  │  (Scene)    │  │  (Core)     │  │  (Router)   │   │
//! │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘   │
//! │         │                │                │                │          │
//! │         ▼                ▼                ▼                ▼          │
//! │  [SWRL/DWL2 Parser] [Cope Version]  [Fixpoint Loop]  [Func Call]      │
//! │  [JSONPath Parser]    [Snapshot]    [Confidence]     [LLM Route]      │
//! └───────────────────────┬───────────────────────────────────────────────┘
//!                         │ (QueryPlan Trait)
//!                         ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │  L3: Storage (ontology-storage)                                         │
//! │  - GraphRepository Trait (execute_plan)                                 │
//! │  - Memgraph Adapter (neo4rs)                                            │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## 架构约束
//!
//! 1. **存储解耦**：gie 严禁直接构造 [`GraphPattern`] 或任何 `ontology-storage` 内部 IR。
//!    所有图查询必须通过构造 [`QueryPlan`] 并调用 [`GraphRepository::execute_plan()`] 完成。
//!    参见 ARCHITECTURE.md §2.1 和 CRATE_CONSTRAINTS.md §14。
//!
//! 2. **词汇表常量**：所有图词汇表（标签、关系类型、属性键）必须使用
//!    [`unified_mapping`] 中定义的常量，严禁硬编码字符串字面量。
//!    参见 CRATE_CONSTRAINTS.md §13 (STORE-001 ~ STORE-005)。
//!
//! 3. **宽容执行**：有字段则用，无字段则跳过，用默认值兜底继续执行。
//!    绝不因字段缺失而 panic 或中断链路。
//!    参见 ARCHITECTURE.md §19。
//!
//! 4. **id 锚定**：所有 CRUD 操作以 `id` 为唯一技术锚点，`code`/`name` 仅用于展示与搜索。
//!    参见 ARCHITECTURE.md §20。
//!
//! ## 模块结构
//!
//! ```text
//! gie/
//! ├── mod.rs              # 模块入口 + 公共 re-exports
//! ├── context.rs          # InferenceContext — 推理过程状态传递
//! ├── engine/             # InferenceEngine — 推理总入口 (Core)
//! │   ├── mod.rs          #   Engine 定义 + Confidence 管理
//! │   └── fixpoint.rs     #   Fixpoint 不动点迭代循环
//! ├── version.rs          # VersionControl — cope_version + 快照/回滚/清理
//! ├── translator/         # 语言翻译器 (Adapter)
//! │   ├── mod.rs
//! │   ├── swrl.rs         #   SWRL 规则 → QueryPlan 翻译
//! │   ├── dwl2.rs         #   DWL2 类表达式 → QueryPlan 翻译
//! │   └── jsonpath.rs     #   JSONPath RFC 9535 → QueryPlan 翻译
//! └── action/             # 动作路由器 (Router)
//!     ├── mod.rs
//!     └── router.rs       #   按语言前缀路由到对应引擎
//! ```

pub mod action;
pub mod context;
pub mod engine;
pub mod translator;
pub mod version;
