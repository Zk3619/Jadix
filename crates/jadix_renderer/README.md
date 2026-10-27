# jadix_renderer

GPU compute kernels, backdrop blur pipelines and emoji classification

The framework retains final drawing, surface ownership and GPU resource lifetimes; this crate owns the extracted rendering subsystems. This crate never depends on jadix or product crates. For source distribution, copy src and the dependencies and features from Cargo.toml into a local crate. Crate installation is recommended for updates.
