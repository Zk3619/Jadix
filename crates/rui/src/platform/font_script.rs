//! Which lazy system-font bucket `font_loader` should open.
//!
//! Data only — loading, `ScriptNeeds`, and bundled faces stay in `renderer/`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SystemFontScript {
    Cjk,
    Hangul,
    Thai,
    Emoji,
    Symbols,
    Arabic,
    Devanagari,
}
