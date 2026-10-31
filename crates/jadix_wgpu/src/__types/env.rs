use alloc::string::String;

/// No-std friendly version of `std::env::var`. Returns `None` if the environment variable is not set
/// or we are in a no-std context.
pub fn var(_key: &str) -> Option<String> {
    #[cfg(jadix_wgpu_types_feature_std)]
    return std::env::var(_key).ok();

    #[cfg(not(jadix_wgpu_types_feature_std))]
    return None;
}
