# dwl2 — DWL2 DL 描述逻辑查询

## 定位

**获取实体对象**。将 `ClassExpression` AST 编译为 `GraphPattern`，在属性图上执行查询。

## 文件

| 文件 | 职责 |
|------|------|
| `ast.rs` | 12 种 DL 构造子 + 辅助类型 |
| `query.rs` | AST → GraphPattern 编译器 + `Dwl2QueryEngine` |

## ClassExpression (12 种)

```rust
Top, Bottom, ClassName, Intersection, Union, Complement,
AllValuesFrom, SomeValuesFrom, MinCardinality, MaxCardinality,
ExactCardinality, OneOf, SelfRestriction
```

## 序列化 (用于 SWRL Query atom)

```rust
expr.to_key()    // → "ClassName(http://ex#Person)"
ClassExpression::from_key(s)  // 反序列化
```

格式: `Variant(arg1,arg2,...)` 递归嵌套。

## Dwl2QueryEngine

```rust
pub fn new(repo: SharedRepository) -> Self
pub fn execute(query: &Dwl2Query) -> Result<Dwl2Result, ReasonerError>
pub fn retrieve_instances(expr: &ClassExpression) -> Result<Vec<String>, ReasonerError>
```

## 约束

- 只读操作，不修改图数据
- 不进入 SWRL 模块的内部调用链（通过 `SharedRepository` 独立访问图存储）
- `from_key()` 解析失败返回 `String` 错误
