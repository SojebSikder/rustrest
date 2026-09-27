//! runs the language server a plugin declares for the script editors

use super::{CollectionSubTab, Rustrest, TabState, WorkspaceContent};
use crate::message::Message;
use crate::ui::script_editor::ScriptContent;
use crate::ui::tab::types::{RequestSubTab, ScriptTab};
use iced::Task;
use rustrest_lsp::{Event, LanguageServer, Position, RequestId, ServerCommand};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// the script editors' language, as declared in a plugin's `language_servers`.
const LANGUAGE: &str = "javascript";
const URI_PREFIX: &str = "file:///rustrest/scripts/";
// a server that never answers shouldn't keep the fast tick running forever
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// how soon to ask the plugin again while it's installing the server
const PENDING_RETRY: Duration = Duration::from_secs(1);
const FAILED_RETRY: Duration = Duration::from_secs(30);
const MAX_AUTO_RESTARTS: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Provider {
    plugin_id: String,
    server_id: String,
    name: String,
}

#[derive(Default)]
enum Server {
    #[default]
    None,
    /// waiting for the plugin (e.g. still downloading) or retrying after a failure
    Waiting {
        provider: Provider,
        retry_at: Instant,
    },
    Running {
        provider: Provider,
        server: Box<LanguageServer>,
    },
    /// crashed too often; back on "Restart Language Server" or a provider change
    Stopped { provider: Provider },
}

#[derive(Default)]
pub struct ScriptIntelState {
    server: Server,
    /// bumped on every server start, so each editor gets re-sent
    generation: u64,
    open_docs: HashSet<String>,
    requests: HashMap<RequestId, PendingRequest>,
    restarts: u32,
    /// last error logged, so a retry loop doesn't flood the console
    last_error: Option<String>,
}

struct PendingRequest {
    uri: String,
    sent: Instant,
    kind: RequestKind,
}

enum RequestKind {
    Completion { anchor: Position },
    Hover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocOwner {
    Tab(usize),
    Collection(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DocRef {
    owner: DocOwner,
    script: ScriptTab,
}

impl DocRef {
    fn id(self) -> String {
        let script = match self.script {
            ScriptTab::PreRequest => "pre",
            ScriptTab::PostResponse => "post",
        };
        match self.owner {
            DocOwner::Tab(id) => format!("tab-{id}-{script}"),
            DocOwner::Collection(id) => format!("collection-{id}-{script}"),
        }
    }

    fn parse(id: &str) -> Option<Self> {
        let (rest, script) = id.rsplit_once('-')?;
        let script = match script {
            "pre" => ScriptTab::PreRequest,
            "post" => ScriptTab::PostResponse,
            _ => return None,
        };
        let (owner, number) = rest.split_once('-')?;
        let number = number.parse().ok()?;
        let owner = match owner {
            "tab" => DocOwner::Tab(number),
            "collection" => DocOwner::Collection(number),
            _ => return None,
        };
        Some(Self { owner, script })
    }

    fn uri(self) -> String {
        let file = match self.script {
            ScriptTab::PreRequest => "pre-request.js",
            ScriptTab::PostResponse => "post-response.js",
        };
        format!("{URI_PREFIX}{}/{file}", self.id())
    }

    fn from_uri(uri: &str) -> Option<Self> {
        let (id, _file) = uri.strip_prefix(URI_PREFIX)?.split_once('/')?;
        Self::parse(id)
    }
}

fn script_ref(tabs: &[TabState], doc: DocRef) -> Option<&ScriptContent> {
    match doc.owner {
        DocOwner::Tab(id) => tabs
            .iter()
            .find(|t| t.tab.id == id)
            .map(|t| match doc.script {
                ScriptTab::PreRequest => &t.tab.pre_request_script,
                ScriptTab::PostResponse => &t.tab.post_response_script,
            }),
        DocOwner::Collection(id) => tabs.iter().find_map(|t| match &t.content {
            WorkspaceContent::CollectionRoot {
                collection_id,
                settings: Some(settings),
                ..
            } if *collection_id == id => Some(match doc.script {
                ScriptTab::PreRequest => &settings.pre_request_script,
                ScriptTab::PostResponse => &settings.post_response_script,
            }),
            _ => None,
        }),
    }
}

fn script_mut(tabs: &mut [TabState], doc: DocRef) -> Option<&mut ScriptContent> {
    match doc.owner {
        DocOwner::Tab(id) => tabs
            .iter_mut()
            .find(|t| t.tab.id == id)
            .map(|t| match doc.script {
                ScriptTab::PreRequest => &mut t.tab.pre_request_script,
                ScriptTab::PostResponse => &mut t.tab.post_response_script,
            }),
        DocOwner::Collection(id) => tabs.iter_mut().find_map(|t| match &mut t.content {
            WorkspaceContent::CollectionRoot {
                collection_id,
                settings: Some(settings),
                ..
            } if *collection_id == id => Some(match doc.script {
                ScriptTab::PreRequest => &mut settings.pre_request_script,
                ScriptTab::PostResponse => &mut settings.post_response_script,
            }),
            _ => None,
        }),
    }
}

/// the script editor currently on screen, if any.
fn visible_doc(app: &Rustrest) -> Option<DocRef> {
    let tab = app.tabs.get(app.active_tab_index)?;
    match &tab.content {
        WorkspaceContent::HttpRequest if tab.tab.active_sub_tab == RequestSubTab::Scripts => {
            Some(DocRef {
                owner: DocOwner::Tab(tab.tab.id),
                script: tab.tab.script_tab,
            })
        }
        WorkspaceContent::CollectionRoot {
            collection_id,
            active_sub_tab: CollectionSubTab::Scripts,
            settings: Some(settings),
            ..
        } => Some(DocRef {
            owner: DocOwner::Collection(*collection_id),
            script: settings.script_tab,
        }),
        _ => None,
    }
}

fn provider(app: &Rustrest) -> Option<Provider> {
    app.plugins
        .plugin_manager
        .language_servers()
        .into_iter()
        .find(|(_, server)| server.languages.iter().any(|l| l == LANGUAGE))
        .map(|(plugin_id, server)| Provider {
            plugin_id,
            server_id: server.id,
            name: server.name,
        })
}

pub fn has_language_plugin(app: &Rustrest) -> bool {
    provider(app).is_some()
}

/// whether there's work in flight that deserves a faster tick than the
/// default plugin pump: unsent edits, pending requests, or a wanted completion/hover.
pub fn needs_fast_tick(app: &Rustrest) -> bool {
    let state = &app.script_intel;
    if !matches!(state.server, Server::Running { .. }) {
        return false;
    }
    if !state.requests.is_empty() {
        return true;
    }
    visible_doc(app)
        .and_then(|doc| script_ref(&app.tabs, doc))
        .is_some_and(|script| {
            script.lsp.synced != Some((state.generation, script.revision()))
                || script.lsp.want_completion
                || script.lsp.want_hover
        })
}

pub fn tick(app: &mut Rustrest) -> Task<Message> {
    // pumps plugin downloads/processes first, so a finished install is seen now
    let task = super::plugins::process_tick(app);
    manage_server(app);
    sync_visible(app);
    apply_events(app);
    task
}

/// "Restart Language Server": relaunches it and forgets the crash count.
pub fn restart(app: &mut Rustrest) -> Task<Message> {
    let state = &mut app.script_intel;
    state.restarts = 0;
    state.last_error = None;
    state.server = match std::mem::take(&mut state.server) {
        Server::Running { provider, .. }
        | Server::Waiting { provider, .. }
        | Server::Stopped { provider } => Server::Waiting {
            provider,
            retry_at: Instant::now(),
        },
        Server::None => Server::None,
    };
    reset_session(state);
    manage_server(app);
    Task::none()
}

fn log(app: &mut Rustrest, line: String) {
    app.layout.console_logs.push(line);
}

fn log_error(app: &mut Rustrest, error: String) {
    if app.script_intel.last_error.as_ref() != Some(&error) {
        app.script_intel.last_error = Some(error.clone());
        log(app, error);
    }
}

/// forgets per-server state (open documents, pending requests).
fn reset_session(state: &mut ScriptIntelState) {
    state.generation += 1;
    state.open_docs.clear();
    state.requests.clear();
}

fn manage_server(app: &mut Rustrest) {
    let wanted = provider(app);
    let state = &mut app.script_intel;
    let current = match &state.server {
        Server::None => None,
        Server::Waiting { provider, .. }
        | Server::Running { provider, .. }
        | Server::Stopped { provider } => Some(provider),
    };
    if current != wanted.as_ref() {
        // plugin installed, removed, disabled or replaced
        state.server = match wanted {
            Some(provider) => Server::Waiting {
                provider,
                retry_at: Instant::now(),
            },
            None => Server::None,
        };
        state.restarts = 0;
        state.last_error = None;
        reset_session(state);
    }

    if let Server::Running { provider, server } = &state.server
        && !server.is_running()
    {
        let provider = provider.clone();
        reset_session(state);
        if state.restarts < MAX_AUTO_RESTARTS {
            state.restarts += 1;
            state.server = Server::Waiting {
                provider,
                retry_at: Instant::now(),
            };
        } else {
            let name = provider.name.clone();
            state.server = Server::Stopped { provider };
            log(
                app,
                format!(
                    "[{name}] stopped after crashing repeatedly; run \"Restart Language Server\""
                ),
            );
            return;
        }
    }

    let Server::Waiting { provider, retry_at } = &app.script_intel.server else {
        return;
    };
    if Instant::now() < *retry_at {
        return;
    }
    let provider = provider.clone();
    let manager = &mut app.plugins.plugin_manager;
    let (next, error) = match manager
        .language_server_command(&provider.plugin_id, &provider.server_id)
    {
        Ok(Some(command)) => {
            let options = manager
                .language_server_initialization_options(&provider.plugin_id, &provider.server_id)
                .unwrap_or_default();
            let command = ServerCommand {
                command: command.command,
                args: command.args,
                env: command.env,
            };
            match LanguageServer::start(&provider.name, &command, options) {
                Ok(server) => (
                    Server::Running {
                        provider,
                        server: Box::new(server),
                    },
                    None,
                ),
                Err(e) => {
                    let error = format!(
                        "[{}] failed to start {}: {e}",
                        provider.name, command.command
                    );
                    let retry_at = Instant::now() + FAILED_RETRY;
                    (Server::Waiting { provider, retry_at }, Some(error))
                }
            }
        }
        Ok(None) => {
            let retry_at = Instant::now() + PENDING_RETRY;
            (Server::Waiting { provider, retry_at }, None)
        }
        Err(e) => {
            let error = format!("[{}] {e}", provider.name);
            let retry_at = Instant::now() + FAILED_RETRY;
            (Server::Waiting { provider, retry_at }, Some(error))
        }
    };
    if matches!(next, Server::Running { .. }) {
        reset_session(&mut app.script_intel);
    }
    app.script_intel.server = next;
    super::plugins::drain_plugin_logs(app);
    if let Some(error) = error {
        log_error(app, error);
    }
}

fn sync_visible(app: &mut Rustrest) {
    let visible = visible_doc(app);
    let tabs = &mut app.tabs;
    let state = &mut app.script_intel;
    state
        .requests
        .retain(|_, r| r.sent.elapsed() < REQUEST_TIMEOUT);

    let Server::Running { server, .. } = &mut state.server else {
        if let Some(script) = visible.and_then(|doc| script_mut(tabs, doc)) {
            script.reset_lsp();
        }
        return;
    };

    if let Some(doc) = visible
        && let Some(script) = script_mut(tabs, doc)
    {
        let uri = doc.uri();
        let synced = (state.generation, script.revision());
        if script.lsp.synced != Some(synced) {
            server.sync_document(&uri, LANGUAGE, script.revision(), &script.text());
            script.lsp.synced = Some(synced);
            state.open_docs.insert(uri.clone());
        }

        if std::mem::take(&mut script.lsp.want_completion) {
            let anchor = script.word_start();
            if let Some(request) = server.completion(&uri, script.cursor_position()) {
                state.requests.insert(
                    request,
                    PendingRequest {
                        uri: uri.clone(),
                        sent: Instant::now(),
                        kind: RequestKind::Completion { anchor },
                    },
                );
            }
        }

        // one hover in flight per editor; a newer cursor position is
        // picked up once it's answered
        let hover_pending = state
            .requests
            .values()
            .any(|r| matches!(r.kind, RequestKind::Hover) && r.uri == uri);
        if script.lsp.want_hover && !hover_pending && server.is_ready() {
            script.lsp.want_hover = false;
            if let Some(request) = server.hover(&uri, script.cursor_position()) {
                state.requests.insert(
                    request,
                    PendingRequest {
                        uri,
                        sent: Instant::now(),
                        kind: RequestKind::Hover,
                    },
                );
            }
        }
    }

    // editors whose tab was closed
    let gone: Vec<String> = state
        .open_docs
        .iter()
        .filter(|uri| DocRef::from_uri(uri).is_none_or(|doc| script_ref(tabs, doc).is_none()))
        .cloned()
        .collect();
    for uri in gone {
        server.close_document(&uri);
        state.open_docs.remove(&uri);
    }
}

fn apply_events(app: &mut Rustrest) {
    let Server::Running { server, provider } = &mut app.script_intel.server else {
        return;
    };
    let name = provider.name.clone();
    let events = server.poll();
    for event in events {
        match event {
            Event::Ready => {}
            Event::Diagnostics {
                uri, diagnostics, ..
            } => {
                if let Some(script) =
                    DocRef::from_uri(&uri).and_then(|doc| script_mut(&mut app.tabs, doc))
                {
                    script.lsp.diagnostics = Some(diagnostics);
                }
            }
            Event::Completions { request, items } => {
                let Some(pending) = app.script_intel.requests.remove(&request) else {
                    continue;
                };
                if let RequestKind::Completion { anchor } = pending.kind
                    && let Some(script) = DocRef::from_uri(&pending.uri)
                        .and_then(|doc| script_mut(&mut app.tabs, doc))
                {
                    script.open_completion(anchor, items);
                }
            }
            Event::Hover { request, contents } => {
                let Some(pending) = app.script_intel.requests.remove(&request) else {
                    continue;
                };
                if let Some(script) =
                    DocRef::from_uri(&pending.uri).and_then(|doc| script_mut(&mut app.tabs, doc))
                {
                    script.lsp.hover = contents;
                }
            }
            Event::Log(line) => log(app, line),
            Event::Exited(code) => {
                let code = code.map_or("killed".to_string(), |c| format!("exit code {c}"));
                log(app, format!("[{name}] language server exited ({code})"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_refs_round_trip() {
        for doc in [
            DocRef {
                owner: DocOwner::Tab(12),
                script: ScriptTab::PreRequest,
            },
            DocRef {
                owner: DocOwner::Collection(3),
                script: ScriptTab::PostResponse,
            },
        ] {
            assert_eq!(DocRef::parse(&doc.id()), Some(doc));
            assert_eq!(DocRef::from_uri(&doc.uri()), Some(doc));
        }
        assert_eq!(
            DocRef {
                owner: DocOwner::Tab(1),
                script: ScriptTab::PostResponse
            }
            .uri(),
            "file:///rustrest/scripts/tab-1-post/post-response.js"
        );
        assert_eq!(DocRef::parse("tab-x-pre"), None);
        assert_eq!(DocRef::parse("other-1-pre"), None);
        assert_eq!(
            DocRef::from_uri("file:///elsewhere/tab-1-pre/pre-request.js"),
            None
        );
    }
}
