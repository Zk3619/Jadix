# @jadix/shell

Local ESM/TypeScript SDK for the Jadix component wire contract. It shares the
product schema with the Rust validator and supports Box, Text, Button, Input,
Scroll, Textarea, Checkbox, Switch, Select, Image and DialogContent descriptions.
The package is private and is not published to npm.

Use the automatic JSX runtime with `jsxImportSource: "@jadix/shell"` and import
components from `@jadix/shell`. Direct function calls and TSX produce the same
description. State and event references are host-exported handles; session, root,
request and handle identities are positive decimal strings, including values
larger than JavaScript's safe integer range.

The application supplies the transport to `createClient(transport, session)`.
The client can submit a component tree, write an existing state handle, deliver a
typed event or call/cancel a host method. The Rust host validates the message,
component schema, root lifetime, permissions and handles before acting on it.
The transport's returned promise is the response; this SDK does not install a
renderer or a network bridge.

Run `npm ci`, then `npm run generate`, `npm run typecheck` and
`npm run build:fixture` in this directory. The generator consumes
`../../jadix_components/schema/{portable,extended}-v1.json`. TypeScript 6.0.3 and esbuild
0.28.2 are pinned in the lockfile. The fixture builds neutral and browser ESM
targets with source maps; native delivery uses source modules rather than private
VM bytecode. Node 22 or newer is the development tool baseline.

String JSX keys are limited to 256 UTF-8 bytes. Rust `native::NativeMount` validates
sibling uniqueness and retains keyed identities through reordering. The Windows
product in `examples/native-controls/shell` builds with `node build.mjs`; use
`--check` to compare the generated bundle, source hashes and development signatures.
Reactive effects, automatic JavaScript callback registration and GPU/DOM renderers
remain separate work. Host validation remains authoritative for style, resource
and permission limits. Only trusted first-party code is in scope.


`createApplication().patch(...)` replaces props on existing Text, Button, Input,
or Textarea leaves, preserving structure, styles and event handles. Editor props
use the same controlled revision and composition-preview rules as a complete
mount. Supply the complete props for that leaf; omitted optional properties take
their default values. Failed host publication leaves the packet queued for retry.
The character host stages only addressed editors and their budget deltas. Shared
layout reuses geometry only when every retained measurement remains equivalent;
changed intrinsic measurements still require layout/frame publication. Stateful
GPU editor updates currently take the existing complete projection fallback.
This API does not promise O(k) drawing, structure changes or GPU frame output.
