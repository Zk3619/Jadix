use super::*;

#[test]
fn parse_common_tags() {
    let zh = Locale::parse("zh-CN").unwrap();
    assert_eq!(zh.language(), "zh");
    assert_eq!(zh.region(), Some("CN"));
    assert!(zh.script().is_none());
    assert_eq!(zh.tag(), "zh-CN");

    let hans = Locale::parse("zh_Hans_CN").unwrap();
    assert_eq!(hans.script(), Some("Hans"));
    assert_eq!(hans.region(), Some("CN"));

    let en = Locale::parse("en").unwrap();
    assert_eq!(en, Locale::en());
    assert_eq!(Locale::parse("jp").unwrap().language(), "ja");
    assert_eq!(Locale::parse("kr").unwrap().language(), "ko");
    assert_eq!(Locale::parse("iw").unwrap().language(), "he");
    assert_eq!(Locale::parse("in").unwrap().language(), "id");
    assert_eq!(Locale::parse("ja-JP").unwrap().language(), "ja");
    assert!(Locale::parse("en-XA").unwrap().is_pseudo());
    assert!(Locale::parse("").is_none());
    assert!(Locale::parse("C").is_none());
}

#[test]
fn fallback_includes_language_and_en() {
    let chain = Locale::parse("zh-CN").unwrap().fallback_chain();
    let tags: Vec<String> = chain.iter().map(|l| l.tag()).collect();
    assert_eq!(tags[0], "zh-CN");
    assert!(tags.iter().any(|t| t == "zh"));
    assert!(tags.iter().any(|t| t == "zh-Hans" || t == "zh-Hans-CN"));
    assert_eq!(tags.last().map(String::as_str), Some("en"));
}

#[test]
fn unix_env_style_tags() {
    let loc = Locale::parse("pt_BR.UTF-8").unwrap();
    assert_eq!(loc.language(), "pt");
    assert_eq!(loc.region(), Some("BR"));
}
