// Copyright 2026 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

#[cfg(target_env = "ohos")]
mod adapter;
#[cfg(any(target_env = "ohos", test))]
mod model;
#[cfg(target_env = "ohos")]
mod node;

#[cfg(target_env = "ohos")]
pub use adapter::Adapter;
#[cfg(target_env = "ohos")]
pub use ohos_accessibility_binding::{AccessibilityError, Provider};
