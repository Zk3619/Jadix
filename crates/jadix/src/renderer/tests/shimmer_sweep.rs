use crate::renderer::font_loader;
use crate::renderer::text_hit::best_font_family;
use crate::style::TextStyle;
use crate::text::install_shared_font_system;

fn buffer_lines(buffer: &glyphon::Buffer) -> Vec<String> {
    buffer
        .layout_runs()
        .map(|run| run.text.to_string())
        .collect()
}

fn glyph_plain(buffer: &glyphon::Buffer) -> String {
    let mut s = String::new();
    for run in buffer.layout_runs() {
        let line = run.text;
        let mut starts: Vec<usize> = run
            .glyphs
            .iter()
            .filter(|g| g.w > 0.0)
            .map(|g| g.start)
            .collect();
        starts.sort_unstable();
        starts.dedup();
        for start in starts {
            if start >= line.len() || !line.is_char_boundary(start) {
                continue;
            }
            if let Some(ch) = line[start..].chars().next() {
                s.push(ch);
            }
        }
    }
    s
}

fn ink_width(buffer: &glyphon::Buffer) -> f32 {
    buffer
        .layout_runs()
        .map(|run| run.line_w)
        .fold(0.0_f32, f32::max)
}

/// Gallery Status (Marker inside Message): per-glyph shimmer `set_rich_text`
/// used to re-wrap 「Oliver is typing...」 at the hug width into 「Oliver i」
/// (second line clipped by the 1-line leaf) and dest-reset flashed the stub.
#[test]
fn shimmer_recolor_must_keep_oliver_is_typing_on_one_line() {
    let text = "Oliver is typing...";
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);
    let family = best_font_family(&fs);
    let size = 14.0_f32;
    let lh = size * crate::style::TAILWIND_TEXT_SM_LEADING_SCALE;
    let attrs = glyphon::Attrs::new()
        .family(family)
        .weight(glyphon::fontdb::Weight(400));
    let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
    // Default cosmic wrap is Word — same as TextStyle::default white_space Normal.
    buffer.set_size(Some(400.0), None);
    buffer.set_text(text, &attrs, glyphon::Shaping::Advanced, None);
    buffer.shape_until_scroll(&mut fs, false);
    let hug = ink_width(&buffer).max(1.0);
    buffer.set_size(Some(hug), None);
    buffer.shape_until_scroll(&mut fs, false);
    let before = buffer_lines(&buffer);

    let mut st = TextStyle::default();
    st.size = Some(size);
    st.color = Some([115, 115, 115, 255]);
    crate::style::utility::set_frame_dt(1.0 / 60.0);
    for i in 0..8 {
        super::apply_shimmer_sweep(
            &mut buffer,
            &mut fs,
            text,
            &st,
            family,
            400,
            glyphon::Shaping::Advanced,
            &crate::renderer::ShimmerParams::default(),
        );
        let lines = buffer_lines(&buffer);
        let glyphs = glyph_plain(&buffer);
        assert!(
                glyphs.contains("typing"),
                "i={i} shimmer set_rich_text dropped glyphs of 「Oliver is typing...」\n before={before:?}\n after_lines={lines:?}\n glyphs={glyphs:?}\n hug={hug:.1}"
            );
        assert!(
                lines.len() == 1 && glyphs.contains("Oliver is typing"),
                "i={i} shimmer recolor must not wrap/clip the caption into a stub\n before={before:?}\n after_lines={lines:?}\n glyphs={glyphs:?}\n hug={hug:.1}"
            );
    }
}
