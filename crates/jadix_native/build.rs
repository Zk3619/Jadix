use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=platforms/ohos/native.cpp");
    for name in ["JADIX_OHOS_SDK", "OHOS_NDK_HOME"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("ohos")
        || env::var_os("CARGO_FEATURE_OHOS").is_none()
    {
        return;
    }
    let sdk = PathBuf::from(
        env::var_os("JADIX_OHOS_SDK")
            .or_else(|| env::var_os("OHOS_NDK_HOME"))
            .expect("set JADIX_OHOS_SDK to one selected OpenHarmony SDK"),
    );
    let native = if sdk.join("llvm").is_dir() {
        sdk
    } else {
        sdk.join("native")
    };
    let target = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "aarch64-linux-ohos",
        Ok("x86_64") => "x86_64-linux-ohos",
        _ => panic!("the Native OHOS host currently supports aarch64 and x86_64"),
    };
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let object = out.join("jadix_native_ohos.o");
    let archive = out.join("libjadix_native_ohos.a");
    let result = Command::new(native.join(format!("llvm/bin/clang++{suffix}")))
        .arg(format!("--target={target}"))
        .arg(format!("--sysroot={}", native.join("sysroot").display()))
        .args([
            "-std=c++17",
            "-fPIC",
            "-fvisibility=hidden",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-D__OHOS_API__=20",
            "-c",
            "platforms/ohos/native.cpp",
            "-o",
        ])
        .arg(&object)
        .status()
        .expect("start selected OHOS clang++");
    assert!(
        result.success(),
        "OHOS NativeNode bridge compilation failed"
    );
    let result = Command::new(native.join(format!("llvm/bin/llvm-ar{suffix}")))
        .arg("rcs")
        .arg(&archive)
        .arg(&object)
        .status()
        .expect("start selected OHOS llvm-ar");
    assert!(result.success(), "OHOS NativeNode bridge archive failed");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!(
        "cargo:rustc-link-search=native={}",
        native.join("sysroot/usr/lib").join(target).display()
    );
    for library in [
        "static=jadix_native_ohos",
        "dylib=ace_ndk.z",
        "dylib=native_drawing",
        "dylib=c++_shared",
    ] {
        println!("cargo:rustc-link-lib={library}");
    }
}
