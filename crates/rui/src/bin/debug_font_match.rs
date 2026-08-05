use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use glyphon::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

fn collect_used_faces(
    font_system: &mut FontSystem,
    family: Family<'_>,
    weight: u16,
    text: &str,
) -> String {
    let mut buffer = Buffer::new(font_system, Metrics::new(14.0, 20.0));
    buffer.set_size(font_system, Some(800.0), Some(40.0));
    buffer.set_text(
        font_system,
        text,
        Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(weight)),
        Shaping::Advanced,
    );
    buffer.shape_until_scroll(font_system, false);

    let db = font_system.db();
    let mut used = BTreeSet::<(String, u16)>::new();
    for run in buffer.layout_runs() {
        for glyph in run.glyphs {
            if let Some(face) = db.face(glyph.font_id) {
                let name = face
                    .families
                    .first()
                    .map(|(n, _)| n.clone())
                    .unwrap_or_else(|| "?".to_string());
                used.insert((name, face.weight.0));
            }
        }
    }
    format!("{used:?}")
}

fn print_case(
    out: &mut dyn Write,
    font_system: &mut FontSystem,
    sample_name: &str,
    req_family: &str,
    req_weight: u16,
    text: &str,
) -> std::io::Result<()> {
    let resolved_family = rui::utils::font::resolve_family(font_system.db(), req_family);
    let effective_weight = rui::utils::font::normalize_weight_for_family(
        font_system.db(),
        resolved_family,
        req_weight,
    );
    let resolved_label = match resolved_family {
        Family::Name(n) => n.to_string(),
        Family::SansSerif => "sans-serif".to_string(),
        Family::Serif => "serif".to_string(),
        Family::Monospace => "monospace".to_string(),
        Family::Cursive => "cursive".to_string(),
        Family::Fantasy => "fantasy".to_string(),
    };
    let used = collect_used_faces(font_system, resolved_family, effective_weight, text);

    writeln!(
        out,
        "sample={sample_name:<14} req_family={req_family:<18} req_w={req_weight:<3} -> resolved_family={resolved_label:<16} effective_w={effective_weight:<3} used={used}"
    )
}

fn main() -> std::io::Result<()> {
    // Match the app renderer (curated fonts — no full system scan).
    // Escape hatch: RUI_LOAD_SYSTEM_FONTS=1
    let mut font_system = rui::renderer::font_loader::create_font_system();

    let mut sans_families: Vec<String> = font_system
        .db()
        .faces()
        .flat_map(|f| f.families.iter().map(|(n, _)| n.clone()))
        .filter(|n| n.contains("Noto Sans"))
        .collect();
    sans_families.sort();
    sans_families.dedup();

    let report_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("debug_font_match_report.txt");
    let mut out = BufWriter::new(File::create(&report_path)?);

    writeln!(out, "=== debug_font_match ===")?;
    writeln!(out, "report_path={}", report_path.display())?;
    writeln!(out, "noto-sans families: {:?}", sans_families)?;

    let samples = [
        ("baseline", "Font probe: Hello 你好 日本語 한국어 😀 ∑ ♪ §"),
        ("latin_ext", "Latin ext: ÀÁÂÃÄÅ ÇÐĒĞİŁŃŐŒŘŚŞŤŮŸŽ ƒ ƛ Ǽ Ȝ"),
        ("arabic", "Arabic: العربية مرحبا بكم ١٢٣٤٥٦٧٨٩٠"),
        (
            "devanagari",
            "Devanagari: देवनागरी हिन्दी भाषा क ख ग घ ङ १२३४",
        ),
        ("symbol_block", "Symbols: § ¶ † ‡ ‰ ‱ ※ ※ ♠ ♣ ♥ ♦ ⚑ ⚙ ⌘ ⌥"),
        (
            "math_alnum",
            "Math alnum: 𝐀 𝐵 𝑪 𝒟 𝔼 𝕱 𝖦 𝟘 𝟙 𝟚 ∀ ∃ ∈ ∑ ∏ √ ∞",
        ),
        ("emoji_zwj", "Emoji ZWJ: 👨‍👩‍👧‍👦 👩‍💻 🧑🏽‍🚀 🏳️‍🌈 👩🏽‍❤️‍💋‍👨🏻 🤝🏻"),
        (
            "mixed_stress",
            "Mixed: Hello العربية देवनागरी 日本語 한국어 𝟘𝟙𝟚 ⚙️ 👨‍👩‍👧‍👦",
        ),
    ];
    let families = [
        "Noto Sans CJK SC",
        "Noto Sans CJK JP",
        "Noto Sans CJK KR",
        "Noto Sans CJK TC",
        "Noto Sans CJK HK",
        "Noto Serif CJK SC",
        "Noto Serif CJK JP",
        "Noto Sans Symbols 2",
        "Noto Emoji",
        "sans-serif",
        "serif",
    ];
    let weights = [100u16, 200, 300, 400, 500, 600, 700, 800, 900];

    writeln!(out, "samples: {}", samples.len())?;
    for locale in ["sc", "tc", "hk", "jp", "kr"] {
        std::env::set_var("RUI_TEXT_LOCALE", locale);
        writeln!(out, "-- locale={locale} --")?;
        for (sample_name, text) in samples {
            writeln!(out, "  >> sample={sample_name}")?;
            for f in families {
                for w in weights {
                    print_case(&mut out, &mut font_system, sample_name, f, w, text)?;
                }
            }
        }
    }

    out.flush()?;
    eprintln!("Wrote {}", report_path.display());
    Ok(())
}
