# ontology-storage — 存储适配层

> **⚠ 此文件已合并到项目根目录的 [ARCHITECTURE.md](../../ARCHITECTURE.md)。**
>
> 本 crate 的架构说明、职责边界、Feature Flags、配置管理等全部内容请查看根文档。
>
> 本文件保留以提供 crate 级别的快速索引。

---

## 快速索引

| 模块 | 说明 | 详细文档 |
|------|------|----------|
| `repository/` | `GraphRepository` trait + `Transaction` | [repository/README.md](src/repository/README.md) |
| `adapters/neo4j/` | Neo4j HTTP API 适配器 | [adapters/README.md](src/adapters/README.md) |
| `adapters/in_memory/` | 内存 HashMap 适配器 | — |
| `mapper/graph/` | 属性图 IR (Node/Relationship/Pattern) | [mapper/README.md](src/mapper/README.md) |
| `mapper/cypher/` | Cypher 查询生成 | — |
| `mapper/llm/` | LLM 数据格式层 (→ gateway 📋) | — |
| `ontology/` | Entity/Type/Patrol/Relationship CRUD | — |

## 连接后端

```rust
use ontology_storage::factory::StorageConfig;

// Neo4j
let repo = StorageConfig::Neo4j {
    uri: "http://localhost:7474".into(),
    user: "neo4j".into(),
    password: std::env::var("ONTOLOGY_NEO4J_PASSWORD").unwrap_or_default(),
}.build()?;

// InMemory
let repo = StorageConfig::InMemory.build()?;
```

## 运行测试

```bash
cargo test -p ontology-storage
```
