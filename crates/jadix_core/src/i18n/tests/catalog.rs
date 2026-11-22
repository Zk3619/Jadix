use super::*;

#[test]
fn flatten_toml_nested() {
    let src = r#"
[jadix.pagination]
previous = "上一页"
next = "下一页"
"#;
    let b = MessageBundle::from_toml("zh-CN", src).unwrap();
    assert_eq!(b.get("jadix.pagination.previous"), Some("上一页"));
    assert_eq!(b.get("jadix.pagination.next"), Some("下一页"));
}

#[test]
fn catalog_fallback_zh_cn_to_zh() {
    let mut cat = Catalog::new();
    let mut zh = MessageBundle::new("zh");
    zh.insert("hello", "你好");
    cat.add(zh);
    let mut en = MessageBundle::new("en");
    en.insert("hello", "Hello");
    en.insert("bye", "Bye");
    cat.add(en);
    assert_eq!(
        cat.get(&Locale::parse("zh-CN").unwrap(), "hello"),
        Some("你好")
    );
    assert_eq!(
        cat.get(&Locale::parse("zh-CN").unwrap(), "bye"),
        Some("Bye")
    );
}

#[test]
fn flatten_json() {
    let src = r#"{"jadix":{"common":{"open":"打开"}}}"#;
    let b = MessageBundle::from_json("zh", src).unwrap();
    assert_eq!(b.get("jadix.common.open"), Some("打开"));
}

#[test]
fn register_vacant_first_wins_per_locale() {
    let mut cat = Catalog::new();
    cat.register_vacant("en-US", "hello", "Hello");
    cat.register_vacant("zh-CN", "hello", "你好");
    cat.register_vacant("en-US", "hello", "Hi");
    cat.register_vacant("zh-CN", "hello", "嗨");
    assert_eq!(
        cat.get(&Locale::parse("en-US").unwrap(), "hello"),
        Some("Hello")
    );
    assert_eq!(
        cat.get(&Locale::parse("zh-CN").unwrap(), "hello"),
        Some("你好")
    );
    assert_eq!(cat.get(&Locale::en(), "hello"), Some("Hello"));
}
