# Complete public API inventory, pinned snapshot

`python crates/jadix_2d/evidence/round17/build-api.py` snapshots current Jadix
public APIs with strict rustdoc warnings. `scan.py` reads the original docs.rs rustdoc JSON snapshots; `report.py` checks
that every Vello-owned public entry receives an explicit mapping and generates
`../../docs/vello-jadix-api-comparison.md`. `mapping.json` contains reviewed
high-level operation mappings; low-level and shared dependency inventories retain
their different authority. Unclassified Vello entries cause generation to fail.

```powershell
python benchmarks/vello-api/scan.py
python benchmarks/vello-api/report.py
```

Jadix's current rustdoc command/target scope is recorded in
`../../crates/jadix_2d/evidence/round17/api-build.json`. Regenerate that snapshot
when changing source, feature sets or Rust toolchain before running the scanner.
The local JSON uses RUSTC_BOOTSTRAP only to request rustdoc's unstable JSON output;
production code and tests do not require a nightly compiler.

Pinned inventories: Vello 0.11, Peniko 0.6.1, Color 0.3.3, Kurbo 0.13.1,
vello_encoding 0.11, linebender_resource_handle 0.1.1, wgpu/wgpu-types 30.0.0.
Official JSON source URL pattern: https://docs.rs/crate/NAME/VERSION/json .
Each inventory retains the input SHA-256, original signature, source span,
attributes, external reexports and non-blanket trait implementation names.
Conditional APIs reflect the downloaded docs.rs all-features target; std blanket
methods are not recursively expanded. This is a complete enumeration within
that declared boundary, not dynamic proof of all signatures on all platforms.

The three downloaded crate source trees retain their upstream notices/licenses.
*-source.json records the official crates.io archive URL/version/SHA-256.
Vello and Peniko use Apache-2.0 OR MIT; Color uses Apache-2.0 OR MIT. Individual
upstream files remain governed by their original notices. The source trees are
reference artifacts, not Jadix production dependencies or modified forks.
