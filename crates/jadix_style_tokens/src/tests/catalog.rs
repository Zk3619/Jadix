use super::*;

#[test]
fn glass_utilities_validate_optical_ranges() {
    assert_eq!(
        classify_utility("leading-[24px]"),
        Some(KnownUtility::LeadingPx(24.0))
    );
    assert!(!is_known_utility("leading-[nanpx]"));
    assert_eq!(
        classify_utility("glass"),
        Some(KnownUtility::Glass([1.46, 12.0, 0.025, 0.6]))
    );
    assert_eq!(
        classify_utility("glass-none"),
        Some(KnownUtility::Glass([0.0; 4]))
    );
    assert_eq!(
        classify_utility("glass-[1.8_24_0.1_0.8]"),
        Some(KnownUtility::Glass([1.8, 24.0, 0.1, 0.8]))
    );
    for invalid in [
        "glass-[nan_12_0_0]",
        "glass-[0.5_12_0_0]",
        "glass-[1.5_-1_0_0]",
        "glass-[1.5_12_1_0]",
        "glass-[1.5_12_0_2]",
        "glass-[1.5_12]",
    ] {
        assert!(!is_known_utility(invalid), "{invalid}");
    }
}

#[test]
fn known_basics() {
    assert!(is_known_utility("flex"));
    assert!(is_known_utility("bg-primary"));
    assert!(is_known_utility(
        "hover:bg-accent".strip_prefix("hover:").unwrap_or("")
    ));
    assert!(is_known_utility("p-4"));
    assert!(is_known_utility("w-[12rem]"));
    assert!(is_known_utility("opacity-50"));
    assert!(is_known_utility("opacity-[0.4]"));
    assert!(is_known_utility("translate-x-4"));
    assert!(is_known_utility("translate-y-2"));
    assert!(is_known_utility("truncate"));
    assert!(is_known_utility("text-overflow-clip"));
    assert!(is_known_utility("text-overflow-hidden"));
    assert!(is_known_utility("text-overflow-ellipsis"));
    assert!(is_known_utility("text-overflow-newline"));
    assert!(matches!(
        classify_utility("text-overflow-clip"),
        Some(KnownUtility::TextOverflow("clip"))
    ));
    assert!(matches!(
        classify_utility("text-overflow-hidden"),
        Some(KnownUtility::TextOverflow("hidden"))
    ));
    assert!(matches!(
        classify_utility("text-overflow-ellipsis"),
        Some(KnownUtility::TextOverflow("ellipsis"))
    ));
    assert!(matches!(
        classify_utility("text-overflow-newline"),
        Some(KnownUtility::TextOverflow("newline"))
    ));
    assert!(matches!(
        classify_utility("text-center"),
        Some(KnownUtility::TextAlign("center"))
    ));
    assert!(matches!(
        classify_utility("text-left"),
        Some(KnownUtility::TextAlign("left"))
    ));
    assert!(matches!(
        classify_utility("text-start"),
        Some(KnownUtility::TextAlign("start"))
    ));
    assert!(matches!(
        classify_utility("text-justify"),
        Some(KnownUtility::TextAlign("justify"))
    ));
    assert!(is_known_utility("font-medium"));
    assert!(matches!(
        classify_utility("font-medium"),
        Some(KnownUtility::FontWeight(FontWeightToken::Medium))
    ));
    assert!(matches!(
        classify_utility("font-500"),
        Some(KnownUtility::FontWeight(FontWeightToken::Medium))
    ));
    assert!(matches!(
        classify_utility("font-sans"),
        Some(KnownUtility::FontFamilySans)
    ));
    assert!(matches!(
        classify_utility("font-serif"),
        Some(KnownUtility::FontFamilySerif)
    ));
    assert!(matches!(
        classify_utility("font-mono"),
        Some(KnownUtility::FontFamilyMono)
    ));
    assert!(matches!(
        classify_utility("leading-none"),
        Some(KnownUtility::Leading(s)) if (s - 1.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("leading-tight"),
        Some(KnownUtility::Leading(s)) if (s - 1.25).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("leading-[1.15]"),
        Some(KnownUtility::Leading(s)) if (s - 1.15).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("whitespace-nowrap"),
        Some(KnownUtility::WhitespaceNowrap)
    ));
    assert!(is_known_utility("box-border"));
    assert!(is_known_utility("box-content"));
    assert!(matches!(
        classify_utility("box-border"),
        Some(KnownUtility::BoxBorder)
    ));
    assert!(matches!(
        classify_utility("box-content"),
        Some(KnownUtility::BoxContent)
    ));
    assert!(!is_known_utility("not-a-real-class"));
    assert!(matches!(
        classify_utility("ring"),
        Some(KnownUtility::RingWidth(w)) if (w - 3.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("ring-2"),
        Some(KnownUtility::RingWidth(w)) if (w - 2.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("ring-[5px]"),
        Some(KnownUtility::RingWidth(w)) if (w - 5.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("ring-primary"),
        Some(KnownUtility::RingColor("primary"))
    ));
    assert!(!is_known_utility("ring-offset-2"));
    assert!(matches!(
        classify_utility("underline"),
        Some(KnownUtility::Underline(true))
    ));
    assert!(matches!(
        classify_utility("no-underline"),
        Some(KnownUtility::Underline(false))
    ));
    assert!(matches!(
        classify_utility("underline-offset-4"),
        Some(KnownUtility::UnderlineOffset(TwLength::Rem(w))) if (w - 1.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("underline-offset-[4px]"),
        Some(KnownUtility::UnderlineOffset(TwLength::Px(w))) if (w - 4.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("underline-offset-[0.25rem]"),
        Some(KnownUtility::UnderlineOffset(TwLength::Rem(w))) if (w - 0.25).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("underline-offset-[2px]"),
        Some(KnownUtility::UnderlineOffset(TwLength::Px(w))) if (w - 2.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("underline-offset-[0.125em]"),
        Some(KnownUtility::UnderlineOffset(TwLength::Em(w))) if (w - 0.125).abs() < f32::EPSILON
    ));
    assert!(classify_utility("underline-offset-[4]").is_none());
}

#[test]
fn size_full_auto_screen_match_web() {
    assert!(matches!(
        classify_utility("w-full"),
        Some(KnownUtility::Width(ref v)) if v == "100%"
    ));
    assert!(matches!(
        classify_utility("h-full"),
        Some(KnownUtility::Height(ref v)) if v == "100%"
    ));
    assert!(matches!(
        classify_utility("min-w-full"),
        Some(KnownUtility::MinWidth(ref v)) if v == "100%"
    ));
    assert!(matches!(
        classify_utility("max-w-full"),
        Some(KnownUtility::MaxWidth(ref v)) if v == "100%"
    ));
    assert!(matches!(
        classify_utility("w-auto"),
        Some(KnownUtility::Width(ref v)) if v == "auto"
    ));
    assert!(matches!(
        classify_utility("w-screen"),
        Some(KnownUtility::Width(ref v)) if v == "100vw"
    ));
    assert!(matches!(
        classify_utility("h-screen"),
        Some(KnownUtility::Height(ref v)) if v == "100vh"
    ));
    assert!(matches!(
        classify_utility("w-fit"),
        Some(KnownUtility::Width(ref v)) if v == "fit-content"
    ));
    assert!(matches!(
        classify_utility("w-min"),
        Some(KnownUtility::Width(ref v)) if v == "min-content"
    ));
    assert!(matches!(
        classify_utility("max-h-max"),
        Some(KnownUtility::MaxHeight(ref v)) if v == "max-content"
    ));
    // `full` must not resolve as a spacing px for gap/padding.
    assert_eq!(spacing_token_to_px("full"), None);
    assert!(matches!(
        classify_utility("w-64"),
        Some(KnownUtility::Width(ref v)) if v == "16rem"
    ));
}

#[test]
fn size_fraction_width_height() {
    assert!(matches!(
        classify_utility("w-1/2"),
        Some(KnownUtility::Width(ref v)) if v == "50%"
    ));
    assert!(matches!(
        classify_utility("w-3/4"),
        Some(KnownUtility::Width(ref v)) if v == "75%"
    ));
    if let Some(KnownUtility::Height(v)) = classify_utility("h-1/3") {
        let pct: f32 = v.trim_end_matches('%').parse().unwrap();
        assert!((pct - 100.0 / 3.0).abs() < 0.001);
    } else {
        panic!("h-1/3 should classify as Height");
    }
    assert!(matches!(
        classify_utility("min-w-1/4"),
        Some(KnownUtility::MinWidth(ref v)) if v == "25%"
    ));
    if let Some(KnownUtility::MaxHeight(v)) = classify_utility("max-h-11/12") {
        let pct: f32 = v.trim_end_matches('%').parse().unwrap();
        assert!((pct - 11.0 / 12.0 * 100.0).abs() < 0.001);
    } else {
        panic!("max-h-11/12 should classify");
    }
    assert!(is_known_utility("w-3/4"));
    assert!(!is_known_utility("gap-3/4"));
    assert!(matches!(
        classify_utility("translate-x-5/6"),
        Some(KnownUtility::TranslateXPct(p)) if (p - 5.0 / 6.0).abs() < 0.0001
    ));
}

#[test]
fn aspect_square_and_arbitrary() {
    assert!(matches!(
        classify_utility("aspect-square"),
        Some(KnownUtility::AspectRatio(r)) if (r - 1.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("aspect-video"),
        Some(KnownUtility::AspectRatio(r)) if (r - 16.0 / 9.0).abs() < 0.001
    ));
    assert!(matches!(
        classify_utility("aspect-[16/9]"),
        Some(KnownUtility::AspectRatio(r)) if (r - 16.0 / 9.0).abs() < 0.001
    ));
    assert!(matches!(
        classify_utility("aspect-[1.5]"),
        Some(KnownUtility::AspectRatio(r)) if (r - 1.5).abs() < f32::EPSILON
    ));
}

#[test]
fn rotate_scale_tracking_direction() {
    assert!(matches!(
        classify_utility("rotate-45"),
        Some(KnownUtility::Rotate(45.0))
    ));
    assert!(matches!(
        classify_utility("rotate-[12deg]"),
        Some(KnownUtility::Rotate(12.0))
    ));
    assert!(matches!(
        classify_utility("scale-150"),
        Some(KnownUtility::Scale(1.5))
    ));
    assert!(matches!(
        classify_utility("scale-[1.2]"),
        Some(KnownUtility::Scale(1.2))
    ));
    assert!(matches!(
        classify_utility("scale-x-150"),
        Some(KnownUtility::ScaleX(1.5))
    ));
    assert!(matches!(
        classify_utility("scale-y-[0.5]"),
        Some(KnownUtility::ScaleY(0.5))
    ));
    assert!(matches!(
        classify_utility("scale-[50%]"),
        Some(KnownUtility::Scale(0.5))
    ));
    assert!(matches!(
        classify_utility("zoom-200"),
        Some(KnownUtility::Zoom(2.0))
    ));
    assert!(matches!(
        classify_utility("zoom-normal"),
        Some(KnownUtility::Zoom(1.0))
    ));
    assert!(matches!(
        classify_utility("zoom-[1.5]"),
        Some(KnownUtility::Zoom(1.5))
    ));
    assert!(matches!(
        classify_utility("zoom-[75%]"),
        Some(KnownUtility::Zoom(0.75))
    ));
    assert!(matches!(
        classify_utility("tracking-wide"),
        Some(KnownUtility::LetterSpacing(_))
    ));
    assert!(matches!(
        classify_utility("rtl"),
        Some(KnownUtility::TextDirectionRtl)
    ));
    assert!(matches!(
        classify_utility("writing-vertical-rl"),
        Some(KnownUtility::WritingMode("vertical-rl"))
    ));
    assert!(matches!(
        classify_utility("fixed"),
        Some(KnownUtility::PositionFixed)
    ));
}

#[test]
fn negative_margin_and_translate() {
    assert!(matches!(
        classify_utility("-m-4"),
        Some(KnownUtility::Margin(ref v)) if v == "-1rem"
    ));
    assert!(matches!(
        classify_utility("-mx-2"),
        Some(KnownUtility::MarginX(ref v)) if v == "-0.5rem"
    ));
    assert!(matches!(
        classify_utility("-mt-1"),
        Some(KnownUtility::MarginT(ref v)) if v == "-0.25rem"
    ));
    assert!(matches!(
        classify_utility("-translate-x-4"),
        Some(KnownUtility::TranslateX(TwLength::Rem(w))) if (w + 1.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("-translate-y-2"),
        Some(KnownUtility::TranslateY(TwLength::Rem(w))) if (w + 0.5).abs() < f32::EPSILON
    ));
}

#[test]
fn filter_utilities_classify() {
    assert!(matches!(
        classify_utility("blur"),
        Some(KnownUtility::FilterBlur(TwLength::Px(v))) if (v - 8.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("blur-xs"),
        Some(KnownUtility::FilterBlur(TwLength::Px(v))) if (v - 4.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("blur-sm"),
        Some(KnownUtility::FilterBlur(TwLength::Px(v))) if (v - 8.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("blur-lg"),
        Some(KnownUtility::FilterBlur(TwLength::Px(v))) if (v - 16.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("blur-none"),
        Some(KnownUtility::FilterNone)
    ));
    assert!(matches!(
        classify_utility("blur-[10px]"),
        Some(KnownUtility::FilterBlur(TwLength::Px(v))) if (v - 10.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("blur-[1]"),
        Some(KnownUtility::FilterBlur(TwLength::Rem(v))) if (v - 1.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("blur-[1rem]"),
        Some(KnownUtility::FilterBlur(TwLength::Rem(v))) if (v - 1.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("blur-(--blur-xl)"),
        Some(KnownUtility::FilterBlur(TwLength::Px(v))) if (v - 24.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("backdrop-blur-sm"),
        Some(KnownUtility::BackdropBlur(TwLength::Px(v))) if (v - 8.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("backdrop-blur-none"),
        Some(KnownUtility::BackdropBlur(TwLength::Px(v))) if v == 0.0
    ));
    assert!(matches!(
        classify_utility("brightness-50"),
        Some(KnownUtility::FilterBrightness(v)) if (v - 0.5).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("contrast-150"),
        Some(KnownUtility::FilterContrast(v)) if (v - 1.5).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("grayscale"),
        Some(KnownUtility::FilterGrayscale(1.0))
    ));
    assert!(matches!(
        classify_utility("grayscale-0"),
        Some(KnownUtility::FilterGrayscale(0.0))
    ));
    assert!(matches!(
        classify_utility("sepia"),
        Some(KnownUtility::FilterSepia(1.0))
    ));
    assert!(matches!(
        classify_utility("invert-0"),
        Some(KnownUtility::FilterInvert(0.0))
    ));
    assert!(matches!(
        classify_utility("saturate-200"),
        Some(KnownUtility::FilterSaturate(v)) if (v - 2.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("hue-rotate-90"),
        Some(KnownUtility::FilterHueRotate(90.0))
    ));
    assert!(matches!(
        classify_utility("hue-rotate-[45deg]"),
        Some(KnownUtility::FilterHueRotate(45.0))
    ));
    assert!(matches!(
        classify_utility("drop-shadow"),
        Some(KnownUtility::FilterDropShadow(""))
    ));
    assert!(matches!(
        classify_utility("drop-shadow-md"),
        Some(KnownUtility::FilterDropShadow("md"))
    ));
    assert!(matches!(
        classify_utility("filter-none"),
        Some(KnownUtility::FilterNone)
    ));
}

#[test]
fn transition_and_animate_classify() {
    assert!(matches!(
        classify_utility("transition-opacity"),
        Some(KnownUtility::TransitionOpacity)
    ));
    assert!(matches!(
        classify_utility("transition-colors"),
        Some(KnownUtility::TransitionColors)
    ));
    assert!(matches!(
        classify_utility("transition-padding"),
        Some(KnownUtility::TransitionPadding)
    ));
    assert!(matches!(
        classify_utility("transition-transform"),
        Some(KnownUtility::TransitionTransform)
    ));
    assert!(matches!(
        classify_utility("animate-spin"),
        Some(KnownUtility::AnimateSpin)
    ));
    assert!(matches!(
        classify_utility("animate-ping"),
        Some(KnownUtility::AnimatePing)
    ));
    assert!(matches!(
        classify_utility("animate-pulse"),
        Some(KnownUtility::AnimatePulse)
    ));
    assert!(matches!(
        classify_utility("animate-bounce"),
        Some(KnownUtility::AnimateBounce)
    ));
    assert!(matches!(
        classify_utility("animate-none"),
        Some(KnownUtility::AnimateNone)
    ));
}

#[test]
fn shimmer_utility_classes() {
    assert!(matches!(
        classify_utility("shimmer"),
        Some(KnownUtility::Shimmer)
    ));
    assert!(matches!(
        classify_utility("shimmer-once"),
        Some(KnownUtility::ShimmerOnce)
    ));
    assert!(matches!(
        classify_utility("shimmer-count-3"),
        Some(KnownUtility::ShimmerCount(Some(n))) if (n - 3.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("shimmer-count-infinite"),
        Some(KnownUtility::ShimmerCount(None))
    ));
    assert!(matches!(
        classify_utility("shimmer-box"),
        Some(KnownUtility::ShimmerBox)
    ));
    assert!(matches!(
        classify_utility("animate-shimmer"),
        Some(KnownUtility::ShimmerBox)
    ));
    assert!(matches!(
        classify_utility("shimmer-reverse"),
        Some(KnownUtility::ShimmerReverse)
    ));
    assert!(matches!(
        classify_utility("shimmer-none"),
        Some(KnownUtility::ShimmerNone)
    ));
    assert!(matches!(
        classify_utility("shimmer-duration-1000"),
        Some(KnownUtility::ShimmerDuration(1000))
    ));
    assert!(matches!(
        classify_utility("shimmer-spread-24"),
        Some(KnownUtility::ShimmerSpread(px)) if (px - 96.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("shimmer-spread-[5rem]"),
        Some(KnownUtility::ShimmerSpread(px)) if (px - 80.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("shimmer-angle-45"),
        Some(KnownUtility::ShimmerAngle(deg)) if (deg - 45.0).abs() < f32::EPSILON
    ));
    assert!(matches!(
        classify_utility("shimmer-color-blue-500/60"),
        Some(KnownUtility::ShimmerColor("blue-500/60"))
    ));
    assert!(matches!(
        classify_utility("shimmer-color-[#378ADD]"),
        Some(KnownUtility::ShimmerColor("[#378ADD]"))
    ));
    assert!(is_known_utility("shimmer"));
    assert!(!is_known_utility("shimmering"));
}

#[test]
fn cursor_utility_classes() {
    assert!(matches!(
        classify_utility("cursor-pointer"),
        Some(KnownUtility::Cursor("pointer"))
    ));
    assert!(matches!(
        classify_utility("cursor-not-allowed"),
        Some(KnownUtility::Cursor("not-allowed"))
    ));
    assert!(matches!(
        classify_utility("cursor-ew-resize"),
        Some(KnownUtility::Cursor("ew-resize"))
    ));
    assert!(classify_utility("cursor-").is_none());
}
