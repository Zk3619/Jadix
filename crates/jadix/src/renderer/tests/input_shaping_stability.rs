use super::node_shape_identity_hash;
use super::nowrap_layout_width_from_text;
use crate::renderer::font_loader;
use crate::renderer::text_hit::best_font_family;
use crate::renderer::text_prepare_debug::is_input_inline_text;
use crate::style::TextDirection;
use crate::style::TextOverflow;

fn shaped_line_w(fs: &mut glyphon::FontSystem, text: &str, shaping: glyphon::Shaping) -> f32 {
    let size = 14.0_f32;
    let lh = size * 1.5;
    let family = best_font_family(fs);
    let mut buffer = glyphon::Buffer::new(fs, glyphon::Metrics::new(size, lh));
    buffer.set_size(Some(1024.0), Some(lh + 2.0));
    buffer.set_text(
        text,
        &glyphon::Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(400)),
        shaping,
        None,
    );
    buffer.shape_until_scroll(fs, false);
    buffer
        .layout_runs()
        .map(|run| run.line_w)
        .fold(0.0_f32, f32::max)
}

#[test]
fn scroll_fast_skips_reshape_only_for_idle_inputs() {
    assert!(super::scroll_fast_skips_input_reshape(true, true, false));
    assert!(
        !super::scroll_fast_skips_input_reshape(true, true, true),
        "focused caret must reshape"
    );
    assert!(!super::scroll_fast_skips_input_reshape(true, false, false));
    assert!(!super::scroll_fast_skips_input_reshape(false, true, false));
}

#[test]
fn scroll_fast_skips_intrinsic_only_for_idle_unpanned_inputs() {
    assert!(super::scroll_fast_skips_input_intrinsic_prepare(
        true, true, false, 0.0
    ));
    assert!(!super::scroll_fast_skips_input_intrinsic_prepare(
        true, true, true, 0.0
    ));
    assert!(!super::scroll_fast_skips_input_intrinsic_prepare(
        true, true, false, 12.0
    ));
    assert!(!super::scroll_fast_skips_input_intrinsic_prepare(
        false, true, false, 0.0
    ));
}

fn dummy_shaped(
    fs: &mut glyphon::FontSystem,
    hash: u64,
    measured: f32,
) -> crate::renderer::text::ShapedTextEntry {
    let mut e = super::make_dummy_cache_entry(fs);
    e.shape_hash = hash;
    e.key_hash = hash.wrapping_mul(3);
    e.measured_text_w = measured;
    e
}

fn reclaim_dummy(
    cache: &mut Vec<crate::renderer::text::ShapedTextEntry>,
    pool: &mut super::ShapedIdentityPool,
    fs: &mut glyphon::FontSystem,
    i: usize,
    hash: u64,
) {
    super::assign_shaped_slot(cache, pool, fs, i, hash);
}

fn grow_and_reclaim(
    cache: &mut Vec<crate::renderer::text::ShapedTextEntry>,
    pool: &mut super::ShapedIdentityPool,
    fs: &mut glyphon::FontSystem,
    hashes: &[u64],
) {
    super::park_shaped_suffix(cache, pool, hashes.len(), super::SHAPED_IDENTITY_POOL_CAP);
    for (i, &hash) in hashes.iter().enumerate() {
        super::assign_shaped_slot(cache, pool, fs, i, hash);
    }
}

#[test]
fn unchanged_prefix_survives_suffix_reorder_clip_and_restore() {
    let mut fs = crate::renderer::font_loader::create_font_system();
    let mut cache: Vec<_> = [0xA, 0xB, 0xC, 0xC, 0xD]
        .into_iter()
        .map(|hash| dummy_shaped(&mut fs, hash, hash as f32))
        .collect();
    let mut pool = super::ShapedIdentityPool::new();
    let stable = [0xA, 0xB, 0xC, 0xC, 0xD];
    grow_and_reclaim(&mut cache, &mut pool, &mut fs, &stable);
    assert!(
        pool.is_empty(),
        "unchanged slots must stay outside the pool"
    );
    for hashes in [
        &[0xA, 0xB, 0xD, 0xC, 0xC][..],
        &[0xC, 0xD][..],
        &stable[..],
        &[0xA, 0xB, 0xC][..],
        &stable[..],
    ] {
        grow_and_reclaim(&mut cache, &mut pool, &mut fs, hashes);
        assert_eq!(cache.len(), hashes.len());
        for (entry, &hash) in cache.iter().zip(hashes) {
            assert_eq!(entry.shape_hash, hash, "existing runs must not reshape");
            assert_eq!(entry.measured_text_w, hash as f32);
        }
    }
}

#[test]
#[ignore = "isolated shaped-cache CPU comparison; run without concurrent workloads"]
fn shaped_slot_reuse_cost() {
    let mut fs = crate::renderer::font_loader::create_font_system();
    let hashes: Vec<_> = (1..=80).collect();
    let mut cache: Vec<_> = hashes
        .iter()
        .map(|&hash| dummy_shaped(&mut fs, hash, hash as f32))
        .collect();
    let mut pool = super::ShapedIdentityPool::new();
    for eager in [true, false] {
        let start = std::time::Instant::now();
        for _ in 0..2000 {
            if eager {
                super::park_shaped_cache(&mut cache, &mut pool);
            }
            grow_and_reclaim(&mut cache, &mut pool, &mut fs, &hashes);
        }
        eprintln!(
            "shaped-cache eager={eager} us/frame={:.3}",
            start.elapsed().as_secs_f64() * 1_000_000.0 / 2000.0
        );
        assert!(pool.is_empty());
        assert!(cache
            .iter()
            .zip(&hashes)
            .all(|(entry, hash)| entry.shape_hash == *hash));
    }
}

#[test]
fn shaped_identity_pool_reclaims_across_index_churn() {
    let mut fs = crate::renderer::font_loader::create_font_system();
    let mut cache = vec![dummy_shaped(&mut fs, 0xB0B, 7.0)];
    let mut pool = super::ShapedIdentityPool::new();
    super::park_shaped_entry(&mut pool, dummy_shaped(&mut fs, 0xA11CE, 42.0));
    reclaim_dummy(&mut cache, &mut pool, &mut fs, 0, 0xA11CE);
    assert_eq!(cache[0].shape_hash, 0xA11CE);
    assert_eq!(cache[0].measured_text_w, 42.0);
    assert!(pool.contains_key(&0xB0B));
    assert!(!pool.contains_key(&0xA11CE));
}

#[test]
fn prefix_insert_reclaims_from_identity_pool() {
    let mut fs = crate::renderer::font_loader::create_font_system();
    let mut cache = vec![
        dummy_shaped(&mut fs, 0xB, 2.0),
        dummy_shaped(&mut fs, 0xC, 3.0),
        dummy_shaped(&mut fs, 0, 0.0),
    ];
    let mut pool = super::ShapedIdentityPool::new();
    super::park_shaped_entry(&mut pool, dummy_shaped(&mut fs, 0xA, 1.0));
    reclaim_dummy(&mut cache, &mut pool, &mut fs, 0, 0xA);
    assert_eq!(cache[0].shape_hash, 0xA);
    assert_eq!(cache[0].measured_text_w, 1.0);
    reclaim_dummy(&mut cache, &mut pool, &mut fs, 1, 0xB);
    assert_eq!(cache[1].shape_hash, 0xB);
    reclaim_dummy(&mut cache, &mut pool, &mut fs, 2, 0xC);
    assert_eq!(cache[2].shape_hash, 0xC);
}

/// Gallery scroll: first visible run leaves, everyone else shifts, a new run
/// enters. Pre-fix (index-aligned, no park-all): every live slot Miss-reshaped.
#[test]
fn band_cull_prefix_drop_reclaims_after_park_all() {
    let mut fs = crate::renderer::font_loader::create_font_system();
    let mut cache = vec![
        dummy_shaped(&mut fs, 0xA, 1.0),
        dummy_shaped(&mut fs, 0xB, 2.0),
        dummy_shaped(&mut fs, 0xC, 3.0),
        dummy_shaped(&mut fs, 0xD, 4.0),
        dummy_shaped(&mut fs, 0xE, 5.0),
    ];
    let mut pool = super::ShapedIdentityPool::new();
    super::park_shaped_cache(&mut cache, &mut pool);
    grow_and_reclaim(&mut cache, &mut pool, &mut fs, &[0xB, 0xC, 0xD, 0xE, 0xF]);
    assert_eq!(cache[0].shape_hash, 0xB);
    assert_eq!(cache[0].measured_text_w, 2.0);
    assert_eq!(cache[1].shape_hash, 0xC);
    assert_eq!(cache[1].measured_text_w, 3.0);
    assert_eq!(cache[2].shape_hash, 0xD);
    assert_eq!(cache[2].measured_text_w, 4.0);
    assert_eq!(cache[3].shape_hash, 0xE);
    assert_eq!(cache[3].measured_text_w, 5.0);
    assert_eq!(
        cache[4].shape_hash, 0,
        "unmatched F stays a dummy; Miss path will shape it"
    );
}

/// Scroll Clear submits only the damage-strip subset, then the next frame
/// submits the full visible window again. Pool must keep the parked majority.
#[test]
fn subset_then_full_window_reclaims() {
    let mut fs = crate::renderer::font_loader::create_font_system();
    let mut cache = vec![
        dummy_shaped(&mut fs, 0xA, 1.0),
        dummy_shaped(&mut fs, 0xB, 2.0),
        dummy_shaped(&mut fs, 0xC, 3.0),
        dummy_shaped(&mut fs, 0xD, 4.0),
        dummy_shaped(&mut fs, 0xE, 5.0),
    ];
    let mut pool = super::ShapedIdentityPool::new();
    super::park_shaped_cache(&mut cache, &mut pool);
    grow_and_reclaim(&mut cache, &mut pool, &mut fs, &[0xC, 0xD]);
    assert_eq!(cache[0].measured_text_w, 3.0);
    assert_eq!(cache[1].measured_text_w, 4.0);
    super::park_shaped_cache(&mut cache, &mut pool);
    grow_and_reclaim(&mut cache, &mut pool, &mut fs, &[0xA, 0xB, 0xC, 0xD, 0xE]);
    assert_eq!(cache[0].measured_text_w, 1.0);
    assert_eq!(cache[1].measured_text_w, 2.0);
    assert_eq!(cache[2].measured_text_w, 3.0);
    assert_eq!(cache[3].measured_text_w, 4.0);
    assert_eq!(cache[4].measured_text_w, 5.0);
}

/// Clip-strip duplication and repeated reaction emoji share `shape_hash`.
/// HashMap last-write-wins dropped the extra copy; FIFO must keep both.
#[test]
fn duplicate_hash_copies_both_survive_pool() {
    let mut fs = crate::renderer::font_loader::create_font_system();
    let mut cache = vec![
        dummy_shaped(&mut fs, 0x11, 10.0),
        dummy_shaped(&mut fs, 0x11, 20.0),
    ];
    let mut pool = super::ShapedIdentityPool::new();
    super::park_shaped_cache(&mut cache, &mut pool);
    grow_and_reclaim(&mut cache, &mut pool, &mut fs, &[0x11, 0x11]);
    assert_eq!(cache[0].measured_text_w, 10.0);
    assert_eq!(cache[1].measured_text_w, 20.0);
}

/// Saturated pool + park-all used to evict the live set (gallery: 60 nodes,
/// cap 256, leftover 225 ⇒ 29 perpetual misses, hits = cap − leftover).
#[test]
fn park_all_does_not_evict_live_when_pool_is_full() {
    let mut fs = crate::renderer::font_loader::create_font_system();
    let mut pool = super::ShapedIdentityPool::new();
    for i in 0..4u64 {
        super::park_shaped_entry(
            &mut pool,
            dummy_shaped(&mut fs, 0x1000 + i, 100.0 + i as f32),
        );
    }
    assert_eq!(super::identity_pool_len(&pool), 4);
    let mut cache = vec![
        dummy_shaped(&mut fs, 0xA, 1.0),
        dummy_shaped(&mut fs, 0xB, 2.0),
        dummy_shaped(&mut fs, 0xC, 3.0),
    ];
    super::park_shaped_cache_with_cap(&mut cache, &mut pool, 4);
    grow_and_reclaim(&mut cache, &mut pool, &mut fs, &[0xA, 0xB, 0xC]);
    assert_eq!(
        cache[0].measured_text_w, 1.0,
        "live A must survive a full pool"
    );
    assert_eq!(
        cache[1].measured_text_w, 2.0,
        "live B must survive a full pool"
    );
    assert_eq!(
        cache[2].measured_text_w, 3.0,
        "live C must survive a full pool"
    );
}

#[test]
fn identity_hash_includes_requested_weight() {
    let family = glyphon::Family::SansSerif;
    let align = glyphon::cosmic_text::Align::Left;
    let shaping = glyphon::Shaping::Advanced;
    let color = [30u8, 30, 30, 255];
    let medium = node_shape_identity_hash(
        "数据",
        14.0,
        14.0,
        400,
        family,
        align,
        shaping,
        0.0,
        TextDirection::Auto,
        color,
        500,
        true,
    );
    let bold = node_shape_identity_hash(
        "数据",
        14.0,
        14.0,
        400,
        family,
        align,
        shaping,
        0.0,
        TextDirection::Auto,
        color,
        700,
        true,
    );
    assert_ne!(
        medium, bold,
        "sidebar medium vs page-title bold must not share shape_hash when attrs_weight matches"
    );
}

#[test]
fn password_placeholder_basic_vs_advanced_differs() {
    // Documents why Input must not flip shaping on focus (caret install).
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    let basic = shaped_line_w(&mut fs, "Password", glyphon::Shaping::Basic);
    let advanced = shaped_line_w(&mut fs, "Password", glyphon::Shaping::Advanced);
    assert!(
        (basic - advanced).abs() > 0.05,
        "expected Basic/Advanced advance difference for Password; basic={basic} adv={advanced}"
    );
}

#[test]
fn input_inline_template_id_detected() {
    assert!(is_input_inline_text(Some("__jadix_input:pw:text")));
    assert!(!is_input_inline_text(Some("__jadix_input:pw:clear")));
    assert!(!is_input_inline_text(Some("label")));
    assert!(!is_input_inline_text(None));
}

#[test]
fn nowrap_live_intrinsic_prevents_soft_wrap_after_cjk_latin_preedit() {
    // Textarea IME: committed CJK + in-flight pinyin under a narrow leaf.
    // Stale prepared intrinsic (CJK only) + Word wrap → extra runs → line jumps down.
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    crate::text::install_shared_font_system(&mut fs);

    let size = 14.0_f32;
    let lh = 21.0_f32;
    let box_w = 72.0_f32;
    let committed = "你好世界";
    let live = format!("{committed}nihaoshijie");
    let stale = crate::text::prepare(committed, size, 400, lh);
    let stale_w = crate::text::intrinsic_width(&stale);
    let live_w =
        nowrap_layout_width_from_text(&live, size, 400, lh, TextOverflow::Clip, false, box_w);
    assert!(
        live_w > stale_w + 8.0,
        "live wrap must exceed stale CJK-only intrinsic (live={live_w} stale={stale_w})"
    );

    let family = best_font_family(&fs);
    let attrs = glyphon::Attrs::new()
        .family(family)
        .weight(glyphon::fontdb::Weight(400));

    let mut soft = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
    soft.set_size(Some(stale_w.max(box_w)), None);
    soft.set_text(&live, &attrs, glyphon::Shaping::Advanced, None);
    soft.shape_until_scroll(&mut fs, false);
    let soft_runs = soft.layout_runs().count();

    let mut hard = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
    hard.set_wrap(glyphon::Wrap::None);
    hard.set_size(Some(live_w), None);
    hard.set_text(&live, &attrs, glyphon::Shaping::Advanced, None);
    hard.shape_until_scroll(&mut fs, false);
    let hard_runs = hard.layout_runs().count();

    assert!(
        soft_runs > 1,
        "stale narrow Word-wrap must invent extra runs (got {soft_runs})"
    );
    assert_eq!(
        hard_runs, 1,
        "nowrap + live intrinsic must keep one run; got {hard_runs}"
    );
}

#[test]
fn resize_rgba_nn_letterboxes_tall_source() {
    // Tall 8x16 opaque block → 16x16 must stay ~8px wide (not stretched to 16).
    let mut rgba = vec![0u8; 8 * 16 * 4];
    for px in rgba.chunks_exact_mut(4) {
        px.copy_from_slice(&[255, 200, 0, 255]);
    }
    let src = crate::renderer::text::EmojiRaster::new(8, 16, rgba);
    let out = super::resize_rgba_nn(&src, 16);
    assert_eq!((out.width, out.height), (16, 16));
    let mut minx = 16usize;
    let mut maxx = 0usize;
    let mut last_row = 0usize;
    for y in 0..16 {
        for x in 0..16 {
            if out.rgba[(y * 16 + x) * 4 + 3] > 0 {
                minx = minx.min(x);
                maxx = maxx.max(x);
                if y == 15 {
                    last_row += 1;
                }
            }
        }
    }
    let ink_w = maxx + 1 - minx;
    assert!(
        ink_w <= 9,
        "letterbox must preserve aspect (got ink_w={ink_w}; stretch would be ~16)"
    );
    assert_eq!(last_row, 0, "safe inset must keep last row transparent");
}

#[test]
fn resize_rgba_nn_pads_full_bleed_source() {
    let mut rgba = vec![0u8; 14 * 14 * 4];
    for px in rgba.chunks_exact_mut(4) {
        px.copy_from_slice(&[255, 220, 0, 255]);
    }
    let src = crate::renderer::text::EmojiRaster::new(14, 14, rgba);
    let out = super::resize_rgba_nn(&src, 14);
    let last = (0..14)
        .filter(|&x| out.rgba[(13 * 14 + x) * 4 + 3] > 0)
        .count();
    let first = (0..14).filter(|&x| out.rgba[x * 4 + 3] > 0).count();
    assert_eq!(last, 0, "full-bleed source must gain bottom pad");
    assert_eq!(first, 0, "full-bleed source must gain top pad");
}

#[test]
fn caret_blink_solid_then_off_then_on() {
    assert!(super::caret_blink_visible_at_elapsed(0));
    assert!(super::caret_blink_visible_at_elapsed(399));
    assert!(
        !super::caret_blink_visible_at_elapsed(400),
        "first blink beat after the solid pause is hidden"
    );
    assert!(!super::caret_blink_visible_at_elapsed(400 + 519));
    assert!(super::caret_blink_visible_at_elapsed(400 + 520));
    assert_eq!(super::caret_blink_next_remain_ms(0), 400);
    assert_eq!(super::caret_blink_next_remain_ms(400), 520);
}

#[test]
fn underline_offset_positive_moves_away_from_text() {
    // CSS text-underline-offset: larger → further below the box (larger Y).
    let y0 = super::underline_line_y(10.0, 20.0, 2.0, 0.0, 1.0, 1.0);
    let y4 = super::underline_line_y(10.0, 20.0, 2.0, 4.0, 1.0, 1.0);
    assert!(
        (y4 - y0 - 4.0).abs() < 0.01,
        "underline-offset-4 must shift +4px down (y0={y0}, y4={y4})"
    );
}

#[test]
fn underline_offset_survives_tight_text_box_clip() {
    // Tight leaf scissor = text box; CSS +offset paints below the box.
    let box_clip = Some([10.0, 10.0, 40.0, 14.0]);
    let ul_h = 1.0;
    let y = super::underline_line_y(10.0, 14.0, ul_h, 2.0, 1.0, 1.0);
    assert!(
        super::clip_rect_to_bounds([10.0, y, 40.0, ul_h], box_clip).is_none(),
        "without expansion the underline is fully clipped"
    );
    let expanded =
        super::expand_clip_for_text_decoration(box_clip, true, false, 2.0, ul_h, 1.0, 1.0);
    assert!(
        super::clip_rect_to_bounds([10.0, y, 40.0, ul_h], expanded).is_some(),
        "decoration clip expand must keep underline visible"
    );
}

#[test]
fn decoration_ink_extends_below_the_layout_box() {
    let mut n = crate::layout::layout_tree::TextLayoutInfo {
        text: "Home".into(),
        shape_suffix: None,
        spans: None,
        template_id: None,
        x: 10.0,
        y: 10.0,
        width: 40.0,
        height: 20.0,
        clip_bounds: None,
        style: crate::style::TextStyle::default(),
        prepared: None,
        glyph_fx: None,
        shimmer: None,
        paint_scale: 1.0,
        paint_transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        paint_origin: [30.0, 20.0],
        glyph_raster: crate::renderer::GlyphRasterMode::Coverage,
    };
    n.style.underline = Some(true);
    n.style.underline_offset = Some(4.0);
    n.style.size = Some(14.0);
    let ink = super::text_decoration_ink_logical(&n, 1.0).expect("ink");
    let box_bottom = n.y + n.height;
    assert!(
        ink[1] + ink[3] > box_bottom + 3.0,
        "offset 4px must leave the layout box (ink={ink:?} bottom={box_bottom})"
    );
}
