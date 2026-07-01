# swrl — SWRL 规则推理

## 定位

**推理函数**。执行 fixpoint 循环，匹配前提 → 绑定变量 → 推导结论 → 写入新事实。

## 文件

| 文件 | 职责 |
|------|------|
| `ast.rs` | 7 种 Atom + Rule + VariableBinding + InferenceResult |
| `parser.rs` | 文本规则解析 `[name: atom ^ ... -> atom]` |
| `builtins.rs` | 15+ 内置函数（比较/数学/字符串/布尔/列表） |
| `engine.rs` | Fixpoint 推理循环 |

## Atom (7 种)

| 变体 | 示例 | 说明 |
|------|------|------|
| `ClassAtom` | `C(?x)` | 类成员 |
| `ObjectPropertyAtom` | `P(?x,?y)` | 对象属性 |
| `DataPropertyAtom` | `D(?x,v)` | 数据属性 |
| `SameAs` | `sameAs(?x,?y)` | 等价 |
| `DifferentFrom` | `differentFrom(?x,?y)` | 不等价 |
| `Builtin` | `swrlb:fn(args)` | 内置函数 |
| `Query` | `Query(?x, expr_key)` | ✅ DWL2 子查询 |

## SwrlEngine

```rust
pub fn new(repo) -> Self                    // 自动创建 Dwl2QueryEngine + 默认 policy=None
pub fn with_policy(policy) -> Self          // 注入置信度策略
pub fn with_max_iterations(n) -> Self       // 最大迭代数
pub fn with_verbose(v) -> Self              // 详细日志
pub fn execute_rules(rules) -> Result       // fixpoint 循环（多规则串行）
pub fn execute_rule(rule) -> Result         // 单条规则：匹配→置信度→熔断→代入
```

### 内部字段

```rust
pub struct SwrlEngine {
    repo: SharedRepository,                    // 图仓库
    builtins: BuiltinRegistry,                 // 内置函数
    confidence_calc: ConfidenceCalculator,     // 置信度计算
    max_iterations: usize,                     // 最大迭代 (默认 100)
    derived_facts: HashSet<String>,            // 已推导事实去重
    verbose: bool,                             // 详细日志
    policy: Option<ConfidencePolicy>,          // 置信度策略
    dwl2_engine: Option<Dwl2QueryEngine>,      // DWL2 查询引擎（Query atom 用）
}
```

### match_query_atom (DWL2 子查询)

```rust
fn match_query_atom(&self, atom: &Atom) -> Result<Vec<VariableBinding>>
```

解析 `Query(?x, expr_key)` → `ClassExpression::from_key()` → `Dwl2QueryEngine::retrieve_instances()` → 每个结果 IRI 绑定到 `?x`

## 执行流程

1. `match_antecedent` → 逐步连接 class/prop/eq/builtin/query 原子
2. 每个绑定 → 计算置信度 → 熔断检查 → 代入结论
3. 新事实去重写入 → 循环至固定点

## 约束

- 变量安全性：consequent 变量必须都在 antecedent 中出现
- Query 原子在 antecedent 阶段调用 `Dwl2QueryEngine::retrieve_instances`
- 置信度 < threshold 时返回 `ConfidenceFuse` 错误
