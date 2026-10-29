use super::*;

#[test]
fn hover_bg() {
    let t = parse_class_token("hover:bg-accent");
    assert_eq!(t.variants, vec![VariantSelector::Hover]);
    assert_eq!(t.utility, "bg-accent");
}

#[test]
fn stacked_dark_hover() {
    let t = parse_class_token("dark:hover:bg-primary");
    assert_eq!(
        t.variants,
        vec![VariantSelector::Dark, VariantSelector::Hover]
    );
    assert_eq!(t.utility, "bg-primary");
}

#[test]
fn data_state() {
    let t = parse_class_token("data-[state=open]:bg-accent");
    assert_eq!(
        t.variants,
        vec![VariantSelector::Data {
            key: "state".into(),
            value: "open".into()
        }]
    );
    assert_eq!(t.utility, "bg-accent");
}

#[test]
fn arbitrary_keeps_colon() {
    let t = parse_class_token("bg-[#ff00aa]");
    assert!(t.variants.is_empty());
    assert_eq!(t.utility, "bg-[#ff00aa]");
}

#[test]
fn breakpoint_sm_bg() {
    let t = parse_class_token("sm:bg-red");
    assert_eq!(
        t.variants,
        vec![VariantSelector::Breakpoint {
            min_width: BREAKPOINT_SM
        }]
    );
    assert_eq!(t.utility, "bg-red");
}

#[test]
fn breakpoint_2xl_stacked() {
    let t = parse_class_token("md:hover:p-4");
    assert_eq!(
        t.variants,
        vec![
            VariantSelector::Breakpoint {
                min_width: BREAKPOINT_MD
            },
            VariantSelector::Hover
        ]
    );
    assert_eq!(t.utility, "p-4");
}

#[test]
fn group_hover_and_named() {
    let t = parse_class_token("group-hover:bg-accent");
    assert_eq!(t.variants, vec![VariantSelector::GroupHover { name: None }]);
    assert_eq!(t.utility, "bg-accent");
    let named = parse_class_token("group-hover/card:opacity-50");
    assert_eq!(
        named.variants,
        vec![VariantSelector::GroupHover {
            name: Some("card".into())
        }]
    );
    assert_eq!(named.utility, "opacity-50");
}

#[test]
fn peer_hover_and_named() {
    let t = parse_class_token("peer-hover:underline");
    assert_eq!(t.variants, vec![VariantSelector::PeerHover { name: None }]);
    assert_eq!(t.utility, "underline");
    let named = parse_class_token("peer-hover/input:border-primary");
    assert_eq!(
        named.variants,
        vec![VariantSelector::PeerHover {
            name: Some("input".into())
        }]
    );
    assert_eq!(named.utility, "border-primary");
}

#[test]
fn motion_reduce_and_safe() {
    let t = parse_class_token("motion-reduce:opacity-0");
    assert_eq!(t.variants, vec![VariantSelector::MotionReduce]);
    assert_eq!(t.utility, "opacity-0");
    let s = parse_class_token("motion-safe:hover:scale-105");
    assert_eq!(
        s.variants,
        vec![VariantSelector::MotionSafe, VariantSelector::Hover]
    );
    assert_eq!(s.utility, "scale-105");
}

#[test]
fn arbitrary_min_max_breakpoints() {
    let t = parse_class_token("min-[800px]:px-8");
    assert_eq!(
        t.variants,
        vec![VariantSelector::Breakpoint { min_width: 800 }]
    );
    assert_eq!(t.utility, "px-8");
    let m = parse_class_token("max-[599px]:flex-col");
    assert_eq!(
        m.variants,
        vec![VariantSelector::MaxBreakpoint { max_width: 599 }]
    );
    assert_eq!(m.utility, "flex-col");
    let range = parse_class_token("min-[800px]:max-[1199px]:gap-4");
    assert_eq!(
        range.variants,
        vec![
            VariantSelector::Breakpoint { min_width: 800 },
            VariantSelector::MaxBreakpoint { max_width: 1199 }
        ]
    );
    assert_eq!(range.utility, "gap-4");
    let named_max = parse_class_token("max-sm:flex-col");
    assert_eq!(
        named_max.variants,
        vec![VariantSelector::MaxBreakpoint { max_width: 640 }]
    );
    assert!(parse_class_token("min-[50rem]:p-2").variants.is_empty());
}
