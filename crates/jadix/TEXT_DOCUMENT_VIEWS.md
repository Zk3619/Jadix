# Text document views

For semantic documents, rich text and in-place live editing, see
[DOCUMENT_EDITING.md](DOCUMENT_EDITING.md). It adds `DocumentView`,
`DocumentEditor`, mathematical rendering and native molecular graphics on top
of the shared source and viewport infrastructure described here.

This is the first implementation stage, not a replacement for the existing
Input, Textarea or experimental CodeEditor. All three retain their current
value signals, IME and undo behavior.

## Available surfaces

- `TextDocument`: immutable shared UTF-8 snapshot, logical line index, revision
  identity, byte-boundary validation and replacement producing a new snapshot.
- `TextView` / `text_view`: read-only fixed-height logical lines, optional line
  numbers, horizontal/vertical scrolling and viewport-only row materialization.
- `DiffDocument`: precomputed line diff, exact source coordinates and unified
  or aligned split rows; row text borrows from the two source snapshots.
- `DiffView` / `diff_view`: virtualized unified or side-by-side presentation.
  The split sides share one scrollport, rather than synchronizing two lists.

```rust
use jadix::prelude::*;

let original = TextDocument::new("first\nsecond\n");
let modified = original.replace(6..12, "changed").unwrap();
let text: Element = text_view("source")
    .document(original.clone())
    .line_numbers(true)
    .viewport_h(320.0)
    .into();
let diff: Element = diff_view("comparison", DiffDocument::new(original, modified))
    .mode(DiffMode::Split)
    .viewport_h(320.0)
    .into();
```

`TextView` also accepts `text` in RSX. Prefer publishing `Signal<TextDocument>`
with `document_signal` for changing documents. `DiffView` similarly accepts
`Signal<DiffDocument>`. Precompute a new diff before publishing it: never
construct a diff in a paint callback or an initializer run on every UI update.
Publishing with `set` schedules a redraw through a view-owned effect without
subscribing the whole root UI. That effect is disposed on unmount. `set_silent`
does not wake an idle view and requires an independently scheduled frame.

Run the 100,000-line example with
`cargo run -p jadix --example text_documents`.

## Coordinate and performance contracts

Document line indices and diff row source indices are zero-based; displayed
line numbers are one-based. Offsets and edit ranges are UTF-8 bytes. The line
index recognizes LF and CRLF. A trailing LF creates an empty TextView line,
but does not create a phantom diff change. Diff comparisons retain terminators,
so CRLF changes and missing final newlines are not silently normalized.

Snapshot clones and snapshot-identity equality are O(1); indexing and current
replacement are O(bytes + lines). Equality is not a full text comparison.
This is deliberately **not** a Rope editor. View construction materializes only
visible rows plus overscan; scrolling does not rescan or clone the whole source.
Same-line-count replacements invalidate visible rows using revision identity.
Horizontal extent is a conservative byte/tab-based estimate, not full-document
glyph measurement. A very long individual line is still shaped as one line;
horizontal glyph virtualization and enormous-scroll-coordinate rebasing are
not implemented. These limitations must not be reported as solved by row
virtualization alone.

Diffs use the existing ecosystem `similar` Patience implementation with a
200 ms algorithm budget on native targets. The current dependency configuration
does not enforce that deadline on WebAssembly. Deadline fallback may yield a coarser valid diff;
the deadline does not bound source indexing, allocation or total construction
time. Build large diffs off the UI thread and publish the resulting snapshot.

Selection currently reuses Label's **per-line** selection and copy behavior.
There is no cross-line drag selection, document-wide select-all, editing,
syntax highlighting, patch import, hunk collapsing, merge or conflict resolution
in these new views. Diff source text excludes line numbers and change markers
when copied. Change color is supplemented by `+` / `-` in both modes;
split rows expose `data-change`, source-line and line-ending attributes.

## Development follow-ups

Future model and pagination directions are maintained in [the development notes](../../development-docs/engine/document-model-followups.md).
These are design directions, not currently available component APIs.
