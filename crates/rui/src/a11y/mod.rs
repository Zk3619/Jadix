//! Accessibility layer: AccessKit tree bridge (Element → `TreeUpdate`) and action dispatch.
//!
//! Runtime wiring (`accesskit_winit::Adapter`) stays in [`crate::app::lifecycle`]; this
//! module owns the pure tree/action mapping.
//!
//! **Boundary (WP-1.3 vs identity engineering):**
//! - **In scope here:** Role/name/value/Action mapping; `Window`→`Document` content tree;
//!   empty/`template_id` collision defense so TreeUpdate never lists duplicate child `NodeId`s.
//! - **Out of scope:** stopping components/macros from writing `Some("")` into
//!   `focus_id`/`template_id`, reducing forced user `id`, scoped Owner keys — see
//!   `docs/element_identity.md` and C-WP-V.5 / C-WP-2.6 / WP-2.4.
//!
//! Static Labels are **not** in Tab order; Narrator must use item navigation (see `a11y_smoke.md`).

pub mod bridge;

pub use bridge::{dispatch_accesskit_action, tree_update_from_elements};
