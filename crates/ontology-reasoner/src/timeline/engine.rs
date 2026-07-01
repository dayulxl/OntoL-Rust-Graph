//! 时序推演引擎。
//!
//! 日志写入 `logs/` 目录下的文件，同时收集到 `logs` 字段供 HTTP 响应。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::timeline::model::{Segment, StrikeInput, StrikeResult, TimelineInput, TimelineResult};
use crate::spatial::haversine_m;

/// 时序推演引擎。
pub struct TimelineEngine;

impl TimelineEngine {
    pub fn new() -> Self { Self }

    /// 执行时序推演。日志写入文件 `logs/patrol_{code}_{ts}.log`，
    /// 同时收集到返回结果的 `logs` 字段。
    pub fn simulate(&self, input: &TimelineInput) -> TimelineResult {
        let TimelineInput {
            patrol_code, patrol_name, patrol_id: _,
            waypoints, entity_code,
            start_lat, start_lon, start_alt, speed,
        } = input;

        let wps = waypoints;
        let mut log_lines: Vec<String> = Vec::new();

        // 日志文件
        let log_file = create_log_file(patrol_code);
        macro_rules! log_out {
            ($($arg:tt)*) => {
                let msg = format!($($arg)*);
                log::info!("{}", msg);
                write_to_file(&log_file, &msg);
                log_lines.push(msg.clone());
            };
        }

        // ── 头部 ──
        log_out!("══════════════════════════════════");
        log_out!("📋 巡逻任务: {} ({})", patrol_name, patrol_code);
        log_out!("   航点数: {}", wps.len());
        log_out!("   实体: {}  当前位置: lat={:.4} lon={:.4} alt={:.0}m",
            entity_code, start_lat, start_lon, start_alt);
        log_out!("   速度: {:.1} m/s", speed);

        // ── 航线计算 ──
        log_out!("   ──── 航线计算 ────");
        let mut prev_lat = *start_lat;
        let mut prev_lon = *start_lon;
        let mut total_distance_m = 0.0;
        let mut total_time_s = 0.0;
        let mut segments: Vec<Segment> = Vec::new();

        for wp in wps {
            let dist_m = haversine_m(prev_lat, prev_lon, wp.lat, wp.lon);
            let time_s = if *speed > 0.0 { dist_m / speed } else { 0.0 };
            total_distance_m += dist_m;
            total_time_s     += time_s;

            log_out!("   [{:2}] ({} {:.4}) → ({} {:.4})  距离={:.0}m  时间={:.1}s  动作={}",
                wp.seq, prev_lat, prev_lon, wp.lat, wp.lon, dist_m, time_s, wp.action);

            segments.push(Segment {
                seq: wp.seq, lat: wp.lat, lon: wp.lon, alt: wp.alt,
                action: wp.action.clone(), dist_m, time_s,
            });
            prev_lat = wp.lat;
            prev_lon = wp.lon;
        }

        let total_dur = (total_time_s as i64).max(1);
        let total_km  = total_distance_m / 1000.0;
        log_out!("   📐 总航程: {:.2} km  总耗时: {}s ({:.1} min)",
            total_km, total_dur, total_dur as f64 / 60.0);

        // ── SWRL 时序字段 ──
        let precondition = format!(
            ":Entity({}) ^ 移动({}, {}) ^ hasSpeed({}, {:.0}) ^ hasStatus({}, '有效')",
            entity_code, entity_code, patrol_code, entity_code, speed, entity_code);
        let effect = format!(
            ":Patrol({}) ^ hasStatus({}, '已执行') ^ hasCovered({}, true) ^ hasDuration({}, {})",
            patrol_code, patrol_code, patrol_code, patrol_code, total_dur);
        let desc: Vec<String> = segments.iter()
            .map(|s| format!("{}(第{}航点,{:.0}m,{:.1}s)", s.action, s.seq, s.dist_m, s.time_s))
            .collect();
        let composed_of = desc.join(" ^ ");
        let cost = format!("{:.0}m, {}s", total_distance_m, total_dur);

        log_out!("   ✅ 巡逻节点写入: {}", patrol_code);

        // ── 时序推演 ──
        log_out!("── 时序推演: {} 步 ──", segments.len());
        let mut remaining_s = total_time_s;
        for (i, seg) in segments.iter().enumerate() {
            let bar: String = (0..segments.len())
                .map(|j| if j <= i { '█' } else { '░' }).collect();
            remaining_s -= seg.time_s;

            log_out!("   [{}/{}] {} {:5} → lat={:.4} lon={:.4} alt={:.0}  航段{:.0}m {:.1}s  剩余{:.0}s",
                seg.seq, segments.len(), bar, seg.action,
                seg.lat, seg.lon, seg.alt,
                seg.dist_m, seg.time_s, remaining_s.max(0.0));
        }

        log_out!("── 推演完成 ──");
        log_out!("   日志文件: {}", log_file.display());

        TimelineResult {
            patrol_code: patrol_code.clone(), patrol_name: patrol_name.clone(),
            entity_code: entity_code.clone(), segments,
            total_distance_m, total_distance_km: format!("{:.2}", total_km),
            total_time_s, duration: total_dur, precondition, effect,
            composed_of, cost, logs: log_lines,
        }
    }
}

impl Default for TimelineEngine { fn default() -> Self { Self::new() } }

impl TimelineEngine {
    /// 打击决策推演。
    ///
    /// 计算攻击方到目标的距离 → 射程判定 → 命中概率 → 毁伤评估。
    /// 日志写入 `logs/strike_{code}_{ts}.log`。
    pub fn simulate_strike(&self, input: &StrikeInput) -> StrikeResult {
        let StrikeInput {
            strike_code, strike_name, strike_id: _,
            attacker_code, attacker_lat, attacker_lon, attacker_alt: _,
            target_code, target_lat, target_lon, target_depth,
            weapon_type, weapon_range_m, confidence,
        } = input;

        let mut log_lines: Vec<String> = Vec::new();

        let log_file = create_strike_log_file(strike_code);
        macro_rules! log_out {
            ($($arg:tt)*) => {
                let msg = format!($($arg)*);
                log::info!("{}", msg);
                write_to_file(&log_file, &msg);
                log_lines.push(msg.clone());
            };
        }

        log_out!("══════════════════════════════════");
        log_out!("🎯 打击决策推演: {} ({})", strike_name, strike_code);
        log_out!("   攻击方: {}  lat={:.4} lon={:.4}", attacker_code, attacker_lat, attacker_lon);
        log_out!("   目标:   {}  lat={:.4} lon={:.4} depth={:.0}m", target_code, target_lat, target_lon, target_depth);
        log_out!("   武器: {}  射程={:.0}m  置信度={:.2}", weapon_type, weapon_range_m, confidence);

        // ── 距离计算 ──
        let distance_m = haversine_m(*attacker_lat, *attacker_lon, *target_lat, *target_lon);
        let in_range = distance_m <= *weapon_range_m;

        log_out!("   ──── 射程判定 ────");
        log_out!("   距离: {:.0}m ({:.2} km)  [{}射程]",
            distance_m, distance_m / 1000.0,
            if in_range { "✅ 进入" } else { "⛔ 超出" });

        // ── 命中概率 ──
        // 公式: P_hit = confidence × (1 - distance/range)  when in_range, else 0
        let base_prob = if in_range && *weapon_range_m > 0.0 {
            confidence * (1.0 - distance_m / weapon_range_m).max(0.0)
        } else {
            0.0
        };
        // 加入深度惩罚因子 (潜艇越深越难命中)
        let depth_factor = if *target_depth > 0.0 {
            (1.0 - (*target_depth / 1000.0).min(0.9)).max(0.1)
        } else {
            1.0
        };
        let hit_probability = (base_prob * depth_factor).min(1.0).max(0.0);

        // ── 毁伤评估 ──
        let damage_level = match hit_probability {
            p if p >= 0.8 => "destroyed",
            p if p >= 0.6 => "heavy",
            p if p >= 0.3 => "moderate",
            p if p > 0.0  => "light",
            _ => "none",
        };

        log_out!("   ──── 命中分析 ────");
        log_out!("   基础命中率: {:.1}%", base_prob * 100.0);
        log_out!("   深度惩罚:   ×{:.2} (depth={:.0}m)", depth_factor, target_depth);
        log_out!("   最终命中率: {:.1}%", hit_probability * 100.0);
        log_out!("   毁伤等级:   {}", damage_level);

        // ── 武器飞行/航行时间 ──
        let weapon_speed = weapon_speed_estimate(weapon_type);
        let total_time_s = if weapon_speed > 0.0 { distance_m / weapon_speed } else { 0.0 };
        log_out!("   武器速度: {:.0} m/s  飞行时间: {:.1}s", weapon_speed, total_time_s);
        let duration = (total_time_s as i64).max(1);

        // ── SWRL 字段 ──
        let precondition = format!(
            ":Entity({}) ^ :Entity({}) ^ 打击({}, {}, {}) ^ hasWeapon({}, '{}') ^ hasRange({}, {:.0}) ^ distance({}, {}, {:.0})",
            attacker_code, target_code, attacker_code, target_code, strike_code,
            attacker_code, weapon_type, weapon_type, weapon_range_m,
            attacker_code, target_code, distance_m);
        let effect = format!(
            ":Strike({}) ^ hasStatus({}, '{}') ^ hasHitProbability({}, {:.2}) ^ hasDamage({}, '{}')",
            strike_code, strike_code,
            if hit_probability > 0.0 { "已执行" } else { "超出射程" },
            strike_code, hit_probability, strike_code, damage_level);
        let composed_of = format!(
            "锁定({}→{},{:.0}m) ^ 发射({},{},{:.1}s) ^ 命中({},{:.1}%) ^ 毁伤({},{})",
            attacker_code, target_code, distance_m,
            weapon_type, strike_code, total_time_s,
            target_code, hit_probability * 100.0,
            target_code, damage_level);
        let cost = format!("{:.0}m, {:.0}s, {}发", distance_m, total_time_s, if hit_probability > 0.3 { 1u8 } else { 2u8 });

        log_out!("   ✅ 打击推演完成: {}", strike_code);

        StrikeResult {
            strike_code: strike_code.clone(),
            strike_name: strike_name.clone(),
            attacker_code: attacker_code.clone(),
            target_code: target_code.clone(),
            weapon_type: weapon_type.clone(),
            distance_m,
            in_range,
            hit_probability,
            damage_level: damage_level.to_string(),
            weapon_range_m: *weapon_range_m,
            total_time_s,
            duration,
            precondition,
            effect,
            composed_of,
            cost,
            logs: log_lines,
        }
    }
}

// ── 日志文件 ──

fn create_strike_log_file(code: &str) -> PathBuf {
    let dir = PathBuf::from("logs");
    let _ = fs::create_dir_all(&dir);
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    dir.join(format!("strike_{}_{}.log", code, ts))
}

/// 根据武器类型估算速度 (m/s)
fn weapon_speed_estimate(weapon_type: &str) -> f64 {
    let lower = weapon_type.to_lowercase();
    if lower.contains("鱼雷") || lower.contains("torpedo") {
        40.0   // 鱼雷 ~40 m/s (~78 knots)
    } else if lower.contains("导弹") || lower.contains("missile") {
        300.0  // 反舰导弹 ~300 m/s (亚音速)
    } else if lower.contains("超音速") || lower.contains("supersonic") {
        680.0  // 超音速导弹
    } else if lower.contains("深弹") || lower.contains("depth charge") {
        10.0   // 深水炸弹下沉
    } else if lower.contains("火炮") || lower.contains("gun") {
        800.0  // 舰炮 ~800 m/s
    } else {
        200.0  // 默认亚音速
    }
}

fn create_log_file(code: &str) -> PathBuf {
    let dir = PathBuf::from("logs");
    let _ = fs::create_dir_all(&dir);
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    dir.join(format!("patrol_{}_{}.log", code, ts))
}

fn write_to_file(path: &PathBuf, msg: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{}", msg);
        let _ = f.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::model::WaypointInput;

    #[test] fn test_simulate() {
        let engine = TimelineEngine::new();
        let input = TimelineInput {
            patrol_code: "TEST".into(), patrol_name: "Test".into(), patrol_id: "u".into(),
            waypoints: vec![
                WaypointInput { seq: 1, lat: 32.0, lon: 118.0, alt: 500.0, action: "HOVER".into() },
                WaypointInput { seq: 2, lat: 32.1, lon: 118.1, alt: 500.0, action: "SCAN".into() },
            ],
            entity_code: "P8A_001".into(),
            start_lat: 31.0, start_lon: 117.0, start_alt: 0.0, speed: 200.0,
        };
        let r = engine.simulate(&input);
        assert_eq!(r.segments.len(), 2);
        assert!(r.logs.len() > 5);
    }
}
