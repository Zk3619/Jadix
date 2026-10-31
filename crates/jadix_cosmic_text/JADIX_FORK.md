# Jadix cosmic-text fork

Package version: `0.1.0`.

Upstream: cosmic-text 0.19.0, commit `c24886c2471e5606587c46090cd25dbbf209186b`.
Source: the locally cached crates.io release, with its original MIT/Apache-2.0 licenses.

This fork adds `AlignLast`, `BufferLine::align_last` / `set_align_last`, and
`ShapeLine::layout_to_buffer_with_last_alignment`. The existing layout API remains
available and retains its behavior when no final-line override is supplied.

The override applies only to the final visual line of each paragraph, including
lines before a hard line break. `start` and `end` resolve using the paragraph's
BiDi direction. `justify` expands whitespace on that final line. Changing the
override invalidates layout while preserving shaping, and resetting a buffer
line clears it. No phantom characters or additional visual lines are inserted.

The `jadix_cosmic_text` package is selected through explicit versioned path
dependencies, including glyphon's dependency, so consumers outside the root
workspace use the same implementation without a root-only Cargo patch.
`BidiParagraphs::next_with_direction` exposes the existing paragraph direction
alongside its text. Jadix uses this to retain the original paragraph boundaries
and direction when its BiDi wrapping path inserts soft line breaks.

Regression coverage is in `crates/jadix/tests/css_alignment.rs` and the
`text_alignment_tests` module in `crates/jadix/src/renderer/prepare_text.rs`.
