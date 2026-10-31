#[cfg(jadix_wgpu_feature_std)]
pub fn is_panicking() -> bool {
    std::thread::panicking()
}

#[cfg(not(jadix_wgpu_feature_std))]
pub fn is_panicking() -> bool {
    false
}
