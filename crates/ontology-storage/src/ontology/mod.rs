//! 本体 CRUD 操作 — 大模型创建本体的核心逻辑。
//!
//! 将 JSON 参数转换为属性图节点/关系，执行校验后写入存储层。
//! 函数签名使用 `&dyn GraphRepository`，不依赖 HTTP 层。
//!
//! ## 子模块
//!
//! | 模块            | 职责                                   |
//! |-----------------|----------------------------------------|
//! | `entity`        | Entity / Type / Patrol 节点创建         |
//! | `relationship`  | 关系创建 + 跨标签节点解析               |

pub mod entity;
pub mod relationship;
