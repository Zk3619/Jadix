//! Shared utility-class catalog + variant parsing for `jadix` and `jadix_macros`.
//!
//! Compile-time `ui!` and runtime `apply_utility_classes` both go through this crate so
//! static `class="…"` and dynamic `class={sig.get()}` stay in lockstep.

#![forbid(unsafe_code)]

mod catalog;
mod variant;

pub use catalog::{
    classify_utility, is_known_utility, spacing_token_to_px, text_size_token_to_px,
    FontWeightToken, KnownUtility, TwLength,
};
pub use variant::{
    parse_class_list, parse_class_token, ClassToken, ParsedClassList, VariantSelector,
};
