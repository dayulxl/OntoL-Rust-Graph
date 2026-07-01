//! 时序推演模块。
//!
//! 巡逻任务的航点仿真——计算距离、时间、沿航线逐步推演坐标。
//! 日志通过 `log` crate 从引擎内部输出到 stderr。

pub mod engine;
pub mod model;

pub use engine::TimelineEngine;
pub use model::{Segment, StrikeInput, StrikeResult, TimelineInput, TimelineResult, WaypointInput};
