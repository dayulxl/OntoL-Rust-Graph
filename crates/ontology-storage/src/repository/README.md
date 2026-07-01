# repository — 抽象仓库层

## 定位

业务代码依赖的唯一接口层。定义 `GraphRepository` trait 和 `Transaction` trait，不依赖任何具体存储实现。

## 核心类型

| 文件 | 类型 | 说明 |
|------|------|------|
| `graph_store.rs` | `GraphRepository` | 属性图 CRUD trait |
| `graph_store.rs` | `SharedRepository` | `Arc<dyn GraphRepository>` 类型别名 |
| `transaction.rs` | `Transaction` | 事务抽象 (begin/commit/rollback) |

## GraphRepository 方法

```rust
fn get_node(id) -> Option<Node>
fn get_nodes_by_label(label) -> Vec<Node>
fn get_relationships(node_id, rel_type?) -> Vec<Relationship>
fn query_pattern(pattern: &GraphPattern) -> Vec<(Node, Vec<Relationship>, Node)>
fn insert_node(node) -> String (iri)
fn insert_relationship(rel)
fn delete_node(id) -> usize
fn delete_relationship(id) -> usize
```

## 版本与演进

当前 trait 为 v0.2 同步版本。v0.3 规划方向：
- 新增 `execute_plan(&self, plan: &QueryPlan) -> Result<QueryResult>` — 推理层通过 `QueryPlan` 抽象访问，不再直接构造 `GraphPattern`
- 逐步标记 `query_pattern()` 为 deprecated
- 长期（Phase 2）：增加 `async fn` 方法供 Bolt 异步驱动使用

## 约束

- **Send + Sync**：trait 绑定 `Send + Sync`，必须线程安全
- **同步**：所有方法为同步签名，不依赖 tokio
- **不可修改**：此 trait 是架构边界，新增方法需全局评审
- **事务**：InMemory 适配器不支持事务，Neo4j 适配器通过 auto-commit 操作

## 测试

```bash
cargo test -p ontology-storage           # 26 个单元测试（所有适配器）
```
