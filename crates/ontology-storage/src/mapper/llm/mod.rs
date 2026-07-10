//! LLM 数据格式层 — 大模型输入输出的结构化处理。
//!
//! ## 设计目标
//!
//! 本体属性图与 LLM 之间的双向 JSON 转换，防止裸字符串直接流入业务层。
//!
//! ## 子模块
//!
//! | 模块        | 职责                                                        |
//! |-------------|-------------------------------------------------------------|
//! | `tool`      | 工具/函数调用定义（ToolDefinition, ToolCall, JsonSchema）   |
//! | `schema`    | 本体概念 → JSON Schema 自动生成                             |
//! | `prompt`    | 结构化 Prompt 构建器（节点/关系 → LLM 可读上下文）          |
//! | `response`  | LLM 响应解析器（JSON / ToolCall → 强类型 Rust 结构）        |
//!
//! ## 使用示例
//!
//! ```rust,ignore
//! use ontology_storage::mapper::llm::{
//!     tool::{ToolDefinition, JsonSchema, PropertySchema},
//!     response::{parse_response, LlmOutputFormat},
//!     prompt::LlmContext,
//!     schema::ontology_query_tools,
//! };
//!
//! // 1. 注册可供 LLM 调用的工具
//! let tools = ontology_query_tools();
//!
//! // 2. 构建结构化 prompt
//! let ctx = LlmContext::new("你是本体查询助手")
//!     .with_nodes(query_results)
//!     .build();
//!
//! // 3. 解析 LLM 响应
//! let response = parse_response::<MyOutputType>(raw_json, LlmOutputFormat::Auto)?;
//! ```

pub mod prompt;
pub mod response;
pub mod schema;
pub mod tool;

// 便捷 re-export
pub use prompt::LlmContext;
pub use response::{LlmOutputFormat, LlmResponse, parse_best_effort, parse_response};
pub use schema::ontology_query_tools;
pub use tool::{JsonSchema, PropertySchema, ToolCall, ToolDefinition, ToolSet};
