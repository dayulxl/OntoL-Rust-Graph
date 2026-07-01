# mapper — 核心转换层

## 定位

本体模型 ↔ 属性图（Property Graph）之间的双向映射。不是 RDF 三元组。

## 子模块

| 模块 | 职责 | 关键文件 |
|------|------|---------|
| `graph/` | 属性图 IR（存储无关） | `node.rs`, `relationship.rs`, `pattern.rs`, `property.rs` |
| `cypher/` | Cypher 方言生成 | `builder.rs`（Pattern→Cypher）, `params.rs`（参数绑定） |
| `llm/` | LLM 数据格式层 | `tool.rs`, `schema.rs`, `prompt.rs`, `response.rs` |

## 数据模型

```
Class         → (:`:Class` { iri, label })         节点
ObjectProperty → (domain)-[:HAS_PROPERTY]->(prop)-[:HAS_RANGE]->(range)
Individual     → (p:`:Individual` { iri, label })  节点
rdf:type       → (ind)-[:INSTANCE_OF]->(class)
数据属性值      → (ind)-[:HAS_VALUE]->(:`Property`)  + rel 上 value 属性
```

## 编码约定

- `PropertyValue`: 7 种变体 (String/Integer/Float/Boolean/List/Map/Null)，零外部依赖
- `GraphPattern: { start: NodePattern, relationship: RelationshipPattern, end: NodePattern }` — 三元组模式匹配
- Cypher 生成必须参数化，防止注入
