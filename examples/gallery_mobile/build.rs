fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        println!("cargo:rerun-if-env-changed=ANDROID_NDK_HOME");
        let ndk = std::path::PathBuf::from(
            std::env::var_os("ANDROID_NDK_HOME").expect("ANDROID_NDK_HOME"),
        );
        let host = if cfg!(windows) {
            "windows-x86_64"
        } else if cfg!(target_os = "macos") {
            "darwin-x86_64"
        } else {
            "linux-x86_64"
        };
        let toolchain = ndk.join("toolchains/llvm/prebuilt").join(host);
        let architecture = std::env::var("CARGO_CFG_TARGET_ARCH").expect("target architecture");
        let (triple, clang_architecture) = match architecture.as_str() {
            "aarch64" => ("aarch64-linux-android", "aarch64"),
            "x86_64" => ("x86_64-linux-android", "x86_64"),
            other => panic!("unsupported Android architecture: {other}"),
        };
        let libraries = toolchain.join("sysroot/usr/lib").join(triple);
        println!(
            "cargo:rustc-link-search=native={}",
            libraries.join("28").display()
        );
        for version in std::fs::read_dir(toolchain.join("lib/clang"))
            .expect("NDK clang versions")
            .flatten()
        {
            let unwind = version.path().join("lib/linux").join(clang_architecture);
            if unwind.join("libunwind.a").is_file() {
                println!("cargo:rustc-link-search=native={}", unwind.display());
            }
        }
    }
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("ohos") {
        napi_build_ohos::setup();
        println!("cargo:rustc-link-lib=dylib=c++_shared");
    }
}
