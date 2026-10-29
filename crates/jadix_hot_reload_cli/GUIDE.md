# jadix_hot_reload_cli

Development watcher, source parser and transaction sender. Shares schema, capability signatures, finite template parsing and rebuild classification with `jadix_hot_reload`; uses the Lucide catalog for legacy compile-time icon text ordering.

Run `cargo run -p jadix_hot_reload_cli -- --run -- cargo run -p gallery` for managed development, or omit `--run` to watch an application started separately. Only the managed process tree is restarted. Supported template data, class/reset and finite structure changes wait for the UI ACK before advancing the sender cache. Lost ACK retries use the same transaction; build changes invalidate the old cache. `.keyframes.css`, local images and WGSL use resource transactions. Rust captures, uncompiled factories and unsupported topology report rebuild requirements.

Rejected drafts do not advance the compiled Rust baseline or the accepted template cache. In manually started applications, restoring the original captures and supported declarations permits live editing again without restarting. A rebuild requirement reported before sending a transaction is a watcher diagnostic, not an application ACK. Managed restarts advance their source baseline only after the command starts successfully.

This is a trusted-local debug tool. Standard Release applications do not start the receiver even when the framework `hot-reload` feature is selected. See the repository `docs/devtools.md` and `scripts/acceptance/check-hr-r3-acceptance.ps1` for the support matrix and verification.
