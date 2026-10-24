# Document products and format adapters

The framework owns reusable text buffers, selection, transactions, history,
virtual viewports, vector/image primitives, accessibility and input methods.
An application owns file formats, user preferences, persistence and workflows.

| Product | Editable model | Product-level adapters |
| --- | --- | --- |
| Word / DOCX | Tree-structured document plus a paginated layout model | DOCX import/export, styles, sections, headers/footers, fields, compatibility policies |
| PowerPoint / PPTX | Slide scene graph: shapes, text frames, masters and ordered slides | PPTX relationships, layouts/themes, presentation mode and media |
| Excel / XLSX | Sparse workbook, typed cells, formulas, dependencies and recalculation | XLSX styles, shared strings, dates, charts, validation and compatibility |
| PDF | Fixed-page display lists, annotations, text/geometry extraction | PDF parser/renderer/exporter and incremental annotation saves |

`PagedDocumentView` / `PagedDocumentEditor` should implement reusable page layout
and page-aware editing, not DOCX package parsing. Word's product layer reuses that
module. These names describe the intended boundary, not existing public APIs:
there is currently no `PagedDocument` implementation in this repository.
The current Markdown-oriented flat `Document` is not yet a Word schema:
paragraph/run properties, nested tables, structural selection and anchoring must
be added before claiming Word-compatible editing.

PPTX is not a paginated prose document, XLSX is not a table-shaped rich-text
document, and PDF is not a lossless editable Word model. Share low-level text
layout and drawing, not a universal document enum containing every product.
PDF reflow or PDF-to-Word conversion is an explicit lossy transformation. Avoid
promising visual fidelity until round-trip fixtures cover the chosen subset.

Recommended order is a tree-shaped rich document and structural transactions,
then reusable pagination (page metrics, flow, tables, footnotes, headers/footers),
then product-specific import/export. Add a format only with fixtures for both
directions and a written unsupported-feature policy. PDF can first be a viewer
and annotator; XLSX recalculation is an independent engine, not a text widget.

None of DOCX/PPTX/XLSX/PDF parsing, Office-compatible pagination/recalculation,
or format conversion is implemented by the example editors in this change.
