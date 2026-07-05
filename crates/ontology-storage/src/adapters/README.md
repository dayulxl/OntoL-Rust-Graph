# adapters — 适配器层

## 定位

实现 `GraphRepository` trait 的后端。通过 feature flag 选择编译。

## 子模块

| 模块 | Feature | 连接方式 | 状态 |
|------|---------|---------|------|
| `memgraph/` | `memgraph` (默认) | neo4rs Bolt → Memgraph | ✅ 主力后端 |
| `in_memory/` | `in-memory` | 无依赖，HashMap + 邻接表 | ✅ 测试用 |

## 切换后端

```rust
// Memgraph（默认）
let repo = StorageConfig::Memgraph { uri: "memgraph://localhost:7687", user: "", password: "" }.build()?;

// InMemory（测试/回退）
let repo = StorageConfig::InMemory.build()?;
```

## 约束

- 所有适配器返回 `SharedRepository = Arc<dyn GraphRepository>`
- InMemory 适配器不支持事务
- Memgraph 通过 Bolt 协议连接（`memgraph://` scheme），默认端口 7687
- 连接参数通过环境变量 `ONTOLOGY_GRAPH_URI` / `ONTOLOGY_GRAPH_USER` / `ONTOLOGY_GRAPH_PASSWORD` 注入

## 参见

- 项目根 [CLAUDE.md](../../../CLAUDE.md)
- [repository/README.md](../repository/README.md) — GraphRepository trait 定义
