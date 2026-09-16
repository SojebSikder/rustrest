# Plugin Development Guide

Rustrest plugins are small [WebAssembly](https://webassembly.org/) modules, sandboxed with [`wasmtime`](https://wasmtime.dev/). A plugin is a directory with a `plugin.toml` manifest plus a compiled `plugin.wasm`.

This guide covers basic fundamendal of Rustrest plugin development.

## Contents

- [Installing a plugin](#installing-a-plugin)
- [Anatomy of a plugin](#anatomy-of-a-plugin)
- [Building your first plugin](#building-your-first-plugin)
- [`plugin.toml` reference](#plugintoml-reference)
- [The `Plugin` trait](#the-plugin-trait)
- [Sidebar panel UI](#sidebar-panel-ui)
- [The `ExternalProcess` capability](#the-externalprocess-capability)
- [Logging and debugging](#logging-and-debugging)
- [Publishing checklist](#publishing-checklist)

## Installing a plugin

Open the **Manage Plugins** tab any of these ways:

- Sidebar -> the gear (⚙) icon in the **PLUGINS** section
- Menu bar -> **Plugins → Manage Plugins...**
- Command Palette (`Ctrl+Shift+P`) -> **Manage Plugins...**

From there:

1. Click **Install Plugin Folder...** and pick a folder that directly contains a `plugin.toml` and a `plugin.wasm`. Rustrest validates the manifest, compiles the wasm, and copies both files into its own plugins directory under a subfolder named after the plugin's `id`.
2. Toggle the checkbox next to a plugin to enable/disable it (it stays on disk, just inactive).
3. Click **Uninstall** to remove it from disk entirely (confirmation required).

Each installed plugin shows capability badges (Request Hooks, Commands, Sidebar Panel, **External Process**, etc.) so you can see what it can touch before turning it on - a plugin with **External Process** can reach the network, run programs, and read/write its own storage directory.

Plugins live under Rustrest's data directory:

| OS      | Plugins directory                                      |
| ------- | ------------------------------------------------------ |
| Linux   | `~/.local/share/Rustrest/plugins/<id>/`                |
| macOS   | `~/Library/Application Support/Rustrest/plugins/<id>/` |
| Windows | `%APPDATA%\Rustrest\plugins\<id>\`                     |

Which plugins are disabled is tracked in `plugins.json` next to the `plugins/` folder. Rustrest only scans the plugins directory at startup - if you drop a plugin folder in manually rather than using **Install Plugin Folder...**, restart Rustrest to pick it up.

## Anatomy of a plugin

An installed plugin's directory looks like this:

```
example/
├── plugin.toml   # declarative manifest - read without running any code
├── plugin.wasm   # the compiled plugin
└── storage/      # created lazily if the plugin uses ExternalProcess (downloads, etc.)
```

The directory name **must** match the `id` field in `plugin.toml` - this is enforced on load; a mismatch is reported as a load error in the Manage Plugins tab rather than silently ignored.

Two real plugins ship in this repo as references:

- [`crates/rustrest-plugin-example`](../crates/rustrest-plugin-example) - a template touching every capability, including spawning a persistent external process.
- [`crates/rustrest-plugin-insomnia`](../crates/rustrest-plugin-insomnia) - a real import/export plugin for Insomnia v4/v5 collections.

Both are deliberately excluded from the root Cargo workspace (see their own `Cargo.toml`/`.cargo/config.toml`) so a normal `cargo build` of Rustrest itself doesn't require the `wasm32-unknown-unknown` target.

## Building your first plugin

### 1. Prerequisites

```bash
rustup target add wasm32-unknown-unknown
```

### 2. Scaffold the crate

```bash
cargo new --lib my-plugin
cd my-plugin
```

`Cargo.toml`:

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
rustrest-plugin-api = { git = "https://github.com/SojebSikder/rustrest", package = "rustrest-plugin-api" }
```

(If you're developing inside a clone of this repo, use a path dependency instead: `{ path = "../../crates/rustrest-plugin-api" }`.)

`.cargo/config.toml` - **required**, or the final link step fails:

```toml
[target.wasm32-unknown-unknown]
# host functions (log, which, download_file, process_spawn, ...) are
# undefined symbols at compile time - they're resolved as wasm imports by
# the host at instantiation time, not by the linker.
rustflags = ["-C", "link-arg=--import-undefined"]
```

### 3. Write `plugin.toml`

Put this at the crate root, next to `Cargo.toml`:

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

### 4. Implement the plugin

`src/lib.rs`:

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

### 5. Build

```bash
cargo build --release --target wasm32-unknown-unknown
```

This produces `target/wasm32-unknown-unknown/release/my_plugin.wasm`.

### 6. Install for testing

Stage `plugin.toml` and the built wasm together (renamed to `plugin.wasm`) in one folder, then use **Install Plugin Folder...** in the Manage Plugins tab:

```bash
mkdir -p dist
cp plugin.toml dist/plugin.toml
cp target/wasm32-unknown-unknown/release/my_plugin.wasm dist/plugin.wasm
```

Point the folder picker at `dist/`. Re-run these two `cp` commands and reinstall (uninstall the old copy first - installing over an existing id is rejected) after every change while iterating.

## `plugin.toml` reference

```toml
id = "example"              # unique, and must match the install directory name
name = "Example Plugin"
version = "0.1.0"
author = "Rustrest"
description = "..."
schema_version = 1          # manifest schema version; omit to default to the current one

[capabilities]
request_hooks = true        # participate in every outgoing request / incoming response
external_process = true     # unlock which/download_file/run_command/Process (see below)

[[capabilities.commands]]   # command palette / menu entries, routed to `on_command`
id = "say-hello"
title = "Example: Say Hello"
subtitle = "from the example plugin"   # optional

[[capabilities.menu_items]] # appended to a top menu-bar group (created if it doesn't exist)
group = "File"
label = "Say Hello"
command_id = "say-hello"    # dispatched the same way as a command-palette entry

[capabilities.sidebar_panel] # a single sidebar-hosted panel, routed to render_panel/on_panel_event
id = "main"
title = "Example"

[[capabilities.import_formats]]  # collection import, routed to `import`
id = "insomnia"
title = "Insomnia (v4 JSON / v5.1 YAML)"
extensions = ["json", "yaml", "yml"]   # file-picker filter extensions

[[capabilities.export_formats]]  # collection export, routed to `export`
id = "insomnia-v4"
title = "Insomnia v4 (JSON)"
extensions = ["json"]
```

Every table under `[capabilities]` is optional - only declare what you use. The host only ever calls the `Plugin` methods matching a capability you actually declared.

## The `Plugin` trait

| Method                                                     | Capability needed       | Purpose                                                                        |
| ---------------------------------------------------------- | ----------------------- | ------------------------------------------------------------------------------ |
| `on_pre_request(ctx) -> ctx`                               | `request_hooks`         | Mutate method/url/headers/body/variables before a request is sent.             |
| `on_post_response(ctx) -> ctx`                             | `request_hooks`         | Inspect/mutate status/headers/body/variables/test results after a response.    |
| `on_command(id) -> Result<Option<String>, String>`         | `commands`/`menu_items` | Handle a command-palette or menu action; the returned string shows as a toast. |
| `render_panel(panel_id) -> UiNode`                         | `sidebar_panel`         | Render (or re-render) your panel's declarative widget tree.                    |
| `on_panel_event(panel_id, event) -> Option<UiNode>`        | `sidebar_panel`         | Handle a widget interaction; return `Some(tree)` to update the panel.          |
| `import(format_id, bytes) -> Result<Value, String>`        | `import_formats`        | Decode into Rustrest's own collection JSON (Postman v2.1-shaped).              |
| `export(format_id, collection) -> Result<Vec<u8>, String>` | `export_formats`        | Encode Rustrest's collection JSON into your format.                            |
| `on_process_output(handle, stream, chunk)`                 | `external_process`      | New stdout/stderr from a process you spawned via `Process::spawn`.             |
| `on_process_exit(handle, code)`                            | `external_process`      | A spawned process exited.                                                      |

`RequestContext`/`ResponseContext` mirror the shape of Rustrest's built-in `pm.*` pre-request/test scripting context, so behavior is consistent between the two mechanisms.

## Sidebar panel UI

A plugin never touches real UI widgets directly - `render_panel`/`on_panel_event` exchange a small serializable tree the host renders with actual widgets:

```rust
use rustrest_plugin_api::{UiEvent, UiNode};

enum UiNode {
    Label(String),
    Button { id: String, label: String },
    TextInput { id: String, value: String, placeholder: String },
    Checkbox { id: String, label: String, checked: bool },
    List(Vec<String>),
    Row(Vec<UiNode>),
    Column(Vec<UiNode>),
}

enum UiEvent {
    Clicked(String),
    Changed(String, String),
    Toggled(String, bool),
}
```

`on_panel_event` gets called with the `id` of whichever widget the user interacted with; return the updated tree from `Some(...)` (or `None` to leave the currently-rendered tree as-is).

## The `ExternalProcess` capability

Declare `external_process = true` in `plugin.toml` to unlock `rustrest_plugin_api::process`:

```rust
use rustrest_plugin_api::process::{which, download_file, make_executable, run_command};

// locate a binary already on PATH
let path = which("rust-analyzer");

// or fetch one - https only, streamed into this plugin's private storage dir,
// size-capped host-side
let path = download_file("https://example.com/tool.tar.gz", "tool.tar.gz")?;
make_executable(&path)?;

// run something to completion (spawn, wait, capture output, host-enforced timeout)
let output = run_command(&path, &["--version"], None, Some(5_000))?;
```

For anything long-lived - the actual "download and drive `rust-analyzer`" case - use `Process`:

```rust
use rustrest_plugin_api::{Plugin, Process, ProcessStream};

#[derive(Default)]
struct MyPlugin {
    server: Option<Process>,
}

impl Plugin for MyPlugin {
    fn on_command(&mut self, command_id: &str) -> Result<Option<String>, String> {
        if command_id == "start-server" {
            self.server = Some(Process::spawn("rust-analyzer", &[], None, &[])?);
        }
        Ok(None)
    }

    fn on_process_output(&mut self, _handle: u32, stream: ProcessStream, chunk: Vec<u8>) {
        // e.g. parse LSP JSON-RPC frames out of `chunk` here
    }

    fn on_process_exit(&mut self, _handle: u32, code: Option<i32>) {
        self.server = None;
    }
}
```

`Process::write`/`kill`/`try_wait` are synchronous and cheap (they just touch a pipe or a handle). Output isn't polled - the host drains it on a timer and delivers it via `on_process_output`/`on_process_exit`, the same call path `on_command`/`on_panel_event` already use. There's no persistent-process cleanup you need to write yourself: disabling or uninstalling the plugin kills anything it spawned.

`rustrest-plugin-example` demonstrates the full spawn -> write -> streamed-output round trip using `cat` (or `findstr /R "^"` on Windows) as a network-free stand-in for a real downloaded tool - a good starting point to copy from.

## Logging and debugging

```rust
rustrest_plugin_api::log("something happened");
```

Log lines are prefixed with your plugin's id and forwarded into Rustrest's own console/log output. A plugin that panics, traps, or returns an error for a given call has that one call's error surfaced (and logged) without taking down the rest of the app - a bad `on_pre_request`, for example, just gets skipped for that request while every other enabled plugin still runs.

## Publishing checklist

- [ ] `id` in `plugin.toml` is unique and matches the folder name you ship
- [ ] Only the capabilities you actually use are declared (undeclared `ExternalProcess` calls fail with a clear "capability not declared" error - by design, so a user can trust the badges shown in Manage Plugins)
- [ ] Built with `cargo build --release --target wasm32-unknown-unknown`
- [ ] The release folder contains exactly `plugin.toml` + `plugin.wasm` at its root
- [ ] Tested via **Install Plugin Folder...**, not just `cargo test` against the crate in isolation
