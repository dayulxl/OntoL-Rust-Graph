//! Haversine 公式 — 球面距离计算。

/// 地球半径（米）
pub const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// 计算两经纬度点之间的地表距离（米）
///
/// # 示例
///
/// ```rust
/// use ontology_reasoner::spatial::haversine_m;
/// let dist = haversine_m(39.9042, 116.4074, 31.2304, 121.4737);
/// assert!((dist / 1000.0) > 1000.0 && (dist / 1000.0) < 1200.0); // Beijing → Shanghai ≈ 1060 km
/// ```
pub fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    EARTH_RADIUS_M * c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_haversine() {
        let dist = haversine_m(39.9042, 116.4074, 31.2304, 121.4737);
        let km = dist / 1000.0;
        assert!(km > 1000.0 && km < 1200.0, "got {:.0} km", km);
    }

    #[test]
    fn test_zero_distance() {
        let dist = haversine_m(0.0, 0.0, 0.0, 0.0);
        assert!((dist - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_antipodal() {
        // antipodal points should be approximately pi * earth_radius
        let dist = haversine_m(0.0, 0.0, 0.0, 180.0);
        let expected = std::f64::consts::PI * EARTH_RADIUS_M;
        assert!((dist - expected).abs() < 1.0);
    }
}
