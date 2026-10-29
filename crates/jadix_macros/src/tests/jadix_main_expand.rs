use super::*;

fn compact(tokens: proc_macro2::TokenStream) -> String {
    tokens
        .to_string()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

#[test]
fn main_fn_last_expr_goes_to_launch() {
    let tokens = expand_jadix_main(
        quote! {},
        quote! {
            fn main() {
                setup();
                ui! { <App></App> }
            }
        },
    )
    .unwrap();
    let s = compact(tokens);
    assert!(s.contains("setup();"), "{s}");
    assert!(s.contains("::jadix::launch"), "{s}");
    assert!(s.contains("ui!"), "{s}");
    assert!(s.contains("<App></App>"), "{s}");
}

#[test]
fn main_attr_window_uses_app_mount() {
    let tokens = expand_jadix_main(
        quote! { title = "Hi", font_size = 18 },
        quote! {
            fn main() {
                ui! { <Button>"x"</Button> }
            }
        },
    )
    .unwrap();
    let s = compact(tokens);
    assert!(s.contains("::jadix::app::app()"), "{s}");
    assert!(s.contains(".title(\"Hi\")"), "{s}");
    assert!(s.contains(".font_size(18)"), "{s}");
    assert!(s.contains(".mount("), "{s}");
}

#[test]
fn component_root_generates_main_and_default_props() {
    let tokens = expand_jadix_main(
        quote! {},
        quote! {
            #[component]
            fn App() -> Element {
                el
            }
        },
    )
    .unwrap();
    let s = compact(tokens);
    assert!(s.contains("fnmain()"), "{s}");
    assert!(s.contains("Default::default()"), "{s}");
    assert!(s.contains("::jadix::launch"), "{s}");
}
