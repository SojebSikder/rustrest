# Plugin Development Guide

Rustrest plugins are small [WebAssembly](https://webassembly.org/) modules, sandboxed with [`wasmtime`](https://wasmtime.dev/). A plugin is a directory with a `plugin.toml` manifest plus a compiled `plugin.wasm`.

This guide covers the fundamentals of Rustrest plugin development.

## Contents

- [Installing a plugin](#installing-a-plugin)
- [Anatomy of a plugin](#anatomy-of-a-plugin)
- [Building your first plugin](#building-your-first-plugin)
- [`plugin.toml` reference](#plugintoml-reference)
- [The `Plugin` trait](#the-plugin-trait)
- [Sidebar panel UI](#sidebar-panel-ui)
- [Right panel (ambient-context) capability](#right-panel-ambient-context-capability)
- [Opening your right panel from a command](#opening-your-right-panel-from-a-command)
- [Collection/environment context and proposing tree operations](#collectionenvironment-context-and-proposing-tree-operations)
- [Streaming HTTP responses](#streaming-http-responses)
- [Picking files from disk](#picking-files-from-disk)
- [The `ExternalProcess` capability](#the-externalprocess-capability)
- [Logging and debugging](#logging-and-debugging)
- [Publishing to the plugin gallery](#publishing-to-the-plugin-gallery)
- [Publishing checklist](#publishing-checklist)

## Installing a plugin

Rustrest plugins can be installed two ways: from a local folder, or from the gallery.

Open the **Manage Plugins** tab any of these ways:

- Sidebar -> the gear (⚙) icon in the **PLUGINS** section
- Menu bar -> **Plugins → Manage Plugins...**
- Command Palette (`Ctrl+Shift+P`) -> **Manage Plugins...**

The tab has two views, switched with the **Installed** / **Browse** buttons at the top:

**Installed** (the default):

1. Click **Install Plugin Folder...** and pick a folder that directly contains a `plugin.toml` and a `plugin.wasm`. Rustrest validates the manifest, compiles the wasm, and copies both files into its own plugins directory under a subfolder named after the plugin's `id`.
2. Toggle the checkbox next to a plugin to enable/disable it (it stays on disk, just inactive).
3. Click **Uninstall** to remove it from disk entirely (confirmation required).

**Browse**: lists plugins published to the gallery index (see [Publishing to the plugin gallery](#publishing-to-the-plugin-gallery)). Click **Install** next to an entry to download, verify, and install it the same way as a local folder - no manual download/unzip needed. Click **Refresh** to re-fetch the index.

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

Real plugins ship in this repo as references:

- [`crates/rustrest-plugin-example`](../crates/rustrest-plugin-example) - a template touching every capability, including spawning a persistent external process.
- [`crates/rustrest-plugin-insomnia`](../crates/rustrest-plugin-insomnia) - a real import/export plugin for Insomnia v4/v5 collections.
- [`crates/rustrest-plugin-openapi`](../crates/rustrest-plugin-openapi) - a real import/export plugin for OpenAPI 3.0/3.1 and Swagger 2.0 API definitions (JSON or YAML).
- [`crates/rustrest-plugin-ai-agent`](../crates/rustrest-plugin-ai-agent) - an AI agent docked in the right panel: chat about the active request/response, generate a test script, edit the request or the collection tree from natural language, and pull in selected collections/environments/files as extra context - backed by a user-configured Anthropic/OpenAI/Ollama-compatible endpoint. The full reference for the `right_panel` capability, streamed responses, proposing collection-tree operations, the file picker, and outbound-HTTP/storage in general.

All of these are deliberately excluded from the root Cargo workspace (see their own `Cargo.toml`/`.cargo/config.toml`) so a normal `cargo build` of Rustrest itself doesn't require the `wasm32-unknown-unknown` target.

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
# a command id shaped `open-right-panel:<panel_id>` is a reserved
# convention the host handles directly, without reaching `on_command` -
# see "Opening your right panel from a command" below.

[[capabilities.menu_items]] # appended to a top menu-bar group (created if it doesn't exist)
group = "File"
label = "Say Hello"
command_id = "say-hello"    # dispatched the same way as a command-palette entry

[capabilities.sidebar_panel] # a single sidebar-hosted panel, routed to render_panel/on_panel_event
id = "main"
title = "Example"

[capabilities.right_panel]  # a single right-hand docked panel, routed to
                             # render_right_panel/on_right_panel_event - unlike
                             # sidebar_panel, gets ambient RightPanelContext
                             # (the active request/response) and can hand back
                             # a RequestPatch the host applies to the active tab
id = "chat"
title = "AI Agent"

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

| Method                                                           | Capability needed       | Purpose                                                                        |
| ---------------------------------------------------------------- | ----------------------- | ------------------------------------------------------------------------------ |
| `on_pre_request(ctx) -> ctx`                                     | `request_hooks`         | Mutate method/url/headers/body/variables before a request is sent.             |
| `on_post_response(ctx) -> ctx`                                   | `request_hooks`         | Inspect/mutate status/headers/body/variables/test results after a response.    |
| `on_command(id) -> Result<Option<String>, String>`               | `commands`/`menu_items` | Handle a command-palette or menu action; the returned string shows as a toast. |
| `render_panel(panel_id) -> UiNode`                               | `sidebar_panel`         | Render (or re-render) your panel's declarative widget tree.                    |
| `on_panel_event(panel_id, event) -> Option<UiNode>`              | `sidebar_panel`         | Handle a widget interaction; return `Some(tree)` to update the panel.          |
| `import(format_id, bytes) -> Result<Value, String>`              | `import_formats`        | Decode into Rustrest's own collection JSON (Postman v2.1-shaped).              |
| `export(format_id, collection) -> Result<Vec<u8>, String>`       | `export_formats`        | Encode Rustrest's collection JSON into your format.                            |
| `on_process_output(handle, stream, chunk)`                       | `external_process`      | New stdout/stderr from a process you spawned via `Process::spawn`.             |
| `on_process_exit(handle, code)`                                  | `external_process`      | A spawned process exited.                                                      |
| `render_right_panel(panel_id, ctx) -> RightPanelAction`          | `right_panel`           | Render (or re-render) the right panel; `ctx` is the active request/response.   |
| `on_right_panel_event(panel_id, ctx, event) -> RightPanelAction` | `right_panel`           | Handle a widget interaction in the right panel.                                |
| `on_http_response(handle, result)`                               | `external_process`      | An outbound request started via `network::http_request` completed.             |
| `on_http_response_chunk(handle, chunk)`                          | `external_process`      | One line of a response body, as it's read off the socket (before the final result). |
| `on_files_picked(handle, result)`                                | `external_process`      | A `process::pick_files` dialog resolved (or was cancelled).                    |

`RequestContext`/`ResponseContext` mirror the shape of Rustrest's built-in `pm.*` pre-request/test scripting context, so behavior is consistent between the two mechanisms.

## Sidebar panel UI

A plugin never touches real UI widgets directly - `render_panel`/`on_panel_event` exchange a small serializable tree the host renders with actual widgets:

```rust
use rustrest_plugin_api::{UiEvent, UiNode};

enum UiNode {
    Label(String),
    Muted(String),                 // dim/secondary line - hints, timestamps, role labels
    Button { id: String, label: String, primary: bool }, // primary = the one emphasized action
    TextInput { id: String, value: String, placeholder: String, on_submit: Option<String> },
    Checkbox { id: String, label: String, checked: bool },
    List(Vec<String>),
    Row(Vec<UiNode>),
    Column(Vec<UiNode>),
    HorizontalSpacer,              // fills horizontal space
    VerticalSpacer,                // fills vertical space
    FixedSpace { width: f32, height: f32 },
    Scrollable(Box<UiNode>),       // an independent scroll region
    AutoScroll(Box<UiNode>),       // like Scrollable, but snapped to the bottom on every render
}

enum UiEvent {
    Clicked(String),
    Changed(String, String),
    Toggled(String, bool),
}
```

`on_panel_event` gets called with the `id` of whichever widget the user interacted with; return the updated tree from `Some(...)` (or `None` to leave the currently-rendered tree as-is).

The host only auto-scrolls a panel's whole tree when it contains no explicit `Scrollable`/`AutoScroll` anywhere; once you use one, you're opting into controlling scrolling yourself - typically to keep a header/footer (a settings button, a send box) pinned while just the middle section scrolls. `TextInput.on_submit`, if set, fires `UiEvent::Clicked` with that id when the user presses Enter in the field - e.g. wiring a chat input to its Send button's id so Enter submits without reaching for the mouse. Every `Label`/`Muted` line is automatically right-click "Copy"-able - the host handles this itself, nothing to do on your end.

## Right panel (ambient-context) capability

`sidebar_panel` opens as a tab and gets no context about what the user is working on. `right_panel` is different: it's docked in a panel on the right of the window (toggled from a button in the top bar), reuses the same `UiNode`/`UiEvent` tree from above, and every `render_right_panel`/`on_right_panel_event` call is handed a `RightPanelContext` snapshot of the active tab:

```rust
use rustrest_plugin_api::{
    RequestContext, ResponseContext, RequestPatch, RightPanelAction, RightPanelContext,
    CollectionSummary, RequestSummary, EnvSummary,
};

struct RightPanelContext {
    active_request: Option<RequestContext>,   // None if the active tab isn't an HTTP request
    active_response: Option<ResponseContext>, // None if it hasn't been sent yet
    collections: Vec<CollectionSummary>,      // every collection currently loaded
    environments: Vec<EnvSummary>,            // every environment currently defined
}

struct CollectionSummary {
    id: usize,
    name: String,
    folders: Vec<Vec<String>>,      // every folder's full path, e.g. ["Auth", "Login"]
    requests: Vec<RequestSummary>,
}

struct RequestSummary {
    id: usize,
    name: String,
    folder_path: Vec<String>,
    method: String,
}

struct EnvSummary {
    name: String,
    variables: Vec<(String, String)>, // values included - see the warning below
}
```

`collections`/`environments` are always populated, regardless of what tab is active - only `active_request`/`active_response` depend on it. `EnvSummary.variables` includes actual variable *values*, which may be secrets (API keys, tokens); if your UI lets the user pick which environments to include, say so visibly rather than sending them silently - see `rustrest-plugin-ai-agent`'s context picker for a working example.

Both hooks return a `RightPanelAction` rather than a plain tree, so a plugin can also ask the host to edit the active request, or propose a change to the collection tree itself - the mechanism an AI-assistant-style plugin uses to turn a natural-language instruction into request changes, a generated test script, or a create/rename/delete/move on a collection/folder/request:

```rust
enum RightPanelAction {
    None,
    UpdateUi(UiNode),                                       // re-render the panel only
    ApplyPatch(RequestPatch),                                // edit the active tab only
    UpdateUiAndApplyPatch(UiNode, RequestPatch),             // both
    UpdateUiAndProposeCollectionOp(UiNode, CollectionOperation), // re-render + propose a tree edit
}

struct RequestPatch {
    method: Option<String>,
    url: Option<String>,
    headers: Option<Vec<(String, String)>>,
    body: Option<String>,
    pre_request_script: Option<String>,
    post_response_script: Option<String>,
}
```

Every `RequestPatch` field is optional - only set the ones you want changed. Since `on_http_response` (see below) has no return value the host acts on, a common pattern for an async flow (e.g. "wait for the LLM's reply, then apply it") is to stash the patch on `self` from `on_http_response` and flush it as `UpdateUiAndApplyPatch` the next time `render_right_panel` is called - see `rustrest-plugin-ai-agent`'s `pending_patch` field for a working example.

## Opening your right panel from a command

Opening a right panel is a host-only action (there's no wasm involved in flipping a docked panel open) - so rather than routing through `on_command` just to have your plugin ask the host to open its own panel, declare a command whose `id` is `open-right-panel:<panel_id>`, matching the `id` you gave that panel under `[capabilities.right_panel]`:

```toml
[[capabilities.commands]]
id = "open-right-panel:chat"
title = "Open AI Agent"

[capabilities.right_panel]
id = "chat"
title = "AI Agent"
```

The host intercepts that command id before it ever reaches your wasm code and opens the panel directly (a no-op if it's already open). This shows up as a normal command-palette/menu entry with no extra code on your end - `on_command` is never called for it.

## Collection/environment context and proposing tree operations

A `right_panel` plugin isn't limited to editing the currently-open request - `RightPanelContext.collections`/`.environments` (above) give it enough information to act on the whole workspace, and `RightPanelAction::UpdateUiAndProposeCollectionOp` lets it propose a change to that tree:

```rust
use rustrest_plugin_api::CollectionOperation;

#[serde(tag = "op", rename_all = "snake_case")]
enum CollectionOperation {
    CreateCollection { name: String },
    RenameCollection { collection_id: usize, new_name: String },
    DeleteCollection { collection_id: usize },
    CreateFolder { collection_id: usize, parent_path: Vec<String>, name: String },
    RenameFolder { collection_id: usize, path: Vec<String>, new_name: String },
    DeleteFolder { collection_id: usize, path: Vec<String> },
    CreateRequest { collection_id: usize, parent_path: Vec<String>, name: String, method: String, url: String },
    RenameRequest { collection_id: usize, request_id: usize, new_name: String },
    DeleteRequest { collection_id: usize, parent_path: Vec<String>, request_id: usize },
    DuplicateRequest { collection_id: usize, parent_path: Vec<String>, request_id: usize },
    MoveRequest { collection_id: usize, from_path: Vec<String>, request_id: usize, to_path: Vec<String> },
}
```

`parent_path`/`path`/`from_path`/`to_path` are folder-name arrays (`[]` for a collection's root, `["Auth"]` for a top-level folder named "Auth") - use the ids/paths handed to you in `RightPanelContext.collections`, never invented ones. The enum is internally tagged (`#[serde(tag = "op", rename_all = "snake_case")]`) specifically so it's easy to have an LLM produce directly, e.g. `{"op":"create_folder","collection_id":1,"parent_path":[],"name":"Auth"}`.

The host applies `CreateCollection`/`RenameCollection`/`CreateFolder`/`RenameFolder`/`CreateRequest`/`RenameRequest` immediately, but shows the user a confirmation dialog first for `DeleteCollection`/`DeleteFolder`/`DeleteRequest`/`MoveRequest` (anything that removes or relocates something) before applying it - you don't need to build this yourself, just return the action and the host handles the rest, including the toast reporting success/failure. `rustrest-plugin-ai-agent` extracts one of these from a fenced ` ```collection-action ` block in an LLM reply (parallel to how it extracts a `RequestPatch` from a ` ```json ` block) - see its `extract_collection_op`/`build_system_prompt` for a full working example, including how it lists `ctx.collections` in the prompt so the model has real ids to reference.

## Streaming HTTP responses

`network::http_request` (below) delivers its result in one shot via `on_http_response`. For a chat-completion-style API where the response streams in as Server-Sent Events or newline-delimited JSON, `on_http_response_chunk` is delivered once per line, as it's read off the socket - before the terminal `on_http_response` call with the full accumulated body:

```rust
impl Plugin for MyPlugin {
    fn on_http_response_chunk(&mut self, handle: u32, chunk: Vec<u8>) {
        // one line at a time - parse whatever line-delimited shape your
        // provider streams (an SSE `data: {...}` line, a raw NDJSON object)
        // and append any text delta to a message you're building up.
    }

    fn on_http_response(&mut self, handle: u32, result: Result<HttpResponseData, String>) {
        // the streamed message is already complete by the time this runs -
        // this is just where you do any end-of-reply bookkeeping (clear a
        // "busy" flag, extract a trailing action block from the full text).
    }
}
```

This has no separate capability or opt-in - just implement it if you care about partial data, and ignore it (the default no-op) if you only need the final result. Nothing needs to change about how you start the request, other than asking the provider to actually stream (typically a `"stream": true` field in the request body). Pair a chat feed's `UiNode::AutoScroll` with this so new text is visible immediately as it arrives - the ~100ms tick that delivers chunks also re-renders any open right panel, so there's no extra polling to write. See `rustrest-plugin-ai-agent`'s `on_http_response_chunk`/`extract_stream_delta` for Anthropic/OpenAI SSE and Ollama NDJSON parsing side by side.

## Picking files from disk

Declaring `external_process = true` also unlocks a native "choose files" dialog, for pulling arbitrary local files in as context (e.g. an OpenAPI spec, a README) rather than requiring the user to paste content by hand:

```rust
use rustrest_plugin_api::{Plugin, PickFilesResult, process::pick_files};

#[derive(Default)]
struct MyPlugin { pending_pick: Option<u32> }

impl Plugin for MyPlugin {
    fn on_command(&mut self, command_id: &str) -> Result<Option<String>, String> {
        if command_id == "attach-files" {
            self.pending_pick = Some(pick_files()?);
        }
        Ok(None)
    }

    fn on_files_picked(&mut self, handle: u32, result: PickFilesResult) {
        if self.pending_pick != Some(handle) {
            return; // a stale/cancelled pick
        }
        self.pending_pick = None;
        // result.files: Vec<PickedFile { name, text }> - successfully read as UTF-8 text
        // result.skipped: Vec<String> - one "<name>: <reason>" per file that couldn't be
        // attached (too large, or not valid UTF-8 text)
    }
}
```

Like `http_request`, this never blocks - the dialog opens on a host-owned background thread, and the call returns a handle immediately with the result delivered later via `on_files_picked` (an empty `result.files` means the user cancelled, not an error). Files are capped at 5 MB and must be valid UTF-8 text; anything else is reported in `skipped` rather than silently dropped. See `rustrest-plugin-ai-agent`'s "Attach Files..." button (`pending_file_pick`/`attached_files`) for a working example, including a per-file "Remove" button.

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

The same capability also unlocks persisting small bits of state (settings, a cached token, ...) in the plugin's private storage directory - there's no direct filesystem access from a wasm guest, so this goes through the host too:

```rust
use rustrest_plugin_api::{storage_read, storage_write};

storage_write("config.json", b"{...}")?;
let bytes: Option<Vec<u8>> = storage_read("config.json")?; // None if it doesn't exist yet
```

And outbound HTTP requests, for anything that needs to talk to a real API (an LLM provider, a webhook, ...) rather than download a file. Unlike everything else in `Plugin`, this call never blocks the plugin call path - it starts the request on a host-owned background thread and returns a handle immediately, with the result delivered later via `on_http_response` (and, if you want it, incrementally via `on_http_response_chunk` - see [Streaming HTTP responses](#streaming-http-responses) above):

```rust
use rustrest_plugin_api::{HttpRequestSpec, HttpResponseData, Plugin, http_request};

#[derive(Default)]
struct MyPlugin { pending_handle: Option<u32> }

impl Plugin for MyPlugin {
    fn on_command(&mut self, command_id: &str) -> Result<Option<String>, String> {
        if command_id == "ping" {
            let spec = HttpRequestSpec {
                method: "GET".to_string(),
                url: "https://example.com/api/ping".to_string(), // https:// only
                headers: vec![],
                body: None,
            };
            self.pending_handle = Some(http_request(spec)?);
        }
        Ok(None)
    }

    fn on_http_response(&mut self, _handle: u32, result: Result<HttpResponseData, String>) {
        // result.body/status/headers, or an error string - inspect and act on it here.
        // there's no return value the host acts on, so if this needs to change what
        // render_panel/render_right_panel shows, stash it on `self` and read it back
        // from there on the next render call.
    }
}
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

## Publishing to the plugin gallery

The **Browse** view in Manage Plugins fetches a JSON index from a separate, curated repo - [`Rustrest/plugins`](https://github.com/Rustrest/plugins) - rather than anything built into Rustrest itself. That keeps plugin curation decoupled from app releases: adding a plugin to the gallery is a pull request to that repo, not a change to Rustrest.

The index is a single `index.json` at the repo root:

```json
{
  "plugins": [
    {
      "id": "example",
      "name": "Example Plugin",
      "version": "0.1.0",
      "author": "Rustrest",
      "description": "Demonstrates a request hook, a command, and a sidebar panel.",
      "download_url": "https://github.com/<you>/<repo>/releases/download/v0.1.0/example.zip",
      "sha256": "<sha256 of the zip, optional but recommended>",
      "homepage": "https://github.com/<you>/<repo>"
    }
  ]
}
```

`download_url` must point at a `.zip` containing `plugin.toml` and `plugin.wasm` at its root (or in a single top-level folder - the installer searches the extracted archive for `plugin.toml`). GitHub Releases work well as a host: attach the zip (and a matching `sha256` if you want integrity-checked installs) to a release of your plugin's own repo, then point `download_url` at the release asset.

To publish:

1. Build and zip your plugin (`plugin.toml` + `plugin.wasm`), attach it to a release in your own repo.
2. Open a pull request against [`Rustrest/plugins`](https://github.com/Rustrest/plugins) adding an entry to `index.json`.
3. Once merged, it shows up in every user's **Browse** view (they may need to click **Refresh**).

## Publishing checklist

- [ ] `id` in `plugin.toml` is unique and matches the folder name you ship
- [ ] Only the capabilities you actually use are declared (undeclared `ExternalProcess` calls fail with a clear "capability not declared" error - by design, so a user can trust the badges shown in Manage Plugins)
- [ ] Built with `cargo build --release --target wasm32-unknown-unknown`
- [ ] The release folder contains exactly `plugin.toml` + `plugin.wasm` at its root
- [ ] Tested via **Install Plugin Folder...**, not just `cargo test` against the crate in isolation
