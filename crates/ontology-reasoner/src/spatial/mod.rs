//! 空间计算模块 — 独立的几何/地理数学库。
//!
//! 从 timeline 模块抽离，供 DWL2 查询、SWRL 推理、HTTP 路由等多处复用。

pub mod haversine;

pub use haversine::haversine_m;
