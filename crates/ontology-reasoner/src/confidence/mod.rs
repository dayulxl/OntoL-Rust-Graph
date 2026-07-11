//! 置信度评估与熔断模块。
//!
//! ## 设计原则
//!
//! 置信度计算独立于推理逻辑，作为**装饰器**在每次推理步骤前后校验。
//! 熔断器（`ConfidenceFuse`）是守护条件 — 一旦置信度 < 0.3 立即截断链路。
//!
//! ## 子模块
//!
//! | 模块        | 职责                                   |
//! |-------------|----------------------------------------|
//! | `calculator`| 多源置信度加权计算                     |
//! | `fuse`      | 熔断器：阈值检查与链路截断              |

pub mod calculator;
pub mod fuse;
pub mod policy;

pub use calculator::ConfidenceCalculator;
pub use fuse::ConfidenceFuse;
pub use policy::{ConfidencePolicy, InferenceMode, SourceCategory};
