use super::{collect_hoisted_scroll_rests, retain_unfrozen_scroll_rests};
use crate::app::portal_overlay::ScrollFreeze;
use crate::core::tree::Element;
use crate::layout::layout_tree::ATTR_HOISTED_PORTAL;

#[test]
fn collect_hoisted_scroll_rests_skips_page_viewports() {
    let mut page_vp = Element::new();
    page_vp.focus_id = Some("scroll:page".into());
    let mut overlay = Element::new();
    overlay.set_attr(ATTR_HOISTED_PORTAL, "1");
    let mut menu_vp = Element::new();
    menu_vp.focus_id = Some("scroll:menu".into());
    overlay.children.push(menu_vp);
    page_vp.children.push(overlay);

    let hoisted = collect_hoisted_scroll_rests(&page_vp);
    crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut page_vp, false);
    assert_eq!(collect_hoisted_scroll_rests(&page_vp), hoisted);
    let mut mounted = Element::new();
    mounted.focus_id = Some("scroll:mounted".into());
    page_vp.children[0].children.push(mounted);
    crate::layout::collect_cache::invalidate_collect_paint_hash_path(&mut page_vp, &[0]);
    assert!(collect_hoisted_scroll_rests(&page_vp).contains("mounted"));
    crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut page_vp, true);
    assert!(collect_hoisted_scroll_rests(&page_vp).contains("mounted"));
    assert!(hoisted.contains("menu"));
    assert!(!hoisted.contains("page"));

    let mut rests = vec!["page".into(), "menu".into()];
    retain_unfrozen_scroll_rests(&mut rests, ScrollFreeze::Page, &hoisted);
    assert_eq!(rests, vec!["menu".to_string()]);
}
