//! 时序推演数据模型。

/// 航点输入（从 HTTP 请求解析）
#[derive(Debug, Clone)]
pub struct WaypointInput {
    pub seq: i64,
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    pub action: String,
}

/// 巡逻任务推演输入
#[derive(Debug, Clone)]
pub struct TimelineInput {
    /// 巡逻任务 code
    pub patrol_code: String,
    /// 巡逻任务名
    pub patrol_name: String,
    /// 巡逻任务 UUID
    pub patrol_id: String,
    /// 航点列表
    pub waypoints: Vec<WaypointInput>,
    /// 实体 code（如 P8A_001）
    pub entity_code: String,
    /// 实体当前纬度
    pub start_lat: f64,
    /// 实体当前经度
    pub start_lon: f64,
    /// 实体当前高度
    pub start_alt: f64,
    /// 实体速度 (m/s)
    pub speed: f64,
}

/// 单个航段（含距离和时间计算结果）
#[derive(Debug, Clone)]
pub struct Segment {
    pub seq: i64,
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    pub action: String,
    pub dist_m: f64,
    pub time_s: f64,
}

// ═══════════════════════════════════════════════════════════
// 打击决策模型
// ═══════════════════════════════════════════════════════════

/// 打击决策推演输入
#[derive(Debug, Clone)]
pub struct StrikeInput {
    /// 打击任务 code
    pub strike_code: String,
    /// 打击任务名
    pub strike_name: String,
    /// 任务 UUID
    pub strike_id: String,
    /// 攻击方实体 code
    pub attacker_code: String,
    /// 攻击方纬度
    pub attacker_lat: f64,
    /// 攻击方经度
    pub attacker_lon: f64,
    /// 攻击方高度
    pub attacker_alt: f64,
    /// 目标实体 code
    pub target_code: String,
    /// 目标纬度
    pub target_lat: f64,
    /// 目标经度
    pub target_lon: f64,
    /// 目标深度
    pub target_depth: f64,
    /// 武器类型
    pub weapon_type: String,
    /// 武器最大射程 (m)
    pub weapon_range_m: f64,
    /// 攻击方置信度 (0.0-1.0)
    pub confidence: f64,
}

/// 打击决策推演输出
#[derive(Debug, Clone)]
pub struct StrikeResult {
    pub strike_code: String,
    pub strike_name: String,
    pub attacker_code: String,
    pub target_code: String,
    pub weapon_type: String,
    /// 攻击方→目标 距离 (m)
    pub distance_m: f64,
    /// 是否进入射程
    pub in_range: bool,
    /// 命中概率 (0.0-1.0)
    pub hit_probability: f64,
    /// 预估毁伤等级: none / light / moderate / heavy / destroyed
    pub damage_level: String,
    /// 武器射程 (m)
    pub weapon_range_m: f64,
    /// 总耗时 (s) — 武器飞行/航行时间
    pub total_time_s: f64,
    /// duration (整数秒)
    pub duration: i64,
    /// SWRL precondition
    pub precondition: String,
    /// SWRL effect
    pub effect: String,
    /// 资源消耗
    pub cost: String,
    /// 组合动作
    pub composed_of: String,
    /// 日志行
    pub logs: Vec<String>,
}

/// 时序推演输出
#[derive(Debug, Clone)]
pub struct TimelineResult {
    pub patrol_code: String,
    pub patrol_name: String,
    pub entity_code: String,
    /// 每个航段
    pub segments: Vec<Segment>,
    /// 总距离（米）
    pub total_distance_m: f64,
    /// 总距离（千米）
    pub total_distance_km: String,
    /// 总时间（秒）
    pub total_time_s: f64,
    /// duration（整数秒）
    pub duration: i64,
    /// SWRL precondition
    pub precondition: String,
    /// SWRL effect
    pub effect: String,
    /// SWRL composedOf
    pub composed_of: String,
    /// 资源消耗
    pub cost: String,
    /// 日志行
    pub logs: Vec<String>,
}
