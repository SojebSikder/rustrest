//! Language-agnostic LSP client. The app spawns whatever server a plugin's
//! `language_servers` capability points at and talks to it through [`LanguageServer`];.
//!
//! Positions are 0-based with `column` as a UTF-8 byte offset into the line
//! (what the app's editor uses). The client offers the `utf-8` position
//! encoding and converts when a server only speaks the LSP default, UTF-16.

mod client;
mod codec;
mod convert;
mod position;

pub use client::LanguageServer;

/// how to launch a language server.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServerCommand {
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    pub line: u32,
    /// UTF-8 byte offset within the line.
    pub column: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Function,
    Method,
    Property,
    Variable,
    Constant,
    Keyword,
    Module,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    /// short signature/type
    pub detail: Option<String>,
    /// plain-text description
    pub documentation: Option<String>,
    /// text to insert in place of the typed word; `label` if `None`
    pub insert_text: Option<String>,
}

/// id of a completion/hover request, echoed back in its [`Event`].
pub type RequestId = i64;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// the server finished initializing.
    Ready,
    Diagnostics {
        uri: String,
        version: Option<i32>,
        diagnostics: Vec<Diagnostic>,
    },
    Completions {
        request: RequestId,
        items: Vec<CompletionItem>,
    },
    Hover {
        request: RequestId,
        contents: Option<String>,
    },
    /// stderr output or a `window/logMessage`.
    Log(String),
    /// the server process went away; outstanding requests were answered empty.
    Exited(Option<i32>),
}
