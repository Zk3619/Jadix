use jadix_core::hosted::HostedContent;
use serde_json::{json, Value};

fn menu(items: Value) -> Value {
    json!({"type":"menu_button", "label":"Actions", "enabled":true, "items":items})
}
fn item(id: &str) -> Value {
    json!({"id":id, "label":id, "enabled":true, "checked":false})
}
fn parse(value: &Value) -> Result<HostedContent, &'static str> {
    HostedContent::from_json(&value.to_string())
}

#[test]
fn web_navigation_checks_origins_credentials_and_bundle_traversal() {
    let web = parse(&json!({"type":"web", "html":"<p>test</p>", "origins":["https://example.test"]})).unwrap();
    for url in ["about:blank", "https://example.test/path?q=ok", "app://bundle/index.html", "asset://bundle/page.html"] {
        assert!(web.allows_web_navigation(url, false), "{url}");
    }
    for url in ["http://example.test", "https://example.test:444", "https://other.test", "https://user@example.test", "file:///etc/passwd", "app://bundle/%2e%2e/secret", "asset://bundle/%2fetc/passwd"] {
        assert!(!web.allows_web_navigation(url, false), "{url}");
    }
    assert!(web.allows_web_navigation("app://provider/page", true));
    assert!(!web.allows_web_navigation("app://provider/page", false));
    assert!(!web.allows_web_navigation("app://bundle/%2e%2e/secret", true));
}

#[test]
fn menu_metadata_round_trips_and_old_payloads_keep_defaults() {
    let old = parse(&menu(json!([item("old")]))).unwrap();
    let HostedContent::MenuButton { items, .. } = &old else { panic!() };
    assert_eq!(items[0].symbol, None);
    assert_eq!(items[0].accelerator, None);
    assert!(!items[0].destructive);
    let mut rich = item("remove");
    rich["symbol"] = json!("trash");
    rich["accelerator"] = json!("CmdOrCtrl+Shift+R");
    rich["accessibility_label"] = json!("Remove selected entry");
    rich["destructive"] = json!(true);
    let content = parse(&menu(json!([rich]))).unwrap();
    assert_eq!(HostedContent::from_json(&content.to_json().unwrap()).unwrap(), content);
    assert!(content.accepts_event("select", &json!("remove")));
    assert!(!content.accepts_event("select", &json!("unknown")));
}

#[test]
fn nested_disabled_menus_cannot_dispatch_and_invalid_metadata_is_rejected() {
    let mut parent = item("parent");
    parent["enabled"] = json!(false);
    parent["children"] = json!([item("child")]);
    let content = parse(&menu(json!([parent]))).unwrap();
    assert!(!content.accepts_event("select", &json!("child")));
    for (key, value) in [
        ("accelerator", json!("Shift+R")),
        ("accelerator", json!("Ctrl+Ctrl+R")),
        ("accelerator", json!("F25")),
        ("symbol", json!("")),
        ("symbol", json!("x".repeat(129))),
        ("accessibility_label", json!("bad\0label")),
    ] {
        let mut invalid = item("action"); invalid[key] = value;
        assert!(parse(&menu(json!([invalid]))).is_err(), "{key}");
    }
    let mut parent = item("same"); parent["children"] = json!([item("same")]);
    assert!(parse(&menu(json!([parent]))).is_err());
}

#[test]
fn segment_metadata_preserves_selection_and_disabled_contract() {
    let mut first = item("first"); first.as_object_mut().unwrap().remove("checked");
    first["symbol"] = json!("square.grid.2x2"); first["accessibility_label"] = json!("Grid view");
    let mut second = first.clone(); second["id"] = json!("second"); second["enabled"] = json!(false);
    let value = json!({"type":"segments", "items":[first,second], "selected":"first", "enabled":true});
    let content = parse(&value).unwrap();
    assert_eq!(HostedContent::from_json(&content.to_json().unwrap()).unwrap(), content);
    assert!(content.accepts_event("select", &json!("first")));
    assert!(!content.accepts_event("select", &json!("second")));
    let mut invalid = value; invalid["selected"] = json!("absent");
    assert!(parse(&invalid).is_err());
}

#[test]
fn tabs_and_inline_groups_enforce_portable_identity_and_visibility() {
    let value = json!({"type":"tab_bar", "enabled":true, "hidden":false, "selected":"first",
        "items":[{"id":"first","label":"First","enabled":true,"badge":"3"},
                 {"id":"second","label":"Second","enabled":false}]});
    let tabs = parse(&value).unwrap();
    assert!(tabs.accepts_event("select", &json!("first")));
    assert!(!tabs.accepts_event("select", &json!("second")));
    assert!(!tabs.accepts_children());
    let mut hidden = value.clone(); hidden["hidden"] = json!(true);
    assert!(!parse(&hidden).unwrap().accepts_event("select", &json!("first")));
    let mut duplicate = value; duplicate["items"][1]["id"] = json!("first");
    assert!(parse(&duplicate).is_err());
    let mut group = item("group"); group["inline"] = json!(true); group["children"] = json!([item("leaf")]);
    let menu = parse(&menu(json!([group]))).unwrap();
    assert!(menu.accepts_children());
    assert!(menu.accepts_event("select", &json!("leaf")));
    assert!(!menu.accepts_event("select", &json!("group")));
}

#[test]
fn shader_inputs_keep_old_defaults_and_reject_unsafe_bindings() {
    let original = json!({"type":"shader","source":"vec4 mainImage(vec2 p){return vec4(1.);}",
        "function":"mainImage","uniforms":[],"frames_per_second":60});
    let legacy = parse(&original).unwrap();
    assert!(legacy.accepts_children());
    let HostedContent::Shader { paused, animated, .. } = legacy else { panic!() };
    assert!(!paused && animated);
    let mut rich = original.clone();
    rich["parameters"] = json!({"tint":{"type":"vec4","value":[1.,0.,0.,1.]}});
    rich["textures"] = json!({"picture":"app://images/photo.png"});
    rich["paused"] = json!(true);
    let content = parse(&rich).unwrap();
    assert_eq!(parse(&serde_json::from_str(&content.to_json().unwrap()).unwrap()).unwrap(), content);
    for (key, value) in [
        ("function", json!("1invalid")),
        ("time_seconds", json!(-1)),
        ("parameters", json!({"uTime":{"type":"float","value":1}})),
        ("parameters", json!({"bad;name":{"type":"float","value":1}})),
        ("parameters", json!({"tint":{"type":"vec4","value":[1,0]}})),
        ("textures", json!({"picture":"app://../escape"})),
        ("textures", json!({"tint":"app://image.png"})),
    ] {
        let mut invalid = rich.clone(); invalid[key] = value;
        assert!(parse(&invalid).is_err(), "{key}");
    }
}
