# timeline — 时序推演模块

## 定位

1. **日志输出** — 推演日志从引擎内部通过文件 + stderr 输出
2. **提取关联动作** — 从实体拉出所有关联的动作
3. **按时间顺序执行** — 按 seq 排序后逐步推演

## 文件

| 文件 | 职责 |
|------|------|
| `model.rs` | WaypointInput / TimelineInput / Segment / TimelineResult |
| `engine.rs` | TimelineEngine::simulate() |

## TimelineEngine

```rust
pub fn simulate(input: &TimelineInput) -> TimelineResult
```

### 流程

1. 创建日志文件 `logs/patrol_{code}_{ts}.log`
2. Haversine 距离计算（调 `spatial::haversine_m`）
3. 时间计算（dist / speed）
4. 生成 SWRL 时序字段（precondition/effect/cost/composedOf）
5. 逐步推演 + 进度条日志
6. 返回 TimelineResult

## TimelineResult

```rust
pub struct TimelineResult {
    pub segments: Vec<Segment>,        // 每段距离/时间
    pub total_distance_m: f64,         // 总距离（米）
    pub duration: i64,                  // 总时间（秒）
    pub precondition: String,           // SWRL 前置条件
    pub effect: String,                 // SWRL 执行效果
    pub composed_of: String,            // SWRL 子动作链
    pub cost: String,                   // 资源消耗
    pub logs: Vec<String>,             // 日志行
}
```

## 约束

- 纯计算，不持有图仓库引用
- 通过 `spatial::haversine_m` 做距离计算
- 日志同时写入文件和返回 log_lines
- 接收实体坐标 + speed 作为输入，不自行查询图
