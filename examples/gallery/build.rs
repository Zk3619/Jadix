fn main() {
    // Gallery mounts a large reactive tree; Windows default stack (~1MB) has overflowed
    // during startup (STATUS_STACK_OVERFLOW). Bump the PE stack reserve for this binary.
    #[cfg(target_os = "windows")]
    println!("cargo:rustc-link-arg=/STACK:16777216");
}
