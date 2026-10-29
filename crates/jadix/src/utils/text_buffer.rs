//! Replace protocol for undo / a future editor buffer.
//!
//! P3 ships [`StringBuffer`] only. `text-undo-rope` stays off: L1/L2 forms stay
//! on `String`. Re-run `cargo test -p jadix --lib --ignored text_buffer_mid_insert -- --nocapture`
//! before enabling a rope feature — see `docs/plans/text_undo_redo.md` §9.3.
//!
//! Live Input/Textarea values stay `Signal<String>`. Undo apply goes through this trait
//! so a later rope impl can slot in without forking `input_controller`.

use std::borrow::Cow;
use std::ops::Range;

/// UTF-8 **byte** ranges on a char boundary (same contract as caret).
pub trait TextBuffer {
    type Slice<'a>: AsRef<str> + Clone
    where
        Self: 'a;

    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn text(&self) -> Cow<'_, str>;
    fn slice(&self, range: Range<usize>) -> Self::Slice<'_>;
    fn replace(&mut self, range: Range<usize>, insert: &str);
}

/// Heap `String` buffer. `slice` borrows; history still owns `String` copies in `TextChange`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringBuffer {
    inner: String,
}

impl StringBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_string(s: String) -> Self {
        Self { inner: s }
    }

    pub fn into_string(self) -> String {
        self.inner
    }

    pub fn as_str(&self) -> &str {
        &self.inner
    }
}

impl From<String> for StringBuffer {
    fn from(inner: String) -> Self {
        Self { inner }
    }
}

impl From<&str> for StringBuffer {
    fn from(s: &str) -> Self {
        Self {
            inner: s.to_string(),
        }
    }
}

fn clamp_char_range(s: &str, range: Range<usize>) -> Range<usize> {
    let start = range.start.min(s.len());
    let end = range.end.min(s.len()).max(start);
    if !s.is_char_boundary(start) || !s.is_char_boundary(end) {
        return start..start;
    }
    start..end
}

impl TextBuffer for StringBuffer {
    type Slice<'a> = &'a str;

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn text(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.inner)
    }

    fn slice(&self, range: Range<usize>) -> Self::Slice<'_> {
        let range = clamp_char_range(&self.inner, range);
        &self.inner[range]
    }

    fn replace(&mut self, range: Range<usize>, insert: &str) {
        let range = clamp_char_range(&self.inner, range);
        self.inner.replace_range(range, insert);
    }
}

impl TextBuffer for String {
    type Slice<'a> = &'a str;

    fn len(&self) -> usize {
        String::len(self)
    }

    fn is_empty(&self) -> bool {
        String::is_empty(self)
    }

    fn text(&self) -> Cow<'_, str> {
        Cow::Borrowed(self)
    }

    fn slice(&self, range: Range<usize>) -> Self::Slice<'_> {
        let range = clamp_char_range(self, range);
        &self[range]
    }

    fn replace(&mut self, range: Range<usize>, insert: &str) {
        let range = clamp_char_range(self, range);
        self.replace_range(range, insert);
    }
}

#[cfg(test)]
#[path = "tests/text_buffer.rs"]
mod tests;
