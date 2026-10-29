//! CSS easing: `cubic-bezier` helpers plus `steps()` / `jump-*` / `linear()`.
//!
//! [`TimingFunction::eval`] lives on the enum (element_style) so existing call sites
//! stay put; step math is implemented here (ANIM-1.9). `linear()` stops are interned
//! so [`TimingFunction`] stays `Copy`.

use std::cell::RefCell;
use std::sync::Arc;

use super::element_style::{StepJumpTerm, TimingFunction};

type LinearStops = Arc<[(f32, f32)]>;

thread_local! {
    static LINEAR_STOPS: RefCell<Vec<LinearStops>> = const { RefCell::new(Vec::new()) };
}

/// CSS Easing 1 `steps(n, jump-*)` at local progress `t` ∈ [0, 1].
pub fn eval_steps(t: f32, n: u32, jump: StepJumpTerm) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let n = n.max(1);
    if t >= 1.0 {
        return 1.0;
    }
    match jump {
        StepJumpTerm::JumpStart => (((t * n as f32).floor() + 1.0) / n as f32).min(1.0),
        StepJumpTerm::JumpEnd => (t * n as f32).floor() / n as f32,
        StepJumpTerm::JumpNone => {
            if n == 1 {
                0.0
            } else {
                (t * n as f32).floor().min((n - 1) as f32) / (n - 1) as f32
            }
        }
        StepJumpTerm::JumpBoth => (((t * (n + 1) as f32).floor() + 1.0) / (n + 1) as f32).min(1.0),
    }
}

/// Parse `steps(4)`, `steps(4, jump-start)`, `step-start`, `step-end`.
pub fn parse_steps_timing(s: &str) -> Option<TimingFunction> {
    let s = s.trim().to_ascii_lowercase();
    match s.as_str() {
        "step-start" => {
            return Some(TimingFunction::Steps {
                n: 1,
                jump: StepJumpTerm::JumpStart,
            });
        }
        "step-end" => {
            return Some(TimingFunction::Steps {
                n: 1,
                jump: StepJumpTerm::JumpEnd,
            });
        }
        _ => {}
    }
    let inner = s.strip_prefix("steps(")?.strip_suffix(')')?;
    let mut parts = inner.split(',').map(|p| p.trim());
    let n: u32 = parts.next()?.parse().ok()?;
    if n == 0 {
        return None;
    }
    let jump = match parts.next().unwrap_or("jump-end") {
        "start" | "jump-start" => StepJumpTerm::JumpStart,
        "end" | "jump-end" => StepJumpTerm::JumpEnd,
        "jump-none" => StepJumpTerm::JumpNone,
        "jump-both" => StepJumpTerm::JumpBoth,
        _ => return None,
    };
    Some(TimingFunction::Steps { n, jump })
}

/// Intern CSS `linear()` stops as a `Copy` handle.
pub fn intern_linear_stops(mut stops: Vec<(f32, f32)>) -> TimingFunction {
    if stops.is_empty() {
        return TimingFunction::Linear;
    }
    stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    for s in &mut stops {
        s.0 = s.0.clamp(0.0, 1.0);
    }
    LINEAR_STOPS.with(|tbl| {
        let mut g = tbl.borrow_mut();
        for (i, existing) in g.iter().enumerate() {
            if existing.as_ref() == stops.as_slice() {
                return TimingFunction::LinearStops(i as u16);
            }
        }
        let id = g.len() as u16;
        g.push(Arc::from(stops));
        TimingFunction::LinearStops(id)
    })
}

/// Parse `linear(0, 1)` / `linear(0, 0.25 75%, 1)`. Keyword `linear` stays [`TimingFunction::Linear`].
pub fn parse_linear_timing(s: &str) -> Option<TimingFunction> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("linear") {
        return None;
    }
    let inner = strip_linear_inner(s)?;
    let pieces = split_comma_list(inner);
    if pieces.is_empty() {
        return None;
    }
    let mut parsed: Vec<(Option<f32>, f32)> = Vec::new();
    for p in pieces {
        let toks: Vec<&str> = p.split_whitespace().collect();
        if toks.is_empty() {
            return None;
        }
        let y: f32 = toks[0].parse().ok()?;
        let x = if toks.len() >= 2 {
            Some(parse_progress(toks[1])?)
        } else {
            None
        };
        parsed.push((x, y));
    }
    let n = parsed.len();
    let mut stops = Vec::with_capacity(n);
    for (i, (x, y)) in parsed.into_iter().enumerate() {
        let input = x.unwrap_or(if n == 1 {
            0.0
        } else {
            i as f32 / (n - 1) as f32
        });
        stops.push((input, y));
    }
    Some(intern_linear_stops(stops))
}

fn strip_linear_inner(s: &str) -> Option<&str> {
    let s = s.trim();
    if s.len() < 8 {
        return None;
    }
    if !s.get(..6)?.eq_ignore_ascii_case("linear") {
        return None;
    }
    s[6..]
        .trim_start()
        .strip_prefix('(')?
        .strip_suffix(')')
        .map(str::trim)
}

fn parse_progress(tok: &str) -> Option<f32> {
    let t = tok.trim();
    if let Some(rest) = t.strip_suffix('%') {
        return rest
            .parse::<f32>()
            .ok()
            .map(|v| (v / 100.0).clamp(0.0, 1.0));
    }
    t.parse::<f32>().ok().map(|v| v.clamp(0.0, 1.0))
}

/// Evaluate interned `linear()` stops. Missing id falls back to identity.
pub fn eval_linear_stops(id: u16, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    LINEAR_STOPS.with(|tbl| {
        let g = tbl.borrow();
        let Some(stops) = g.get(id as usize) else {
            return t;
        };
        if stops.is_empty() {
            return t;
        }
        if t <= stops[0].0 {
            return stops[0].1;
        }
        for w in stops.windows(2) {
            if t <= w[1].0 {
                let span = (w[1].0 - w[0].0).max(1e-6);
                let u = (t - w[0].0) / span;
                return w[0].1 + (w[1].1 - w[0].1) * u;
            }
        }
        stops.last().map(|s| s.1).unwrap_or(t)
    })
}

/// Split a CSS list on commas that are **not** inside parentheses (`animation` / `transition`).
pub fn split_comma_list(s: &str) -> Vec<&str> {
    split_top_level(s, ',')
}

/// Split on whitespace that is not inside parentheses.
pub fn split_ws_tokens(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = None;
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ if c.is_whitespace() && depth == 0 => {
                if let Some(st) = start.take() {
                    out.push(s[st..i].trim());
                }
            }
            _ => {
                if start.is_none() && !c.is_whitespace() {
                    start = Some(i);
                }
            }
        }
    }
    if let Some(st) = start {
        out.push(s[st..].trim());
    }
    out.retain(|t| !t.is_empty());
    out
}

fn split_top_level(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if c == sep && depth == 0 => {
                let piece = s[start..i].trim();
                if !piece.is_empty() {
                    out.push(piece);
                }
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    let piece = s[start..].trim();
    if !piece.is_empty() {
        out.push(piece);
    }
    out
}

#[cfg(test)]
#[path = "tests/easing.rs"]
mod tests;
