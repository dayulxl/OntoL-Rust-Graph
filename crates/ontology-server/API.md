# Ontology Server — HTTP API

Base: `http://localhost:8085`

所有响应 `Content-Type: application/json; charset=utf-8`。

## 端点总览

| 方法 | 路径 | 作用 |
|------|------|------|
| GET | `/health` | 健康检查 + Entity/Type 计数 |
| GET | `/schema` | 知识图谱数据模型定义 |
| GET | `/tools` | LLM Function Calling 工具定义（12 个工具） |
| POST | `/tools/call` | LLM 统一调度入口（OpenAI FC 兼容） |
| POST | `/query` | Entity 搜索（code/type/红蓝方/空间/关键词/层级） |
| POST | `/reason` | SWRL 规则推理（含置信度熔断 422） |
| POST | `/context` | Entity 图上下文（关系+Type 层级链+摘要） |
| GET\|POST | `/patrol` | 巡逻任务查询/提交 + 时序推演 |
| GET\|POST | `/strike` | 打击决策推演（射程/命中/毁伤） |
| POST | `/nl-query` | 自然语言 → DWL2 查询 |
| POST | `/infer-forward` | 向前推理（自动遍历+状态推断+规则匹配+预测） |
| POST | `/ontology/create` | LLM 创建本体（Entity/Type/Patrol + 批量） |
| POST | `/relationships/create` | LLM 创建关系（自动节点解析） |
| POST | `/confidence/policy` | 切换推理策略（Balanced/Permissive/Strict） |
| GET\|POST | `/rules` | GET 列出已加载规则；POST 热加载 SWRL 规则 |

---

## GET /health

```
作用：服务状态 + 节点统计
方法：GET
路径：/health
参数：无
```

```json
// 响应 200
{
  "status": "ok",
  "service": "ontology-server",
  "version": "0.1.0",
  "backend": "neo4j",
  "counts": { "entities": 50, "types": 44 }
}
```

---

## GET /schema

```
作用：知识图谱数据模型（Entity/Type 字段定义 + 关系定义）
方法：GET
路径：/schema
参数：无
```

```json
// 响应 200
{
  "domain": "Anti-SubmarineWarfare",
  "labels": { "Entity": { "count": 50, "standard_fields": { ... } } },
  "relationships": { "subClassOf": "...", "移动": "..." }
}
```

---

## GET /tools

```
作用：LLM Function Calling 工具定义（注册到大模型用）
方法：GET
路径：/tools
参数：无
```

```json
// 响应 200 — 12 个工具:
// search_entities        — 搜索 Entity（code/type/红蓝方/空间范围/关键词）
// get_entity_context     — Entity 图邻域上下文
// load_swrl_rule         — 加载 SWRL 规则
// execute_reasoning      — 执行推理
// update_entity          — 更新 Entity 字段
// create_entity          — 创建 Entity 节点
// create_type            — 创建 Type 分类节点
// create_patrol          — 创建 Patrol 巡逻节点
// simulate_patrol        — 巡逻时序推演
// simulate_strike        — 打击决策推演
// infer_forward          — 向前推理
// create_relationship    — 创建关系
```

---

## POST /tools/call

```
作用：LLM 统一调度入口（兼容 OpenAI Function Calling 格式）
方法：POST
路径：/tools/call
Content-Type: application/json
```

```json
// 请求 — 调用单个工具
{
  "name": "search_entities",
  "arguments": { "code": "P8A_001" }
}

// 响应 200
{ "results": [...] }
```

---

## POST /query

```
作用：查询 Entity 节点
方法：POST
路径：/query
Content-Type: application/json
```

**1) 按 code 精确查找**

```json
// 请求
{ "code": "P8A_001" }

// 响应 200
{
  "labels": ["Entity"],
  "properties": { "code": "P8A_001", "name": "P-8A-1", "type": "P-8A海神巡逻机", "command_side": 1, "Space_abs": [23.13, 12.8, -30, 0], "duration": 100 }
}
```

**2) 按 type 过滤**

```json
{ "type": "尼米兹级" }
// → { "count": 10, "entities": [...] }
```

**3) 按红蓝方过滤**

```json
{ "command_side": 1 }
```

**4) 按分类层级搜索**

```json
{ "subclass_of": "航母" }
// → 返回航母 + 尼米兹级 + 福特级 的全部 Entity
```

**5) 空间范围搜索**

```json
{ "lat": 23.0, "lon": 12.0, "radius": 1.0 }
// → 返回圆心 (23,12) 半径 1° 内的 Entity
```

**6) 关键词搜索**

```json
{ "keyword": "尼米兹" }
```

**7) 组合过滤**

```json
{ "type": "阿利伯克级", "command_side": 1 }
```

---

## POST /context

```
作用：Entity 全景上下文（属性 + 出入关系 + Type 层级链 + 自然语言摘要）
方法：POST
路径：/context
Content-Type: application/json
```

```json
// 请求
{ "code": "P8A_001", "depth": 2 }

// 响应 200
{
  "entity": {
    "code": "P8A_001",
    "labels": ["Entity"],
    "properties": { "name": "P-8A-1", "type": "P-8A海神巡逻机", "command_side": 1, "Space_abs": [23.13, 12.8, -30, 0], "duration": 100 }
  },
  "outgoing": [
    { "relation": "移动", "target_code": "PATROL_001", "target_props": { "name": "巡逻", "duration": 500 } }
  ],
  "incoming": [],
  "type_hierarchy": ["P-8A海神巡逻机", "固定翼飞机", "有人机", "飞机"],
  "summary": "Entity 'P-8A-1' (P8A_001) — 1 outgoing rels, type chain: P-8A海神巡逻机 → 固定翼飞机 → 有人机 → 飞机"
}
```

---

## POST /reason

```
作用：SWRL 规则推理（fixpoint 循环），置信度 < 0.3 自动熔断
方法：POST
路径：/reason
Content-Type: application/json
```

```json
// 请求 — 对已加载规则执行推理
{}

// 请求 — 加载新规则并执行
{
  "rules": ["[ruleName: atom ^ ... -> atom]"],
  "incremental": false
}

// 响应 200
{ "ok": true, "rules_loaded": 2, "total_steps": 3, "derived_facts": 7, "fuse_trips": 0, "elapsed_ms": 45 }

// 响应 422 — 置信度熔断
{ "error": "confidence_fuse_tripped", "confidence": 0.25, "threshold": 0.3, "rule_name": "ruleName" }
```

---

## GET /patrol

```
作用：查询巡逻任务
方法：GET
路径：/patrol 或 /patrol?code=xxx
参数：code（可选，字符串）
```

```json
// GET /patrol → 响应 200
{
  "count": 1,
  "patrols": [{
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "code": "PATROL_2023_A_SECTOR",
    "name": "A海域例行反潜巡航",
    "waypoints": [
      { "seq": 1, "lat": 32.058, "lon": 118.796, "alt": 500, "action": "HOVER" },
      { "seq": 2, "lat": 32.062, "lon": 118.801, "alt": 500, "action": "SCAN" }
    ],
    "duration_estimated_s": 200
  }]
}
```

---

## POST /patrol

```
作用：JSON 解析 → 调用推理机 TimelineEngine 执行时序推演 → 回写结果到 Neo4j
推理链路：
  ontology-server(HTTP) → ontology-reasoner::timeline::TimelineEngine::simulate()
    1. Haversine 公式计算当前坐标→航点1→航点2→...的地表距离（m）
    2. 时间 = 距离 / speed，累计填入 duration
    3. 自动生成 SWRL 时序字段写入巡逻节点
    4. 沿航点逐步推演：更新实体 Space_abs + 消耗 duration
    5. 推演日志通过 log::info! 输出到服务端 stderr（不在 HTTP 响应体中）
方法：POST
路径：/patrol
Content-Type: application/json
```

```json
// 请求
[{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "code": "PATROL_2023_A_SECTOR",
  "name": "A海域例行反潜巡航",
  "waypoints": [
    { "seq": 1, "lat": 32.058, "lon": 118.796, "alt": 500, "action": "HOVER" },
    { "seq": 2, "lat": 32.062, "lon": 118.801, "alt": 500, "action": "SCAN" }
  ]
}]

// 响应 201
{
  "ok": true,
  "count": 1,
  "patrols": [{
    "code": "PATROL_2023_A_SECTOR",
    "name": "A海域例行反潜巡航",
    "entity": "P8A_001",
    "current_position": { "lat": 23.1291, "lon": 12.8, "alt": 0 },
    "speed_m_s": 200.0,
    "waypoints": [...],
    "total_distance_m": 847123.5,
    "total_distance_km": "847.12",
    "total_time_s": 4235.6,
    "duration": 4235,
    "priority": 3,
    "precondition": ":Entity(P8A_001) ^ 移动(P8A_001, PATROL_2023_A_SECTOR) ^ hasSpeed(P8A_001, 200) ^ hasStatus(P8A_001, '有效')",
    "effect": ":Patrol(PATROL_2023_A_SECTOR) ^ hasStatus(PATROL_2023_A_SECTOR, '已执行') ^ hasCovered(PATROL_2023_A_SECTOR, true) ^ hasDuration(PATROL_2023_A_SECTOR, 4235)",
    "composedOf": "HOVER(第1航点,135694m,678.5s) ^ SCAN(第2航点,711429m,3557.1s)",
    "cost": "847124m, 4235s",
    "status": "executed"
  }]
}
// 推演日志输出到服务端终端（stderr），不在 HTTP 响应中
```

---

## GET|POST /strike

```
作用：打击决策推演 — 攻击方/目标/武器/射程/命中概率/毁伤等级评估
方法：GET（查询）/ POST（提交推演）
路径：/strike
Content-Type: application/json
```

```json
// GET /strike → 查询已有打击决策
// POST /strike → 提交打击推演请求
{
  "attacker": "DDG_101",
  "target": "TGT_001",
  "weapon": "鱼雷",
  "range_m": 5000
}
```

---

## POST /nl-query

```
作用：自然语言 → DWL2 查询语句
方法：POST
路径：/nl-query
Content-Type: application/json
```

```json
// 请求
{ "query": "查找所有蓝方舰船" }

// 响应 200
{ "dwl2": "Some(hasCommandSide, Class(:蓝方))", "results": [...] }
```

---

## POST /infer-forward

```
作用：通用图推理 — 传入实体ID + 关系名，系统全自动进行图遍历、状态推断、规则匹配、下一步预测
方法：POST
路径：/infer-forward
Content-Type: application/json
推理链路：
  1. 实体解析 — 在所有标签类型（Entity/Patrol/Strike/Type）中按 id 查找实体
  2. 多跳 BFS 图遍历 — 沿指定关系逐跳遍历，支持 outoing/incoming/both 方向
  3. 状态变化检测 — 对比每跳源/目标节点的位置(Space_abs)、状态、速度、功率、置信度、组合动作链
  4. SWRL 规则匹配 — 扫描 rules/*.swrl 文件中与当前跳相关的规则
  5. 下一步预测 — 分析目标节点所有出向关系，按频率排序推荐后续操作
  6. 类型层次 — 沿 subClassOf 向上获取目标节点的类型祖代链
```

**必填字段:** `id`, `name`, `relation`
**可选字段:** `depth`(默认3, 最大5), `direction`("outgoing"|"incoming"|"both", 默认"outgoing")

```json
// 请求（单个对象）
{
  "id": "P8A_001",
  "name": "反潜巡逻推演",
  "relation": "移动",
  "depth": 3,
  "direction": "outgoing"
}

// 请求（数组，批量推理）
[{
  "id": "P8A_001",
  "name": "P-8A巡逻链",
  "relation": "移动"
}, {
  "id": "DDG_101",
  "name": "驱逐舰打击链",
  "relation": "打击"
}]

// 响应 200
{
  "ok": true,
  "count": 1,
  "results": [{
    "source": {
      "id": "P8A_001",
      "name": "P-8A海神巡逻机",
      "labels": ["Entity"],
      "properties": { "code": "P8A_001", "speed": 200.0, "Space_abs": [23.1, 12.8, -30, 0] }
    },
    "relation": "移动",
    "direction": "outgoing",
    "name": "反潜巡逻推演",
    "source_context": {
      "outgoing_relations": [
        {"relation": "移动", "count": 5, "example_targets": ["PATROL_A", "PATROL_B"]},
        {"relation": "侦察", "count": 3, "example_targets": ["AREA_X"]}
      ],
      "incoming_relations": []
    },
    "chain": [{
      "hop": 1,
      "direction": "outgoing",
      "source_id": "P8A_001",
      "target_id": "PATROL_2023_A_SECTOR",
      "rel_type": "移动",
      "target": {
        "id": "PATROL_2023_A_SECTOR",
        "name": "A海域例行反潜巡航",
        "labels": ["Patrol"],
        "properties": {}
      },
      "type_hierarchy": ["Patrol"],
      "inferred": {
        "state_changes": [
          "📍 位置移动: (23.1291, 12.8000) → (32.0580, 118.7960) 距离=11143.21 km",
          "🧭 实体 P8A_001 沿移动关系到达节点 PATROL_2023_A_SECTOR",
          "📌 目标前置条件: :Entity(P8A_001) ^ 移动(P8A_001, PATROL_...) ^ hasSpeed(P8A_001, 200)..."
        ],
        "matching_rules": [
          {"rule_name": "巡逻任务分解", "source_file": "patrol_rules", "match_type": "keyword"}
        ],
        "next_relations": [
          {"relation": "composedOf", "count": 3, "example_targets": ["HOVER_1", "SCAN_2"]},
          {"relation": "打击", "count": 1, "example_targets": ["STRIKE_X"]}
        ]
      }
    }],
    "stats": {
      "hops": 3,
      "nodes_visited": 3,
      "state_changes": 12,
      "matching_rules": 2
    },
    "summary": "反潜巡逻推演: 实体 'P8A_001' 沿 '移动' 关系 outgoing 方向遍历 3 跳, 访问 3 个新节点, 发现 12 条状态变化, 2 条匹配规则。实体现有 5 种出向关系、2 种入向关系。"
  }]
}

// 实体未找到
{"ok": true, "count": 1, "results": [{"error": "实体 'UNKNOWN' 未找到", "id": "UNKNOWN"}]}

// 缺少必填字段
{"ok": true, "count": 1, "results": [{"error": "Missing required field: id"}]}
```

---

## POST /ontology/create

```
作用：LLM 创建本体节点 — 支持 Entity/Type/Patrol 类型，单条或批量
方法：POST
路径：/ontology/create
Content-Type: application/json
```

```json
// 请求 — 创建单个 Entity
{
  "type": "Entity",
  "properties": { "code": "NEW_001", "name": "测试实体", "type": "航空母舰" }
}

// 请求 — 批量创建
[{
  "type": "Entity",
  "properties": { "code": "E1", "name": "实体1" }
}, {
  "type": "Type",
  "properties": { "name": "新类型", "parent": "舰船" }
}]

// 响应 201
{ "ok": true, "created": 2, "iris": ["NEW_001", "新类型"] }
```

---

## POST /relationships/create

```
作用：LLM 创建关系 — 自动解析起始/目标节点，支持多种查找方式
方法：POST
路径：/relationships/create
Content-Type: application/json
```

```json
// 请求
{
  "start": { "code": "P8A_001" },
  "rel_type": "移动",
  "end": { "code": "PATROL_2023" }
}

// 响应 201
{ "ok": true }
```

---

## POST /confidence/policy

```
作用：切换推理置信度策略模式
方法：POST
路径：/confidence/policy
Content-Type: application/json
```

```json
// 请求
{ "mode": "Permissive" }

// 响应 200
{ "ok": true, "mode": "Permissive", "threshold": 0.15 }
```

---

## GET|POST /rules

```
作用：GET 列出已加载的 SWRL 规则；POST 从 Neo4j (:Rule) 节点或 rules/*.swrl 文件热加载规则
方法：GET / POST
路径：/rules
```

```json
// GET /rules → 响应 200
{
  "count": 2,
  "rules": [
    {"name": "uncleRule", "antecedent_count": 2, "consequent_count": 1, "is_safe": true}
  ]
}

// POST /rules → 响应 200
{ "ok": true, "loaded": 3, "total_rules": 5 }
```

---

## 错误响应

```json
// 400 / 404 / 405 / 500
{ "error": "错误描述" }

// 422（仅 /reason 置信度熔断）
{ "error": "confidence_fuse_tripped", "confidence": 0.25, "threshold": 0.3, "rule_name": "..." }
```
