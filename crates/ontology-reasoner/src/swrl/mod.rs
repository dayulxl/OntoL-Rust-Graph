//! SWRL (Semantic Web Rule Language) 推理模块。
//!
//! ## 子模块
//!
//! | 模块        | 职责                                              |
//! |-------------|---------------------------------------------------|
//! | `ast`       | SWRL 规则 AST：Atom, Rule, VariableBinding        |
//! | `parser`    | 文本规则解析器，将 SWRL 语法字符串 → AST          |
//! | `builtins`  | SWRL 内置函数实现（swrlb:equal, swrlb:greaterThan 等）|
//! | `engine`    | 规则执行引擎：匹配前提 → 绑定变量 → 生成推论       |
//!
//! ## SWRL 规则语法（支持的子集）
//!
//! ```text
//! [ruleName: ClassAtom(...) ^ PropertyAtom(...) ^ ... -> ConsequentAtom(...) ^ ...]
//! ```
//!
//! 示例：
//! ```text
//! [parentChild: hasParent(?x, ?y) ^ hasBrother(?y, ?z) -> hasUncle(?x, ?z)]
//! ```

pub mod ast;
pub mod behavior;
pub mod builtins;
pub mod engine;
pub mod parser;

pub use ast::{Atom, Rule, VariableBinding};
pub use behavior::{
    BehaviorAction, BehaviorResult, evaluate_shacl_precondition, execute_behaviors_batch,
    execute_effect, parse_behavior,
};
pub use builtins::BuiltinRegistry;
pub use engine::SwrlEngine;
pub use parser::SwrlParser;
