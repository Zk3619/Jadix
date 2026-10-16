fn main() {
    // Build scripts run on the host; inspect Cargo's target instead of cfg!(ohos).
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("ohos") {
        napi_build_ohos::setup();
        println!("cargo:rustc-link-lib=dylib=c++_shared");
    }
}
