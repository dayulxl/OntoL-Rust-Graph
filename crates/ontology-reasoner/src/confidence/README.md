# confidence — 置信度体系

## 定位

独立模块，在每次推理步骤前/后进行置信度校验。熔断器作为守护条件。

## 文件

| 文件 | 职责 |
|------|------|
| `calculator.rs` | 4 维加权置信度计算 |
| `fuse.rs` | 熔断器 (< threshold → abort) |
| `policy.rs` | ✅ 策略引擎 (SourceCategory + InferenceMode + ConfidencePolicy) |

## ConfidenceCalculator

4 维加权求和: `Σ(source_i × weight_i) / Σ(weight_i)`

```rust
pub struct ConfidenceInput {
    pub source_match: f64,        // 前提匹配度 (0.35)
    pub source_cardinal: f64,     // 基数满足度 (0.20)
    pub source_property: f64,     // 属性一致性 (0.25)
    pub source_structural: f64,   // 结构匹配度 (0.20)
    pub source_category: Option<SourceCategory>,  // ✅ 数据来源
}
```

## ConfidencePolicy

```rust
pub struct ConfidencePolicy {
    pub mode: InferenceMode,  // Permissive(0.15) / Strict(0.50) / Balanced(0.30)
    source_weight_overrides,  // 按 SourceCategory 覆盖权重
}
```

## SourceCategory

```rust
SonarRealtime(0.45) > Satellite(0.30) > Historical(0.25) > Unknown(0.20)
```

## HTTP 端点

```
POST /confidence/policy  { "mode": "Permissive" }
```

## 约束

- 置信度计算独立于推理逻辑，作为装饰器
- 熔断不终止整个 fixpoint，只跳过当前规则
- Policy 通过 `ReasonerConfig` 或 `POST /confidence/policy` 注入
