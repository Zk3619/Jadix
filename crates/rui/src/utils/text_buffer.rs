//! Replace protocol for undo / a future editor buffer.
//!
//! P3 ships [`StringBuffer`] only. `text-undo-rope` stays off: L1/L2 forms stay
//! on `String`. Re-run `cargo test -p rui --lib --ignored text_buffer_mid_insert -- --nocapture`
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
mod tests {
    use super::*;

    #[test]
    fn string_buffer_replace_insert_delete() {
        let mut b = StringBuffer::from("hello");
        b.replace(5..5, "!");
        assert_eq!(b.as_str(), "hello!");
        b.replace(0..1, "");
        assert_eq!(b.as_str(), "ello!");
        b.replace(0..4, "hi");
        assert_eq!(b.as_str(), "hi!");
        assert_eq!(b.slice(0..2), "hi");
        assert_eq!(b.text().as_ref(), "hi!");
    }

    #[test]
    fn string_impl_matches_buffer() {
        let mut s = String::from("ab");
        s.replace(1..1, "X");
        assert_eq!(s, "aXb");
    }

    /// Mid-buffer insert: `String` / [`StringBuffer`] vs `ropey::Rope`.
    /// Clone sits outside the timed window. ASCII so rope char index == byte index.
    ///
    /// ```text
    /// cargo test -p rui --lib --ignored text_buffer_mid_insert -- --nocapture
    /// ```
    #[test]
    #[ignore]
    fn text_buffer_mid_insert() {
        use std::hint::black_box;
        use std::time::Instant;

        use ropey::Rope;

        fn median_ns(samples: &[u128]) -> u128 {
            let mut v = samples.to_vec();
            v.sort_unstable();
            v[v.len() / 2]
        }

        println!(
            "text_buffer_mid_insert  host={}  profile=lib-test(debug unless --release)",
            std::env::consts::OS
        );
        println!(
            "{:<8} {:<6} {:<22} {:>12}",
            "size", "pay", "impl", "median_ns"
        );

        for &(size, iters) in &[(1usize << 20, 7usize), (10 << 20, 5), (50 << 20, 3)] {
            let base = "x".repeat(size);
            let mid = size / 2;
            let kb = "y".repeat(1024);
            for payload in ["y", kb.as_str()] {
                let pay_label = if payload.len() == 1 { "1B" } else { "1KB" };

                let mut string_ns = Vec::with_capacity(iters);
                for _ in 0..iters {
                    let mut s = base.clone();
                    let t0 = Instant::now();
                    s.insert_str(mid, payload);
                    string_ns.push(t0.elapsed().as_nanos());
                    black_box(s);
                }

                let mut buf_ns = Vec::with_capacity(iters);
                for _ in 0..iters {
                    let mut b = StringBuffer::from_string(base.clone());
                    let t0 = Instant::now();
                    b.replace(mid..mid, payload);
                    buf_ns.push(t0.elapsed().as_nanos());
                    black_box(b);
                }

                let rope_src = Rope::from_str(&base);
                let mut rope_ns = Vec::with_capacity(iters);
                for _ in 0..iters {
                    let mut r = rope_src.clone();
                    let t0 = Instant::now();
                    r.insert(mid, payload);
                    rope_ns.push(t0.elapsed().as_nanos());
                    black_box(r);
                }

                let row = |name: &str, samples: &[u128]| {
                    println!(
                        "{:<8} {:<6} {:<22} {:>12}",
                        format!("{}MB", size / (1 << 20)),
                        pay_label,
                        name,
                        median_ns(samples)
                    );
                };
                row("String::insert_str", &string_ns);
                row("StringBuffer::replace", &buf_ns);
                row("Rope::insert", &rope_ns);
            }
        }
    }
}
