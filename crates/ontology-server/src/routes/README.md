# routes — HTTP 路由模块

## 端点

| 方法 | 路径 | 文件 | 功能 |
|------|------|------|------|
| GET | `/health` | `health.rs` | 健康检查 + Entity/Type 计数 |
| GET | `/schema` | `schema.rs` | ASW 知识图谱 JSON Schema |
| GET | `/tools` | `tools.rs` | LLM Function Calling 工具定义（12 个工具） |
| POST | `/tools/call` | `tools_call.rs` | LLM 统一调度入口（OpenAI FC 兼容） |
| POST | `/query` | `query.rs` | Entity 搜索（code/type/红蓝方/空间/关键词/层级） |
| POST | `/reason` | `reason.rs` | SWRL 推理（含熔断 422） |
| POST | `/context` | `context.rs` | Entity 图上下文（关系+Type 层级链） |
| GET\|POST | `/patrol` | `patrol.rs` | 巡逻任务查询/提交 + 时序推演 |
| GET\|POST | `/strike` | `strike.rs` | 打击决策推演（射程/命中/毁伤） |
| POST | `/nl-query` | `nl_query.rs` | 自然语言 → DWL2 查询 |
| POST | `/infer-forward` | `infer.rs` | 向前推理（自动遍历+状态推断+规则匹配+预测） |
| POST | `/ontology/create` | `ontology_create.rs` | LLM 创建本体（Entity/Type/Patrol + 批量） |
| POST | `/relationships/create` | `ontology_relationship.rs` | LLM 创建关系（自动节点解析） |
| POST | `/confidence/policy` | `confidence_policy.rs` | 切换推理策略（Balanced/Permissive/Strict） |
| GET\|POST | `/rules` | `rules.rs` | GET 列出已加载规则；POST 热加载 SWRL 规则 |

## patrol.rs 核心流程

```
POST /patrol
  → 找实体（[:移动] 关系 或 type 匹配）
  → ⚠ 前置条件检查（check_preconditions: 解析 SWRL precondition 中的 swrlb:greaterThanOrEqual/greaterThan/equal，对比实体 power/speed/status）
  → TimelineEngine::simulate()（距离+时间+推演）
  → 写回巡逻节点 + 更新实体坐标
  → ⚠ composedOf 级联推理（解析子动作链 ^ 分隔，遍历关联实体递归传导）
```

## 前置条件检查

`check_preconditions()` 解析 patrol JSON + 实体 `precondition` 字段：
- `swrlb:greaterThanOrEqual(?x.power, 150)` → 检查实体 power ≥ 150
- `swrlb:greaterThan(?x.speed, 20)` → 检查实体 speed > 20  
- `swrlb:equal(?x.status, '有效')` → 检查实体 status = '有效'
- 不满足 → 返回 `{"status":"blocked", "reason":"..."}`

## composedOf 级联推理

推理完成后解析 `composedOf` 中的子动作链（如 `HOVER(第1航点,...) ^ SCAN(第2航点,...)`），遍历所有 Entity 检查是否有其他实体引用了这些子动作作为自己的 `composedOf`，发现级联则输出递归传导日志。

## 统一错误格式

```json
{"error": "描述"}
```

## infer.rs — 产品/业务层隔离 (v0.3)

`infer.rs` 是这个项目中**唯一实现产品/业务分层**的路由文件：

```
┌──────────────────────────────────┐
│  infer.rs (业务层)                │
│  ├─ JSON 解析 / HTTP 响应          │
│  ├─ MilitaryStateChangeDetector   │  ← 领域知识（军事 ASW）
│  │   ├─ Space_abs → haversine     │
│  │   ├─ "移动"/"打击" 中文语义     │
│  │   └─ speed/power/confidence    │
│  └─ build_response() → JSON      │
└────────────┬─────────────────────┘
             │ 调用 trait 实现
┌────────────▼─────────────────────┐
│  reasoner::graph (产品层 / 框架)   │
│  ├─ GraphExplorer (BFS 遍历)      │  ← 零领域知识
│  ├─ StateChangeDetector trait     │  ← 只定义接口
│  └─ util (查找/汇总/预测/匹配)     │
└──────────────────────────────────┘
```

**换业务场景**——只换 `StateChangeDetector` 实现，`GraphExplorer` 不动。

## 约束

- 所有 handler 签名: `pub fn handle(request: &mut Request, state: &Arc<Mutex<AppState>>) -> (u16, String)`
- 直接操作 `app.repo` 或调用 `app.reasoner`，不创建新连接
