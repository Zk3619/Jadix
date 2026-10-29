use super::*;

#[test]
fn accepts_plain_reload_command() {
    assert!(matches!(
        parse_command("reload"),
        Some(HotReloadCommand::Reload)
    ));
    assert!(matches!(
        parse_command("RELOAD"),
        Some(HotReloadCommand::Reload)
    ));
}

#[test]
fn decodes_template_patch_using_shared_protocol_types() {
    let message = r#"{"kind":"template_patch","file":"src/main.rs","templates":[{"id":"root","hash":"abc","len":1,"index":0,"text_literals":["old"],"text_replacements":[{"index":0,"from":"old","to":"new"}],"style_attrs":[{"element_index":0,"name":"color","value":"red"}]}]}"#;
    let Some(HotReloadCommand::TemplatePatch(patch)) = parse_command(message) else {
        panic!("template patch decoded");
    };
    assert_eq!(patch.file, "src/main.rs");
    assert_eq!(patch.templates[0].id, "root");
    assert_eq!(patch.templates[0].text_replacements[0].to, "new");
    assert_eq!(patch.templates[0].style_attrs[0].name, "color");
}

#[test]
fn rejects_unknown_or_malformed_messages() {
    assert!(parse_command("not-json").is_none());
    assert!(parse_command(r#"{"kind":"unknown"}"#).is_none());
}
