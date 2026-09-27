# rustrest-plugin-js-lsp

A Rustrest plugin that adds JavaScript language support to the
pre-request / post-response script editors:

- autocomplete (including the `pm` API),
- hover docs,
- error checking (diagnostics as you type).

It works like a Zed extension: the plugin only declares the
[`rustrest-js-lsp`](../rustrest-js-lsp) language server (a Rust server, no
Node.js needed) and tells the app how to launch it. The app runs the server
and its own LSP client (`rustrest-lsp`) drives the editors.

## Build

```sh
cargo build --release --target wasm32-unknown-unknown
```

Put these two files into one folder:

- `plugin.toml`
- `target/wasm32-unknown-unknown/release/rustrest_plugin_js_lsp.wasm`, renamed to `plugin.wasm`

Then install it via **Manage Plugins -> Install from folder**.

Run the tests natively with `cargo test`.

## Where the server comes from

When the app asks for the launch command (`language_server_command`):

1. `rustrest-js-lsp` on `PATH` wins. During development:
   `cargo install --path crates/rustrest-js-lsp`.
2. Otherwise a copy this plugin downloaded before (its path is kept in the
   plugin's storage as `server-path.txt`, next to `server-version.txt`),
   if its version matches the plugin's `SERVER_VERSION`.
3. Otherwise the plugin downloads the release for the host's target and checks
   it against the published `.sha256`:
   `https://github.com/SojebSikder/rustrest/releases/download/v<version>/rustrest-js-lsp-<target>.<zip|tar.xz>`
   It's extracted into the plugin's storage (`bin/`). Until then the plugin
   answers "not ready yet" and the app asks again shortly.

If the download fails, the error shows in the console. Run
**JavaScript Language Server: Reinstall** to download it again, and
**Restart Language Server** (built into the app) to relaunch the server.
