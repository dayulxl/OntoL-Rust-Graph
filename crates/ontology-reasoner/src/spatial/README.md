# spatial — 空间计算模块

## 定位

独立的几何/地理数学库。不含任何推理逻辑或图操作依赖。

## 文件

| 文件 | 职责 |
|------|------|
| `haversine.rs` | Haversine 球面距离公式 |

## API

```rust
pub const EARTH_RADIUS_M: f64 = 6_371_000.0;
pub fn haversine_m(lat1, lon1, lat2, lon2) -> f64;
```

## 被调用方

- `timeline/engine.rs` — 航点距离计算
- （未来）`dwl2/query.rs` — 空间范围查询

## 约束

- 纯数学，无副作用
- 输入输出均为 `f64`
- 结果单位为**米**
