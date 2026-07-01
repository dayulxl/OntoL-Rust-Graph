# adapters — 适配器层

## 定位

实现 `GraphRepository` trait 的后端。通过 feature flag 选择编译。

## 子模块

| 模块 | Feature | 连接方式 | 状态 |
|------|---------|---------|------|
| `in_memory/` | `in-memory` (默认) | 无依赖，HashMap + 邻接表 | ✅ 生产可用 |
| `neo4j/` | `neo4j` | ureq HTTP → `POST /db/neo4j/tx/commit` | ✅ 生产可用 |
| `vector/` | — | — | 📋 规划 |

## 切换后端

```rust
// Neo4j
let repo = StorageConfig::Neo4j { uri: "http://localhost:7474", user: "neo4j", password: "secret" }.build()?;

// InMemory
let repo = StorageConfig::InMemory.build()?;
```

## 约束

- 所有适配器返回 `SharedRepository = Arc<dyn GraphRepository>`
- InMemory 适配器不支持事务
- Neo4j 适配器通过 `POST /db/neo4j/tx/commit` 发送参数化 Cypher（非 Bolt 协议）
- Neo4j URI 格式：本地 `http://localhost:7474`，AuraDB `https://<instance-id>.databases.neo4j.io`
- 连接参数通过环境变量 `ONTOLOGY_NEO4J_URI` / `ONTOLOGY_NEO4J_USER` / `ONTOLOGY_NEO4J_PASSWORD` 注入
- 上层业务通过 `ontology/` 子模块（Entity/Type/Patrol/Relationship CRUD）操作，不直接调用适配器

## 参见

- 项目根 [ARCHITECTURE.md](../../../ARCHITECTURE.md)
- [repository/README.md](../repository/README.md) — GraphRepository trait 定义
- [ontology/](../ontology/) — 本体 CRUD 封装
