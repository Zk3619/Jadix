#[test]
fn translation_macro_uses_the_library_feature_configuration() {
    #[cfg(feature = "i18n")]
    {
        let locale = jadix::i18n::LocaleContext::new();
        locale.with(|| {
            jadix::i18n::set_locale("en");
            jadix::i18n::add_json("en", r#"{"greeting":"Hello %{name}","saved":"Saved"}"#).unwrap();
            assert_eq!(jadix::t!("saved"), "Saved");
            assert_eq!(jadix::t!("greeting", name = "Ada"), "Hello Ada");
        });
    }
    #[cfg(not(feature = "i18n"))]
    {
        assert_eq!(jadix::t!("saved"), "saved");
        assert_eq!(jadix::t!("Hello {name}", name = "Ada"), "Hello Ada");
    }
}
