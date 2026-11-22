use jadix_core::{hosted::{HostedMenuItem, MenuAction, MenuNavigation}, presentation::NavigationKey::*};

fn item(id: &str, children: Vec<HostedMenuItem>) -> HostedMenuItem {
    serde_json::from_value(serde_json::json!({"id":id,"label":id,"enabled":true,"checked":false,"children":children})).unwrap()
}

#[test]
fn inline_groups_skip_headers_and_disabled_descendants_and_restore_submenu_focus() {
    let mut disabled = item("disabled", vec![item("blocked", vec![])]);
    disabled.inline = true; disabled.enabled = false;
    let mut group = item("group", vec![item("Alpha", vec![]), item("Beta", vec![item("Child", vec![])])]);
    group.inline = true;
    let mut menu = MenuNavigation::new(vec![disabled, group]).unwrap();
    assert_eq!(menu.highlighted(), Some("Alpha"));
    assert_eq!(menu.activate("blocked"), MenuAction::None);
    assert_eq!(menu.activate("group"), MenuAction::None);
    menu.navigate(Up);
    assert_eq!(menu.highlighted(), Some("Beta"));
    assert_eq!(menu.navigate(Right), MenuAction::Changed);
    assert_eq!(menu.path(), ["Beta"]);
    assert_eq!(menu.navigate(Enter), MenuAction::Selected("Child".into()));
    assert_eq!(menu.navigate(Escape), MenuAction::Changed);
    assert_eq!(menu.highlighted(), Some("Beta"));
    menu.navigate(Home);
    assert!(menu.typeahead("bE"));
    assert_eq!(menu.highlighted(), Some("Beta"));
    menu.reset();
    assert_eq!(menu.highlighted(), Some("Alpha"));
    assert_eq!(menu.navigate(Escape), MenuAction::Close);
}

#[test]
fn empty_or_disabled_menu_navigation_is_total_and_duplicate_ids_are_rejected() {
    let mut disabled = item("disabled", vec![]); disabled.enabled = false;
    for items in [vec![], vec![disabled]] {
        let mut menu = MenuNavigation::new(items).unwrap();
        for key in [Up, Down, Home, End, Enter, Right] { assert_eq!(menu.navigate(key), MenuAction::None); }
        assert_eq!(menu.highlighted(), None);
        assert!(!menu.typeahead("disabled"));
    }
    assert!(MenuNavigation::new(vec![item("x", vec![item("x", vec![])])]).is_err());
}

#[test]
fn pages_skip_disabled_items_and_clamp_without_wrapping() {
    let mut items: Vec<_> = (0..20).map(|n| item(&n.to_string(), vec![])).collect();
    items[3].enabled = false;
    let mut menu = MenuNavigation::new(items).unwrap();
    menu.navigate_page(PageDown, 4);
    assert_eq!(menu.highlighted(), Some("5"));
    menu.navigate_page(PageUp, 4);
    assert_eq!(menu.highlighted(), Some("0"));
    menu.navigate_page(PageDown, usize::MAX);
    assert_eq!(menu.highlighted(), Some("19"));
    menu.navigate(PageDown);
    assert_eq!(menu.highlighted(), Some("19"));
}

#[test]
fn typeahead_buffers_prefixes_cycles_repeats_and_expires() {
    use web_time::{Duration, Instant};
    let mut menu = MenuNavigation::new(vec![item("Apple", vec![]), item("Apricot", vec![]), item("Banana", vec![])]).unwrap();
    let now = Instant::now();
    assert!(menu.typeahead_input_at("a", now));
    assert_eq!(menu.highlighted(), Some("Apricot"));
    assert!(menu.typeahead_input_at("p", now + Duration::from_millis(20)));
    assert_eq!(menu.highlighted(), Some("Apricot"));
    assert!(menu.typeahead_input_at("p", now + Duration::from_millis(40)));
    assert_eq!(menu.highlighted(), Some("Apple"));
    assert!(menu.typeahead_input_at("b", now + Duration::from_secs(1)));
    assert_eq!(menu.highlighted(), Some("Banana"));
    assert!(menu.typeahead_input_at("a", now + Duration::from_secs(2)));
    assert_eq!(menu.highlighted(), Some("Apple"));
    assert!(menu.typeahead_input_at("a", now + Duration::from_millis(2050)));
    assert_eq!(menu.highlighted(), Some("Apricot"));
    assert!(!menu.typeahead_input_at("\n", now));
    menu.navigate(Home);
    assert!(menu.typeahead_input_at("b", now));
    assert_eq!(menu.highlighted(), Some("Banana"));
}
