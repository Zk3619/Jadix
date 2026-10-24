# Semantic document viewing and editing

`DocumentView` and `DocumentEditor` share a measured, variable-height document
viewport. They do not build a Textarea for each line or route Markdown through
form-control chrome. `TextEditor` reuses the native IME, caret, selection and
soft-wrapping implementation below Input/Textarea. Existing form controls and
CodeEditor keep their current behavior.

## Live source reveal

```rust
use jadix::prelude::*;

let state = DocumentEditorState::new("# Title\n\n**Edit this** and $x^2$.");
let editor: Element = document_editor("article", state.clone())
    .viewport_h(600.0)
    .into();
```

In `LivePreview`, clicking a rendered source line reveals its original syntax
in place, while other lines retain target formatting. Moving the caret to
another source line re-renders the previous line. Tables, fenced/indented code,
HTML blocks, mathematical equations, chemical equations and molecular-data
fences reveal their complete source block. `DocumentBlock::edit_range` records
the enclosing syntax group separately from each virtualized display row.

`Source` edits the entire original source in the native text engine. `Preview`
does not expose an editable source region. Switch with `state.set_mode(...)`.
Escape finishes the active edit. New source is committed through the same
native text/IME path; uncommitted IME preedit never changes the parsed document.

`DocumentEditorState` owns document-coordinate selection, search, replacement,
format commands and history, rather than assigning these to mounted rows:

```rust
state.set_selection(DocumentSelection { anchor: 0, caret: 7 })?;
state.toggle_mark(DocumentMark::Bold)?;
state.find_next("needle", false, true);
state.replace_all("old", "new")?;
state.undo();
state.redo();
# Ok::<(), jadix::text::DocumentEditError>(())
```

The native editor command hook handles document-wide undo/redo and select-all,
format shortcuts, and navigation/deletion at revealed-source boundaries.
Shortcut modifiers follow the existing platform Ctrl/Cmd policy. The hook is
shared by primary and secondary windows and is removed on unmount.

## Structured rich text

Markdown keeps source syntax. Rich text instead has styled runs and structured
blocks, so applying bold does not insert asterisks into visible text:

```rust
let document = Document::rich_text([
    RichTextBlock::paragraph([
        RichTextSpan::styled("Important", DocumentInlineStyle {
            bold: true,
            color: Some([190, 20, 40, 255]),
            ..Default::default()
        }),
        RichTextSpan::new(" text"),
    ]),
]);
let saved = serde_json::to_string(&document.rich_text_blocks())?;
```

Rich text uses plain source coordinates plus style metadata. Editing preserves
surviving runs and inherits style at insertion boundaries. `RichTextSpan::math`
retains a typed inline formula; `with_inline_kind` supports typed image/math
runs. Math, chemistry and molecule block kinds retain multiline atomic source.
Formatting and text
edits restore their styles on undo. Plain-text source is not a substitute for
the structured JSON when saving rich text. This is not an HTML DOM editor or
an arbitrary HTML/CSS round-trip implementation.

## Renderers and features

- CommonMark parsing uses pulldown-cmark with source offsets, tables, tasks,
  footnotes, strikethrough and mathematical notation. Source is never silently
  normalized. Code fences are indexed by logical line before UI virtualization.
- `document-math` enables native LaTeX-Rust/STIX math layout and the existing SVG
  rendering path with a two-times supersampling factor above window DPI.
  Formula textures are refreshed when DPI changes. Fractions, scripts, radicals and supported environments are
  genuinely typeset, not simulated with Unicode substitutions. The feature is
  optional to preserve Jadix's lean default configuration. Disabled features,
  unsupported commands and oversized formulas produce visible diagnostics.
- `chem` / `chemistry` fences render formula counts, grouped formulas, explicit
  `^charges` and reaction arrows through native math layout. This is a documented
  chemical-equation subset, not complete mhchem or a reaction-balancing engine.
- `xyz`, `pdb` and `molecule` fences provide dedicated molecular graphics.
  `molecule` uses the typed JSON schema `MoleculeData`; atoms and bonds are not
  text UI nodes. PDB loading uses the first model, ATOM/HETATM coordinates and
  explicit CONECT bonds. XYZ, and PDB without connectivity, infer single bonds
  from covalent radii and distances using spatial cells. Inferred connections
  carry `MoleculeBond::inferred`; unknown elements are not guessed. Dense input
  has a comparison budget. This does not infer bond orders, polymer topology or
  secondary structure. MDL MOL is not silently treated as XYZ.
- `MoleculeView` projects atomic coordinates in a native ball-and-stick canvas;
  shaded atoms use element-dependent radii; drag rotates and double-click resets.
  In `DocumentEditor`, dragging remains available and double-click reveals source.
  A camera signal controls rotation/zoom.
  This is not a chemistry drawing tool or a protein surface/trajectory engine.
- A `block_renderer` callback replaces or extends individual semantic nodes,
  without embedding scientific graphics into the text editor itself.

Markdown local images resolve against an explicit `asset_root` (the file's parent
directory in Jadix MD). HTTPS images use the sandboxed HTML renderer with `webview`.
Protein PDB blocks with a valid alpha-carbon backbone render as shaded ribbon
surfaces, honoring chain breaks and HELIX/SHEET annotations; small molecules and
XYZ blocks keep the ball-and-stick renderer. Unsupported specialized nodes
retain source and display a diagnostic instead of inventing a render.

## Full HTML documents

`webview` enables `html_document_view(id, html)` and
`html_document_editor(id, html_signal)`. These use the existing system WebView
engine for full HTML DOM/CSS layout, including nested blocks, style sheets,
tables with merged cells, images, links, lists and inline formatting. Native
`Document::rich_text` remains a separate structured-run format; converting arbitrary
HTML into that smaller schema would lose information.

The imported document lives in a scriptless sandboxed iframe. Parent-owned
editing controls and DOM input events export the complete HTML document through
an opt-in host message handler; ordinary WebViews retain disabled host messages.
Styles and HTTPS/data images/fonts are supported, subject to browser/CSP policy.
The `_with_assets` variants embed bounded local image resources from an explicit
asset root and restore their original `src` values when exporting. User scripts/forms/navigation are not
part of the document editing contract. The exported DOM is normalized HTML,
not a byte-for-byte reproduction of the original source. Only initial editor
content is read from the signal; remount with a new id to load another document.

Windows WebView2 and macOS/Linux-X11 Wry have the host message bridge. Other
WebView hosts need their own message bridge before editor state export is supported.
HTML layout/scrolling belongs to the browser and does not inherit the native
Fenwick block viewport's complexity guarantees. Markdown HTML blocks use
content-height browser regions; double-click reveals the entire source block
in `DocumentEditor`. Inline HTML mixes with Markdown through the CommonMark
HTML writer, preserving its enclosing paragraph, list or table as the edit unit.
Separate HTML fragments do not share a global CSS cascade or native block
virtualization within one very large HTML fragment.

## Large-document contracts

Snapshot clones are O(1). Initial Markdown parsing/indexing is O(document size).
Simple isolated-line edits reuse unaffected semantic blocks and only reparse the
changed line. Ordinary fenced-code text edits retain unaffected row identities
and group ranges, refreshing the fence's shared lexical payload so later rows
do not retain stale highlighting. Structural edits and reference-sensitive cases reparse the source
for correctness. The current source snapshot is contiguous UTF-8, not a Rope:
replacements still copy/reindex source, and enormous files need measurement
before selecting a different buffer implementation.

The viewport creates UI nodes only around the visible interval. A Fenwick height
index supports O(log n) height updates and offset lookup. Actual measured row
heights replace estimates, with scroll-anchor restoration. Code fences with
100,000 lines remain line-addressable rather than one enormous UI subtree.

This is not a promise of fully incremental CommonMark parsing, horizontal glyph
virtualization, unlimited individual-line size, globally exact initial scroll
geometry, full-document rich clipboard interoperability, or IDE language
services. Programmatic document selections/search can address unmounted text;
native source selection is used for active editing. `DocumentView` enables the
existing per-text-node selection/copy behavior by default (`selectable(false)`
disables it). Cross-block preview drag selection and
screen-reader parity require dedicated acceptance testing.

The local GPUI Kit design informed state/view separation and source-aware
presentation. Jadix retains its own native event, layout, Glyphon and canvas
systems; no GPUI runtime dependency is introduced.

## Demonstration

```text
cargo run -p jadix --example document_editor --features document-math,text-undo
cargo run -p jadix_md
cargo run -p jadix_ide
```

Run `document_editing` integration tests for source reveal, native-store commits,
unmounted-document editing/search, rich-style history and large-code viewport
behavior. Native window/IME accessibility acceptance should also be performed
on each supported platform; headless structural tests do not replace it.

Windows/DX12 smoke captures verified the rendered document, clicked-line source
reveal, Preview/Source switching, formula/chemistry images and molecular canvas.
IME regression tests exercise committed/preedit stores and document history;
they do not certify an operating-system IME candidate-window session. Mixed
text/object inline layout currently wraps between runs; full typographic inline
flow and cross-block preview selection remain follow-up work.

## Development follow-ups

Future model and pagination directions are maintained in [the development notes](../../development-docs/engine/document-model-followups.md).
These are design directions, not currently available component APIs.
