pub(crate) fn equal_f64(a: f64, b: f64, epsilon: f64) -> bool {
    (a - b).abs() < epsilon
}

/// Compares two floats allowing a relative error
///
/// `tolerance` is a fraction of the largest of the two values, so `0.05`
/// allows a 5% error.
pub(crate) fn equal_f64_relative(a: f64, b: f64, tolerance: f64) -> bool {
    // covers equal values, zeros and infinities
    if a == b {
        return true;
    }
    (a - b).abs() <= tolerance * a.abs().max(b.abs())
}

pub(crate) fn round_f64(val: f64, precision: u32) -> f64 {
    let factor = (10.0 as f64).powi(precision as i32);
    (val * factor).round() / factor
}

pub(crate) fn round_f64_with_tolerance(val: f64, precision: u32, tolerance: f64) -> f64 {
    let new_val = round_f64(val, precision);
    let nearest_int = val.round();

    if equal_f64(nearest_int, new_val, tolerance) {
        nearest_int
    } else {
        new_val
    }
}
