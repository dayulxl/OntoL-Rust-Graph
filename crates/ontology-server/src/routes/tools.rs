//! GET /tools — LLM Function Calling 工具定义（ASW 领域）。

pub fn handle() -> (u16, String) {
    let tools = serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": "search_entities",
                "description": "搜索 Entity 节点。可按 code 精确查找、按 type 分类查找、按 command_side 红蓝方过滤、按空间范围筛选。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "code":  { "type": "string", "description": "精确匹配 Entity.code" },
                        "type":  { "type": "string", "description": "按 type 字段过滤（如 '尼米兹级'、'P-8A海神巡逻机'）" },
                        "command_side": { "type": "integer", "description": "0红方/1蓝方/2中立/3不确定" },
                        "subclass_of": { "type": "string", "description": "通过 subClassOf 层级查找：匹配 type 属于此分类的所有 Entity" },
                        "keyword": { "type": "string", "description": "在 name/description 中模糊搜索" }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "get_entity_context",
                "description": "获取 Entity 的图邻域上下文：子类层级（subClassOf）、关联实体（移动关系）、空间位置、行为字段。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "code": { "type": "string", "description": "Entity.code" },
                        "depth": { "type": "integer", "description": "关系遍历深度 1-3，默认 2" }
                    },
                    "required": ["code"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "load_swrl_rule",
                "description": "加载 SWRL 推理规则。规则语法：[ruleName: atom ^ atom ^ ... -> consequent]。用于编写 Entity 间的推理逻辑（如行为链、分类传递）。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "rule_text": { "type": "string", "description": "SWRL 规则字符串" }
                    },
                    "required": ["rule_text"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "execute_reasoning",
                "description": "对已加载的 SWRL 规则执行 fixpoint 推理循环。置信度 < 0.3 时自动熔断。返回推理报告。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "incremental": { "type": "boolean", "description": "是否增量推理" }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "update_entity",
                "description": "更新 Entity 的字段：位置、状态、command_side、时序字段等。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "code": { "type": "string", "description": "Entity.code" },
                        "Space_abs": { "type": "array", "description": "[纬度, 经度, 深度, 高度]" },
                        "duration": { "type": "integer", "description": "持续时间（秒）" },
                        "status": { "type": "string", "description": "有效/无效" },
                        "command_side": { "type": "integer", "description": "红蓝方" }
                    },
                    "required": ["code"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "create_entity",
                "description": "创建新的 Entity 本体节点。包含基础字段、行为字段、空间字段和边属性共 30 个标准字段。code 必须唯一。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "code": { "type": "string", "description": "业务编码，唯一标识" },
                        "name": { "type": "string", "description": "名称" },
                        "type": { "type": "string", "description": "本体类型：M1对象/M2行为/M3规则/M4场景/M5主体/M6异常补偿/M7质量约束/ME事件" },
                        "domain": { "type": "string", "description": "领域ID" },
                        "leven": { "type": "integer", "description": "本体层级 L0-L5" },
                        "description": { "type": "string", "description": "描述（最长 500）" },
                        "status": { "type": "string", "description": "有效/无效" },
                        "command_side": { "type": "integer", "description": "0红方/1蓝方/2中立/3不确定" },
                        "confidence": { "type": "number", "description": "置信度百分数" },
                        "speed": { "type": "number", "description": "速度 m/s" },
                        "power": { "type": "number", "description": "电量" },
                        "duration": { "type": "integer", "description": "持续时间（秒）" },
                        "priority": { "type": "integer", "description": "行为等级" },
                        "Space_abs": { "type": "array", "description": "[纬度, 经度, 深度, 高度]" },
                        "precondition": { "type": "string", "description": "前置条件/约束（SWRL语法）" },
                        "effect": { "type": "string", "description": "执行效果（SWRL语法）" },
                        "cost": { "type": "string", "description": "资源消耗（SWRL语法）" },
                        "parent_id": { "type": "string", "description": "父本体ID" },
                        "source": { "type": "string", "description": "来源" },
                        "owner": { "type": "string", "description": "维护人员" }
                    },
                    "required": ["code", "name", "type"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "create_type",
                "description": "创建分类层级节点 Type，可选通过 parent_type 建立 subClassOf 关系。用于构建本体分类树。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Type 名称" },
                        "parent_type": { "type": "string", "description": "父 Type 名称，设置后自动创建 subClassOf 关系" },
                        "description": { "type": "string", "description": "分类描述" },
                        "domain": { "type": "string", "description": "领域ID" },
                        "leven": { "type": "integer", "description": "层级" }
                    },
                    "required": ["name"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "create_patrol",
                "description": "创建巡逻任务 Patrol 节点，可关联多个 Entity 并指定巡逻路径。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "巡逻任务名称" },
                        "code": { "type": "string", "description": "巡逻任务编码" },
                        "description": { "type": "string", "description": "任务描述" },
                        "status": { "type": "string", "description": "任务状态" },
                        "domain": { "type": "string", "description": "领域ID" },
                        "command_side": { "type": "integer", "description": "0红方/1蓝方/2中立" },
                        "duration": { "type": "integer", "description": "巡逻持续时间（秒）" },
                        "path": { "type": "array", "description": "巡逻路径点数组 [[lat,lon],...]" },
                        "entities": { "type": "array", "description": "关联的 Entity code 列表" }
                    },
                    "required": ["name"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "simulate_patrol",
                "description": "执行巡逻任务时序推演。根据航点列表、实体速度和位置，计算航线距离、时间，逐航段模拟巡逻过程。结果自动写入 Patrol 节点。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "code": { "type": "string", "description": "巡逻任务编码" },
                        "name": { "type": "string", "description": "巡逻任务名称" },
                        "id": { "type": "string", "description": "任务 UUID" },
                        "waypoints": { "type": "array", "description": "航点列表 [{seq, lat, lon, alt, action}]" },
                        "attacker": { "type": "string", "description": "执行巡逻的 Entity code（可选，自动匹配）" }
                    },
                    "required": ["code", "name"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "simulate_strike",
                "description": "执行打击决策推演。输入攻击方、目标坐标和武器参数，计算射程判定、命中概率、毁伤评估。结果自动写入 Strike 节点。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "code": { "type": "string", "description": "打击任务编码" },
                        "name": { "type": "string", "description": "打击任务名称" },
                        "attacker": { "type": "string", "description": "攻击方 Entity code（如 P8A_001）" },
                        "attacker_lat": { "type": "number", "description": "攻击方纬度（code 不可用时直接指定）" },
                        "attacker_lon": { "type": "number", "description": "攻击方经度" },
                        "attacker_alt": { "type": "number", "description": "攻击方高度（m）" },
                        "target": { "type": "string", "description": "目标 Entity code" },
                        "target_lat": { "type": "number", "description": "目标纬度（code 不可用时直接指定）" },
                        "target_lon": { "type": "number", "description": "目标经度" },
                        "target_depth": { "type": "number", "description": "目标深度（m，潜艇深度）" },
                        "weapon_type": { "type": "string", "description": "武器类型：鱼雷/导弹/深弹/火炮" },
                        "weapon_range_m": { "type": "number", "description": "武器最大射程（m），默认 10000" },
                        "confidence": { "type": "number", "description": "置信度 0.0-1.0，影响命中概率" },
                        "precondition": { "type": "string", "description": "SWRL 前置条件" }
                    },
                    "required": ["code", "name", "attacker", "target", "weapon_type"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "infer_forward",
                "description": "向前推理：给定实体ID和关系名，沿图自动多跳遍历，检测每跳的状态变化（位置/速度/状态/功率/置信度），匹配SWRL规则，预测下一步操作。无需手动指定路径，系统全自动推导。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "起始实体 code" },
                        "name": { "type": "string", "description": "推理任务名称" },
                        "relation": { "type": "string", "description": "要沿哪个关系向前推导（如 移动/打击/subClassOf/composedOf）" },
                        "depth": { "type": "integer", "description": "最大遍历深度 1-5，默认 3" },
                        "direction": { "type": "string", "description": "遍历方向：outgoing(默认)/incoming/both" },
                        "confidence_threshold": { "type": "number", "description": "全局置信度阈值 0.0-1.0，低于此值的跳标记为低置信度" }
                    },
                    "required": ["id", "name", "relation"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "create_relationship",
                "description": "在两个节点之间创建关系。支持移动、subClassOf 等关系类型。source/target 通过 code 或 node_id 指定。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "rel_type": { "type": "string", "description": "关系类型，如 移动/subClassOf/HAS_VALUE/INSTANCE_OF/contains 等" },
                        "source_code": { "type": "string", "description": "源节点 code（优先使用）" },
                        "source_id": { "type": "string", "description": "源节点 ID（code 不可用时使用）" },
                        "target_code": { "type": "string", "description": "目标节点 code（优先使用）" },
                        "target_id": { "type": "string", "description": "目标节点 ID（code 不可用时使用）" },
                        "properties": { "type": "object", "description": "关系上的附加属性，键值对" }
                    },
                    "required": ["rel_type"]
                }
            }
        }
    ]);

    (200, tools.to_string())
}
