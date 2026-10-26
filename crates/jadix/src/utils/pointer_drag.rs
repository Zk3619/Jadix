//! Compatibility re-exports for the pre-R1 pointer drag namespace.
//!
//! New code should use [`crate::utils::drag::pointer`] and
//! [`crate::utils::drag::routing`]. The implementation now lives under the
//! unified `utils::drag` namespace.

pub use crate::utils::drag::pointer::*;
pub use crate::utils::drag::routing;
