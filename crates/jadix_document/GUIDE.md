# jadix_document

Immutable UTF-8 snapshots, checked edits and variable-height block indexing without a UI runtime, renderer, window or procedural macro dependency.

```rust
use jadix_document::TextDocument;

let original = TextDocument::new("hello\nworld");
let edited = original.replace(6..11, "Jadix").unwrap();
assert_eq!(edited.text(), "hello\nJadix");
assert_eq!(original.text(), "hello\nworld");
```

The optional `markdown` feature provides CommonMark blocks, table alignments (`DocumentAlignment`), source-preserving inline styles and document edit models; `document-editing`, `document-math` and `science` select the related document capabilities. Element/TextSpan conversion, text measurement and editor widgets remain host adapters.

The optional `syntax` feature adds GUI-independent colored spans through `syntect`; the framework alone adapts these spans to UI text styles and maintains the UI line cache.

The optional `diff` feature enables precomputed unified/split diffs through `similar`. Default builds have no dependencies. Jadix keeps its existing `text` paths as re-exports of these same types; no second snapshot counter or duplicate state model is created.

For source distribution, copy `src/` and `LICENSE` into a local crate, retaining the module layout. The base needs only std; enabled features need the dependencies and configuration declared in `Cargo.toml` (including pulldown-cmark/serde for Markdown, similar for diff and syntect for syntax). Keep only the features your application actually enables. Crate consumption is recommended for routine updates.
