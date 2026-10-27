# jadix_lsp

GUI-independent stdio LSP transport, file URIs, UTF-16 positions and text edits.
The server command is explicitly supplied by the application. Feedback runs on
transport threads; dispatch it to the application's own UI scheduler when needed.

The crate depends only on serde and serde_json, not Jadix, a window backend or
an editor component. `jadix::text::lsp` remains a compatibility re-export under
`code-lsp`. The implementation and its existing tests moved together.

For source distribution, copy `src/lib.rs` and `LICENSE` and declare serde (derive) plus serde_json. The same source can be embedded as a project module; server processes and UI scheduling remain the application's responsibility. Crate consumption is recommended for routine updates.
