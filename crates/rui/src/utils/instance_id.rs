//! 实例键空串兜底（身份契约 I-1）。
//!
//! 空 / 空白用户 `id` → 会话内递增 `__rui_{kind}_rt_N`；**不**保证跨 rebuild 同一逻辑实例。
//! 需要稳定身份时：显式非空键、受控 Signal、或 Owner 作用域（`use_scoped_signal` / WP-2.4+2.8）。

use std::sync::atomic::{AtomicU64, Ordering};

/// 若 `user_id` 非空则原样返回；否则生成 `__rui_{kind}_rt_{n}`。
pub fn resolve_instance_id(user_id: impl Into<String>, kind: &str) -> String {
    let id = user_id.into();
    if id.trim().is_empty() {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let kind = if kind.trim().is_empty() { "id" } else { kind };
        format!("__rui_{kind}_rt_{n}")
    } else {
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonempty_passthrough() {
        assert_eq!(resolve_instance_id("gal-tabs", "tabs"), "gal-tabs");
    }

    #[test]
    fn empty_gets_kind_prefixed_unique() {
        let a = resolve_instance_id("", "tabs");
        let b = resolve_instance_id("   ", "tabs");
        assert!(a.starts_with("__rui_tabs_rt_"), "{a}");
        assert!(b.starts_with("__rui_tabs_rt_"), "{b}");
        assert_ne!(a, b);
    }
}
