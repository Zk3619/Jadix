fn main() {
    // napi-build-ohos lives in the gpui-template build.rs. Do not depend on it
    // here: this crate is a workspace member and must compile on desktop CI.
    // When targeting ohos (`ohrs build`), copy the template's build.rs:
    //   napi_build_ohos::setup();
    //   println!("cargo:rustc-link-lib=dylib=c++_shared");
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("ohos") {
        println!(
            "cargo:warning=harmonyos_host: add napi-build-ohos setup (see gpui-template build.rs)"
        );
    }
}
