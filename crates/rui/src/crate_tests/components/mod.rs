mod button_bind_text;
mod button_caption;
mod input_adornments;
mod input_caret;
mod input_shape_suffix;
mod textarea_multiline;

#[cfg(test)]
mod glyphon_newline_diag {
    //! cosmic-text only shapes lines that fit in `buffer` height (`shape_until_scroll`).
    //! Textarea must size the buffer to full content height (external scroll model).

    use crate::renderer::font_loader;
    use crate::style::tailwind_default_leading_scale;
    use crate::text::install_shared_font_system;

    #[test]
    fn short_buffer_drops_second_hard_line() {
        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        install_shared_font_system(&mut fs);

        let size = 14.0_f32;
        let lh = size * tailwind_default_leading_scale(size);
        let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
        buffer.set_size(&mut fs, Some(360.0), Some(lh + 4.0));
        buffer.set_text(
            &mut fs,
            "啊\n啊",
            glyphon::Attrs::new().family(glyphon::Family::SansSerif),
            glyphon::Shaping::Advanced,
        );
        buffer.shape_until_scroll(&mut fs, false);
        let n = buffer.layout_runs().count();
        assert_eq!(
            n, 1,
            "regression guard: 1-line buffer must only yield 1 run (got {n})"
        );
    }

    #[test]
    fn content_tall_buffer_keeps_hard_newline_gap() {
        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        install_shared_font_system(&mut fs);

        let size = 14.0_f32;
        let lh = size * tailwind_default_leading_scale(size);
        let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
        buffer.set_size(&mut fs, Some(360.0), Some(lh * 2.0 + 8.0));
        buffer.set_text(
            &mut fs,
            "啊\n啊",
            glyphon::Attrs::new().family(glyphon::Family::SansSerif),
            glyphon::Shaping::Advanced,
        );
        buffer.shape_until_scroll(&mut fs, false);
        let runs: Vec<_> = buffer
            .layout_runs()
            .map(|r| (r.line_top, r.line_height))
            .collect();
        assert!(runs.len() >= 2, "expected 2 layout runs, got {runs:?}");
        let gap = runs[1].0 - runs[0].0;
        assert!(
            (gap - lh).abs() < 1.0,
            "line gap {gap} vs lh {lh}; runs={runs:?}"
        );
    }

    #[test]
    fn unconstrained_height_keeps_all_hard_lines() {
        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        install_shared_font_system(&mut fs);

        let size = 14.0_f32;
        let lh = size * tailwind_default_leading_scale(size);
        let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
        // Matches prepare_text's Input/Textarea path: height_opt = None.
        buffer.set_size(&mut fs, Some(360.0), None);
        buffer.set_text(
            &mut fs,
            "啊啊\n啊\n啊",
            glyphon::Attrs::new().family(glyphon::Family::SansSerif),
            glyphon::Shaping::Advanced,
        );
        buffer.set_scroll(glyphon::cosmic_text::Scroll::default());
        buffer.shape_until_scroll(&mut fs, false);
        buffer.set_scroll(glyphon::cosmic_text::Scroll::default());
        let n = buffer.layout_runs().count();
        assert_eq!(buffer.lines.len(), 3);
        assert!(
            n >= 3,
            "height None must yield all hard lines (runs={n}, lines={})",
            buffer.lines.len()
        );
    }

    /// Reproducing gallery bug: reshape from 1-line → 3-line while reusing the same Buffer
    /// (shaped_cache hit path only resets scroll — must still expose new lines after miss/reshape).
    #[test]
    fn reshape_growing_text_with_content_height_shows_all_lines() {
        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        install_shared_font_system(&mut fs);

        let size = 14.0_f32;
        let lh = size * tailwind_default_leading_scale(size);
        let attrs = glyphon::Attrs::new().family(glyphon::Family::SansSerif);
        let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));

        // First shape: single line into a 1-line-tall buffer (historical squash path).
        buffer.set_size(&mut fs, Some(360.0), Some(lh + 4.0));
        buffer.set_text(&mut fs, "啊啊", attrs, glyphon::Shaping::Advanced);
        buffer.shape_until_scroll(&mut fs, false);
        assert_eq!(buffer.layout_runs().count(), 1);

        // Grow text; size buffer to content (the fix). Must not keep the 1-line window.
        let text = "啊啊\n啊\n啊";
        let soft_h = lh * 3.0 + 8.0;
        buffer.set_size(&mut fs, Some(360.0), Some(soft_h));
        buffer.set_text(&mut fs, text, attrs, glyphon::Shaping::Advanced);
        buffer.set_scroll(glyphon::cosmic_text::Scroll::default());
        buffer.shape_until_scroll(&mut fs, false);
        let n = buffer.layout_runs().count();
        assert!(
            n >= 3,
            "after grow+content height, expected >=3 runs, got {n}"
        );
    }

    /// Bug: growing text while leaving a 1-line `height_opt` drops lines 2+ (matches screenshot).
    #[test]
    fn reshape_growing_text_keeping_short_height_drops_lines() {
        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        install_shared_font_system(&mut fs);

        let size = 14.0_f32;
        let lh = size * tailwind_default_leading_scale(size);
        let attrs = glyphon::Attrs::new().family(glyphon::Family::SansSerif);
        let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
        buffer.set_size(&mut fs, Some(360.0), Some(lh + 4.0));
        buffer.set_text(&mut fs, "啊啊", attrs, glyphon::Shaping::Advanced);
        buffer.shape_until_scroll(&mut fs, false);

        buffer.set_text(&mut fs, "啊啊\n啊\n啊", attrs, glyphon::Shaping::Advanced);
        // height_opt still lh+4 from set_size above — set_text does not clear it.
        buffer.shape_until_scroll(&mut fs, false);
        let n = buffer.layout_runs().count();
        assert_eq!(
            n, 1,
            "stale 1-line height_opt must drop later hard lines (got {n})"
        );
    }
}
