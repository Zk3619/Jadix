use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=gallery.rc");
    println!("cargo:rerun-if-changed=../../assets/icons/desktop/app.ico");
    println!("cargo:rerun-if-env-changed=RC");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let msvc = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let compiler = env::var_os("RC").unwrap_or_else(|| {
        if msvc {
            find_msvc_tools::find_windows_sdk(env::consts::ARCH)
                .and_then(|sdk| {
                    sdk.path()
                        .map(|directory| directory.join("rc.exe"))
                        .find(|path| path.is_file())
                })
                .map(PathBuf::into_os_string)
                .unwrap_or_else(|| if cfg!(windows) { "rc.exe" } else { "llvm-rc" }.into())
        } else {
            "windres".into()
        }
    });
    let output =
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory")).join(if msvc {
            "gallery-icon.res"
        } else {
            "gallery-icon.o"
        });
    let mut command = Command::new(compiler);
    if msvc {
        command
            .arg("/nologo")
            .arg("/fo")
            .arg(&output)
            .arg("gallery.rc");
    } else {
        command
            .args(["--input", "gallery.rc", "--output-format=coff", "--output"])
            .arg(&output);
    }
    let status = command
        .status()
        .expect("run Windows resource compiler (override with RC)");
    assert!(
        status.success(),
        "Gallery icon resource compilation failed: {status}"
    );
    println!("cargo:rustc-link-arg-bin=gallery={}", output.display());
}
