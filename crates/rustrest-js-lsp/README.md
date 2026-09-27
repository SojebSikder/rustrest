# rustrest-js-lsp

A small, pure-Rust language server for the JavaScript in Rustrest's
pre-request / post-response scripts. It knows the `pm` / `console` API that
`rustrest-core`'s script engine registers and provides:

- completion (trigger `.`) for `pm`, scope objects, `pm.response`, the
  `pm.response.to.*` chain, `pm.expect(...)` assertions, common JS globals,
  keywords and locally declared names
- hover (plain text: signature, blank line, description), falling back to
  the enclosing call's signature when the cursor is inside its arguments
- signature help (triggers `(` and `,`)
- diagnostics (`source: "rustrest-js"`): syntax errors (oxc), unknown members
  on closed API objects (`pm.enviroment`), and members unavailable in the
  current script kind (`pm.test` in a pre-request script)

## Running

```sh
cargo install --path crates/rustrest-js-lsp
```

puts `rustrest-js-lsp` on `PATH`; the js-lsp plugin prefers a binary found on
`PATH` over downloading a release. The server speaks LSP over stdio and takes
no arguments; logs go to stderr.

## Protocol notes

- Document URIs select the API: paths ending in `pre-request.js` or
  `post-response.js` (the app uses `file:///rustrest/scripts/<doc-id>/post-response.js`)
  get that script's `pm` members; any other URI sees the union of both.
- Position encoding: `utf-8` (byte columns) when the client lists it in
  `general.positionEncodings`, otherwise `utf-16`. The choice is returned in
  `ServerCapabilities.positionEncoding`.
- Text sync is full-document; diagnostics are published with the document
  version after every open/change, and cleared on close.

## Releases

This crate is a workspace member that the app never links. Like
`rustrest-remote-agent`, it inherits the workspace version and cargo-dist
builds it for every target whenever a Rustrest release tag (`v<version>`) is
pushed (`.github/workflows/release.yml`). The release gets one archive per
target plus a `.sha256` for each:
`rustrest-js-lsp-<target>.zip` (Windows) or `.tar.xz` (macOS/Linux). The
js-lsp plugin's `SERVER_VERSION` picks which release it downloads.

Build and test it from this directory:

```sh
cargo test
cargo build --release
```
