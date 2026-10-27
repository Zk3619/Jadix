//! Serialize sampled polylines and canonical cubics.

use super::types::CubicPath;

fn fmt2(v: f64) -> String {
    format!("{}", (v * 100.0).round() / 100.0)
}

fn fmt4(v: f64) -> String {
    format!("{}", (v * 1e4).round() / 1e4)
}

/// Sampled subpaths → polyline `d`. `closed[k]` appends Z.
pub fn serialize(subs: &[Vec<f64>], closed: &[bool]) -> String {
    let mut d = String::new();
    for (k, o) in subs.iter().enumerate() {
        let n = o.len() / 2;
        if n == 0 {
            continue;
        }
        d.push('M');
        d.push_str(&fmt2(o[0]));
        d.push(' ');
        d.push_str(&fmt2(o[1]));
        for i in 1..n {
            d.push('L');
            d.push_str(&fmt2(o[2 * i]));
            d.push(' ');
            d.push_str(&fmt2(o[2 * i + 1]));
        }
        if closed.get(k).copied().unwrap_or(false) {
            d.push('Z');
        }
    }
    d
}

/// Cubic subpaths → canonical `d`, quantized to 4 decimals.
pub fn cubics_to_path_d(paths: &[CubicPath]) -> String {
    let mut d = String::new();
    for path in paths {
        let pts = &path.pts;
        if pts.len() < 2 {
            continue;
        }
        d.push('M');
        d.push_str(&fmt4(pts[0]));
        d.push(' ');
        d.push_str(&fmt4(pts[1]));
        let mut i = 2;
        while i + 5 < pts.len() {
            d.push('C');
            d.push_str(&fmt4(pts[i]));
            d.push(' ');
            d.push_str(&fmt4(pts[i + 1]));
            d.push(' ');
            d.push_str(&fmt4(pts[i + 2]));
            d.push(' ');
            d.push_str(&fmt4(pts[i + 3]));
            d.push(' ');
            d.push_str(&fmt4(pts[i + 4]));
            d.push(' ');
            d.push_str(&fmt4(pts[i + 5]));
            i += 6;
        }
        if path.closed {
            d.push('Z');
        }
    }
    d
}
