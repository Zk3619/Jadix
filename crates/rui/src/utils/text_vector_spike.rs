//! Spike: glyph outline → [`crate::primitives::path::PathCommand`] → `RUI_GPU_2D`.
//!
//! **Status:** API stub only — not wired into the frame renderer.
//!
//! ## Decision (ADR-style)
//!
//! | Option | When |
//! |--------|------|
//! | **Coverage** (glyphon) | Axis-aligned UI small text |
//! | **Bake** (RGBA + mip) | Sticky follow-anim rotate/spin |
//! | **Msdf** (SDF bake) | Free scale / 3D billboard |
//! | **Vector outline** (this spike) | Deferred: titles that beat SDF at huge scale |
//! | **Vello** | Reference only (`reference/vello-main`); never replaces glyphon for UI text |
//!
//! Promote this spike only after Bake/Msdf stabilize and a Gallery golden shows
//! outline quality clearly better than SDF at ≥4×. See [`docs/drawing.md`](../../../../docs/drawing.md)
//! and [`docs/engine/glyphon_audit.md`](../../../../docs/engine/glyphon_audit.md).

use crate::primitives::path::PathCommand;

/// Placeholder conversion: emits an empty path until outline extraction is hooked to
/// cosmic-text / skrifa. Kept so call sites and tests can target a stable API.
pub fn glyph_outline_to_path_commands(
    _font_data: &[u8],
    _glyph_id: u16,
    _em_size: f32,
) -> Vec<PathCommand> {
    Vec::new()
}

/// Intended next step (not implemented): tessellate [`PathCommand`]s via
/// [`crate::renderer::geometry2d::path_to_instances`] when `RUI_GPU_2D=1`.
pub fn path_commands_ready_for_gpu2d(cmds: &[PathCommand]) -> bool {
    !cmds.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spike_api_returns_empty_until_wired() {
        assert!(glyph_outline_to_path_commands(&[], 0, 16.0).is_empty());
        assert!(!path_commands_ready_for_gpu2d(&[]));
    }
}
