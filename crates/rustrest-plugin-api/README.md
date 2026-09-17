# rustrest-plugin-api

[![Crates.io](https://img.shields.io/crates/v/rustrest-plugin-api.svg)](https://crates.io/crates/rustrest-plugin-api)
[![docs.rs](https://img.shields.io/docsrs/rustrest-plugin-api)](https://docs.rs/rustrest-plugin-api)

SDK for writing native plugins for [Rustrest](https://github.com/SojebSikder/rustrest), a cross-platform native API testing platform.

A Rustrest plugin is a small Rust crate compiled to `wasm32-unknown-unknown` and sandboxed with [`wasmtime`](https://wasmtime.dev/) by the host app. This crate provides the `Plugin` trait, the manifest types, and safe wrappers around the host functions a plugin is allowed to call (logging, spawning processes, downloading files, ...).

## Quick start

Add the dependency and target a `cdylib`:

```toml
[package]
name = "my-plugin"
version = "0.1.0"
edition = "2024"
publish = false

[lib]
name = "my_plugin"
crate-type = ["cdylib"]

[dependencies]
rustrest-plugin-api = "0.1"
```

`.cargo/config.toml` - required, or the final link step fails, since host functions like `log`/`download_file`/`process_spawn` are undefined symbols at compile time (resolved as wasm imports by the host at instantiation time):

```toml
[target.wasm32-unknown-unknown]
rustflags = ["-C", "link-arg=--import-undefined"]
```

`plugin.toml`, sitting next to `Cargo.toml`, declares who the plugin is and which capabilities it needs - the host reads this file directly, without running any guest code:

```toml
id = "my-plugin"
name = "My Plugin"
version = "0.1.0"
author = "Your Name"
description = "What it does."
schema_version = 1

[capabilities]
request_hooks = true

[[capabilities.commands]]
id = "say-hello"
title = "My Plugin: Say Hello"
```

`src/lib.rs` implements `Plugin` for a type and registers it once with `export_plugin!`:

```rust
use rustrest_plugin_api::{Plugin, RequestContext};

#[derive(Default)]
struct MyPlugin;

impl Plugin for MyPlugin {
    fn on_pre_request(&mut self, mut ctx: RequestContext) -> RequestContext {
        ctx.headers.push(("X-My-Plugin".to_string(), "1".to_string()));
        ctx
    }

    fn on_command(&mut self, command_id: &str) -> Result<Option<String>, String> {
        match command_id {
            "say-hello" => Ok(Some("Hello from my plugin!".to_string())),
            other => Err(format!("unknown command: {other}")),
        }
    }
}

rustrest_plugin_api::export_plugin!(MyPlugin);
```

Build it, then stage `plugin.toml` alongside the compiled wasm (renamed to `plugin.wasm`) in one folder:

```bash
cargo build --release --target wasm32-unknown-unknown
```

That folder is what you point **Install Plugin Folder...** at in Rustrest's Manage Plugins tab.

## The `Plugin` trait

Every method has a no-op default - only override what your declared capabilities need. The host only ever calls the methods matching a capability actually declared in `plugin.toml`.

| Method | Capability needed | Purpose |
| --- | --- | --- |
| `on_pre_request(ctx) -> ctx` | `request_hooks` | Mutate method/url/headers/body/variables before a request is sent. |
| `on_post_response(ctx) -> ctx` | `request_hooks` | Inspect/mutate status/headers/body/variables/test results after a response. |
| `on_command(id) -> Result<Option<String>, String>` | `commands`/`menu_items` | Handle a command-palette or menu action; the returned string shows as a toast. |
| `render_panel(panel_id) -> UiNode` | `sidebar_panel` | Render (or re-render) your panel's declarative widget tree. |
| `on_panel_event(panel_id, event) -> Option<UiNode>` | `sidebar_panel` | Handle a widget interaction; return `Some(tree)` to update the panel. |
| `import(format_id, bytes) -> Result<Value, String>` | `import_formats` | Decode into Rustrest's own collection JSON (Postman v2.1-shaped). |
| `export(format_id, collection) -> Result<Vec<u8>, String>` | `export_formats` | Encode Rustrest's collection JSON into your format. |
| `on_process_output(handle, stream, chunk)` | `external_process` | New stdout/stderr from a process spawned via `process::Process::spawn`. |
| `on_process_exit(handle, code)` | `external_process` | A spawned process exited. |
| `render_right_panel(panel_id, ctx) -> RightPanelAction` | `right_panel` | Render (or re-render) the right panel; `ctx` includes the active request/response plus every collection/environment. |
| `on_right_panel_event(panel_id, ctx, event) -> RightPanelAction` | `right_panel` | Handle a widget interaction in the right panel. |
| `on_http_response(handle, result)` | `external_process` | An outbound request started via `network::http_request` completed. |
| `on_http_response_chunk(handle, chunk)` | `external_process` | One line of a response body, as it's read off the socket (before the final result) - for rendering a streamed reply incrementally. |
| `on_files_picked(handle, result)` | `external_process` | A `process::pick_files` dialog resolved (or was cancelled). |

`RequestContext`/`ResponseContext` mirror the shape of Rustrest's built-in `pm.*` pre-request/test scripting context, so behavior stays consistent between the two mechanisms.

## The `ExternalProcess` capability

Declaring `external_process = true` in `plugin.toml` unlocks the [`process`] module: `which`, `download_file`, `make_executable`, `run_command` for one-shot commands, `Process` for spawning a long-lived child process whose output arrives through `Plugin::on_process_output`/`on_process_exit`, and `pick_files` for a native "choose files" dialog whose result arrives through `Plugin::on_files_picked`. It also unlocks `network::http_request` for outbound HTTPS requests (result via `Plugin::on_http_response`, optionally streamed line-by-line via `Plugin::on_http_response_chunk`) and `storage_read`/`storage_write` for persisting small bits of state in the plugin's private storage directory.

## The `right_panel` capability

A docked panel (toggled from the top bar) that, unlike `sidebar_panel`, is handed an ambient `RightPanelContext` snapshot on every render/event - the active request/response, every loaded collection, and every environment - and returns a `RightPanelAction` that can re-render the panel, patch the active request, or propose a create/rename/delete/duplicate/move on the collection tree (`CollectionOperation`), with destructive operations confirmed by the user before the host applies them.

## Full guide

For manifest reference, the full `right_panel`/streaming/file-picker walkthroughs, sidebar panel UI details, installing plugins, and a publishing checklist, see the [Plugin Development Guide](https://github.com/SojebSikder/rustrest/blob/main/docs/plugin-development.md) in the main Rustrest repository. Three complete example plugins also live there:

- [`rustrest-plugin-example`](https://github.com/SojebSikder/rustrest/tree/main/crates/rustrest-plugin-example) - a template touching every capability.
- [`rustrest-plugin-insomnia`](https://github.com/SojebSikder/rustrest/tree/main/crates/rustrest-plugin-insomnia) - a real import/export plugin for Insomnia v4/v5 collections.
- [`rustrest-plugin-ai-agent`](https://github.com/SojebSikder/rustrest/tree/main/crates/rustrest-plugin-ai-agent) - a `right_panel` AI assistant: streamed replies, proposing collection-tree operations, and a collection/environment/file context picker.

## License

MIT
