//! The tab strip: opening/closing/renaming/reordering tabs (HTTP request,
//! terminal, remote file, collection root, plugin panel), routing widget
//! messages to the active tab, sending the active request (pre-request
//! script -> plugin hooks -> HTTP -> post-response script -> plugin hooks),
//! and the "Save Request" modal.

use super::{Rustrest, Tab, TabState, WorkspaceContent};
use crate::collection::collection::CollectionItem;
use crate::collection::env::Environment;
use crate::message::Message;
use crate::ui::context_menu::{ContextMenu, FieldTarget};
use crate::ui::save_request_model::types::SaveRequestModalState;
use crate::ui::tab::TabMessage;
use crate::ui::tab::types::{KeyValuePair, ResponseView};
use crate::ui::toast::toast::ToastStatus;
use crate::utils::{find_request_mut, format_json_or_fallback, insert_nested_request};
use iced::Task;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct WorkbenchState {
    pub dragging_tab_index: Option<usize>,
    pub last_tab_name_click: Option<(usize, std::time::Instant)>,
    pub tab_rename_input_hovered: bool,
    pub save_request_model: Option<SaveRequestModalState>,
}

/// finishes an in-progress tab rename: closes the inline editor, and falls
/// back to a content-appropriate default name if the user left it blank.
fn finalize_tab_rename(app: &mut Rustrest, idx: usize) {
    if let Some(tab_state) = app.tabs.get_mut(idx) {
        tab_state.is_editing_name = false;
        if tab_state.tab.name.trim().is_empty() {
            tab_state.tab.name = match &tab_state.content {
                WorkspaceContent::HttpRequest => "Untitled Request".to_string(),
                WorkspaceContent::CollectionRoot {
                    collection_name, ..
                } => collection_name.clone(),
                WorkspaceContent::Terminal { .. } => "Terminal".to_string(),
                WorkspaceContent::RemoteFile { path, .. } => path.clone(),
                WorkspaceContent::Plugin { panel_id, .. } => panel_id.clone(),
                WorkspaceContent::PluginManager => "Manage Plugins".to_string(),
                WorkspaceContent::WebSocket(_) => "WebSocket Request".to_string(),
                WorkspaceContent::GraphQl(_) => "GraphQL Request".to_string(),
                WorkspaceContent::Grpc(_) => "gRPC Request".to_string(),
            };
        }
    }

    app.workbench.tab_rename_input_hovered = false;
    app.sync_tab_to_collection(idx);
}

/// closes the tab at `index`, if any. closing the last remaining tab leaves
/// `app.workbench.tabs` empty, which shows the workspace's empty-state screen.
fn close_tab(app: &mut Rustrest, index: usize) {
    if let Some(tab_state) = app.tabs.get(index) {
        if tab_state.tab.is_loading {
            tab_state.tab.cancel_token.cancel();
        }
        if let WorkspaceContent::Terminal { terminal_id, .. } = tab_state.content {
            app.terminal.terminal_manager.close(terminal_id);
        }
    } else {
        return;
    }

    app.tabs.remove(index);
    if app.active_tab_index >= app.tabs.len() && !app.tabs.is_empty() {
        app.active_tab_index = app.tabs.len() - 1;
    }
}

pub fn new_tab_pressed(app: &mut Rustrest) -> Task<Message> {
    app.tabs.push(TabState {
        tab: Tab::new(app.next_tab_id),
        content: WorkspaceContent::HttpRequest,
        is_editing_name: false,
    });
    app.active_tab_index = app.tabs.len() - 1;
    app.next_tab_id += 1;
    iced::widget::operation::snap_to_end(crate::ui::workspace::tab_bar_scroll_id())
}

pub fn new_protocol_tab_pressed(
    app: &mut Rustrest,
    protocol: super::NewTabProtocol,
) -> Task<Message> {
    let content = match protocol {
        super::NewTabProtocol::Http => return new_tab_pressed(app),
        super::NewTabProtocol::WebSocket => {
            WorkspaceContent::WebSocket(crate::ui::tab::ws::WsTabState::default())
        }
        super::NewTabProtocol::GraphQl => {
            WorkspaceContent::GraphQl(crate::ui::tab::graphql::GraphQlTabState::default())
        }
        super::NewTabProtocol::Grpc => {
            WorkspaceContent::Grpc(crate::ui::tab::grpc::GrpcTabState::default())
        }
    };

    let mut tab = Tab::new(app.next_tab_id);
    tab.name = match protocol {
        super::NewTabProtocol::Http => "Untitled Request".to_string(),
        super::NewTabProtocol::WebSocket => "WebSocket Request".to_string(),
        super::NewTabProtocol::GraphQl => "GraphQL Request".to_string(),
        super::NewTabProtocol::Grpc => "gRPC Request".to_string(),
    };

    app.tabs.push(TabState {
        tab,
        content,
        is_editing_name: false,
    });
    app.active_tab_index = app.tabs.len() - 1;
    app.next_tab_id += 1;
    iced::widget::operation::snap_to_end(crate::ui::workspace::tab_bar_scroll_id())
}

pub fn close_tab_pressed(app: &mut Rustrest, index: usize) -> Task<Message> {
    close_tab(app, index);
    Task::none()
}

pub fn new_terminal_tab_pressed(app: &mut Rustrest) -> Task<Message> {
    let tx = app.terminal.terminal_event_tx.clone();
    match app
        .terminal
        .terminal_manager
        .spawn(80, 24, move |id, notice| {
            let _ = tx.send((id, notice));
        }) {
        Ok(terminal_id) => {
            let widget_id = iced::widget::Id::unique();
            let mut term_tab = Tab::new(app.next_tab_id);
            term_tab.name = format!("Terminal {}", terminal_id + 1);
            app.tabs.push(TabState {
                tab: term_tab,
                content: WorkspaceContent::Terminal {
                    terminal_id,
                    widget_id: widget_id.clone(),
                },
                is_editing_name: false,
            });
            app.next_tab_id += 1;
            app.active_tab_index = app.tabs.len() - 1;

            Task::batch([
                iced::widget::operation::snap_to_end(crate::ui::workspace::tab_bar_scroll_id()),
                iced::widget::operation::focus(widget_id),
            ])
        }
        Err(err) => Task::done(Message::ShowToast(
            format!("Failed to start terminal: {err}"),
            ToastStatus::Error,
        )),
    }
}

pub fn terminal_input(app: &mut Rustrest, id: u64, bytes: Vec<u8>) -> Task<Message> {
    if let Some(session) = app.terminal.terminal_manager.get(id) {
        session.scroll_to_bottom();
        session.write(bytes);
    }
    Task::none()
}

pub fn terminal_resized(
    app: &mut Rustrest,
    id: u64,
    columns: usize,
    rows: usize,
    cell_width: u16,
    cell_height: u16,
) -> Task<Message> {
    app.terminal
        .terminal_manager
        .resize(id, columns, rows, cell_width, cell_height);
    Task::none()
}

pub fn terminal_notice(
    app: &mut Rustrest,
    id: u64,
    notice: rustrest_terminal::TerminalNotice,
) -> Task<Message> {
    if notice == rustrest_terminal::TerminalNotice::Exited {
        if let Some(idx) = app.tabs.iter().position(|t| {
            matches!(t.content, WorkspaceContent::Terminal { terminal_id, .. } if terminal_id == id)
        }) {
            close_tab(app, idx);
        }
    }
    Task::none()
}

pub fn close_active_tab_shortcut(app: &mut Rustrest) -> Task<Message> {
    close_tab(app, app.active_tab_index);
    Task::none()
}

pub fn active_tab_message(app: &mut Rustrest, tab_msg: TabMessage) -> Task<Message> {
    if let TabMessage::ShowFieldContextMenu(target, value) = tab_msg {
        app.overlays.active_context_menu = Some(ContextMenu::TextField {
            target: FieldTarget::Tab(target),
            current_value: value,
        });
        app.overlays.context_menu_position = app.cursor_position;
        return Task::none();
    }
    if let TabMessage::ShowResponseTimingModal(saved_idx) = tab_msg {
        if let Some(tab_state) = app.tabs.get(app.active_tab_index) {
            let tab = &tab_state.tab;
            app.overlays.response_timing_modal = match saved_idx {
                None => tab
                    .response
                    .as_ref()
                    .and_then(|res| res.as_ref().ok())
                    .map(
                        |resp| crate::ui::response_timing_modal::ResponseTimingModalState {
                            status: resp.status,
                            timings: resp.timings,
                            request_size: resp.request_size,
                            response_size: resp.response_size,
                        },
                    ),
                Some(idx) => tab.saved_responses.get(idx).map(|saved| {
                    crate::ui::response_timing_modal::ResponseTimingModalState {
                        status: saved.status,
                        timings: saved.timings,
                        request_size: saved.request_size,
                        response_size: saved.response_size,
                    }
                }),
            };
        }
        return Task::none();
    }
    let active_idx = app.active_tab_index;
    let mut syncs_saved_responses = false;
    if let Some(tab_state) = app.tabs.get_mut(active_idx) {
        if let TabMessage::ResponseViewChanged(view) = tab_msg {
            tab_state.tab.response_view = view;
            if let Some(Ok(resp)) = &tab_state.tab.response {
                let body_text = match view {
                    ResponseView::Json => format_json_or_fallback(&resp.body),
                    ResponseView::Raw => resp.body.clone(),
                };
                tab_state.tab.response_body_editor =
                    iced::widget::text_editor::Content::with_text(&body_text);
            }
        } else {
            syncs_saved_responses = matches!(
                tab_msg,
                TabMessage::SaveResponse | TabMessage::DeleteSavedResponse(_)
            );
            tab_state.tab.update(tab_msg);
        }
    }
    // saving/deleting a response snapshot mutates the request's saved-response
    // list directly, so mirror it into the collection tree (sidebar) right away
    // instead of waiting for an explicit "Save Request".
    if syncs_saved_responses {
        app.sync_active_tab_saved_responses_to_collection(active_idx);
    }
    Task::none()
}

pub fn send_pressed(app: &mut Rustrest) -> Task<Message> {
    if let Some(tab_state) = app.tabs.get_mut(app.active_tab_index) {
        if !matches!(tab_state.content, WorkspaceContent::HttpRequest) {
            return Task::none();
        }
        let tab = &mut tab_state.tab;
        if tab.is_loading || tab.url.is_empty() {
            return Task::none();
        }

        app.layout.console_logs.clear();

        // build variable/header maps for the pre-request script
        let mut script_vars: std::collections::HashMap<String, String> = app
            .env
            .active_env_index
            .and_then(|i| app.env.environments.get(i))
            .map(|e| {
                e.variables
                    .iter()
                    .filter(|v| v.is_active)
                    .map(|v| (v.key.clone(), v.value.clone()))
                    .collect()
            })
            .unwrap_or_default();

        if let Some(c_id) = tab.collection_id {
            if let Some(col) = app.collections.iter().find(|c| c.id == c_id) {
                for kv in col.get_native_variables() {
                    script_vars.insert(kv.key, kv.value);
                }
            }
        }

        let mut script_headers: std::collections::HashMap<String, String> = tab
            .request_headers
            .iter()
            .filter(|h| h.is_active)
            .map(|h| (h.key.clone(), h.value.clone()))
            .collect();

        let mut script_globals: std::collections::HashMap<String, String> = app
            .env
            .globals
            .iter()
            .filter(|v| v.is_active)
            .map(|v| (v.key.clone(), v.value.clone()))
            .collect();

        let script_vars_snapshot = script_vars.clone();

        let pre_script_text = tab.pre_request_script.text();
        match crate::script_engine::ScriptRunner::run_pre_request(
            &pre_script_text,
            &mut script_vars,
            &mut script_headers,
            &mut script_globals,
        ) {
            Ok(logs) => app.layout.console_logs.extend(logs),
            Err(e) => return Task::done(Message::ShowToast(e, ToastStatus::Error)),
        }

        for (k, v) in &script_globals {
            if let Some(existing) = app.env.globals.iter_mut().find(|kv| &kv.key == k) {
                existing.value = v.clone();
                existing.is_active = true;
            } else {
                let mut kv = KeyValuePair::new(k, v);
                kv.is_active = true;
                app.env.globals.push(kv);
            }
        }

        if let Some(idx) = app.env.active_env_index {
            if let Some(env) = app.env.environments.get_mut(idx) {
                for (k, v) in &script_vars {
                    if script_vars_snapshot.get(k) != Some(v) {
                        if let Some(existing) = env.variables.iter_mut().find(|kv| &kv.key == k) {
                            existing.value = v.clone();
                            existing.is_active = true;
                        } else {
                            let mut kv = KeyValuePair::new(k, v);
                            kv.is_active = true;
                            env.variables.push(kv);
                        }
                    }
                }
            }
        }

        let mut effective_env = app
            .env
            .active_env_index
            .and_then(|i| app.env.environments.get(i))
            .cloned()
            .unwrap_or_else(|| Environment::new("__script"));

        // globals are the lowest-precedence variable source; environment/script
        // variables (applied next) override them for the same key
        for (k, v) in &script_globals {
            if !effective_env.variables.iter().any(|kv| &kv.key == k) {
                let mut kv = KeyValuePair::new(k, v);
                kv.is_active = true;
                effective_env.variables.push(kv);
            }
        }

        for (k, v) in &script_vars {
            if let Some(existing) = effective_env.variables.iter_mut().find(|kv| &kv.key == k) {
                existing.value = v.clone();
                existing.is_active = true;
            } else {
                let mut kv = KeyValuePair::new(k, v);
                kv.is_active = true;
                effective_env.variables.push(kv);
            }
        }

        let tab_id = tab.id;
        tab.cancel_token = CancellationToken::new();
        tab.is_loading = true;
        tab.response = None;
        tab.sse_active = false;
        tab.sse_log.clear();

        let collection_vars = tab
            .collection_id
            .and_then(|c_id| app.collections.iter().find(|c| c.id == c_id))
            .map(|c| c.get_native_variables());

        let (
            final_url,
            compiled_body,
            compiled_form_data,
            mut filtered_headers,
            filtered_cookies,
            compiled_auth,
        ) = tab.compile_request_fields(&Some(effective_env), collection_vars.as_deref());

        // apply any header overrides the script made via pm.setHeader(...)
        for (k, v) in script_headers {
            if let Some(existing) = filtered_headers.iter_mut().find(|(hk, _)| hk == &k) {
                existing.1 = v;
            } else {
                filtered_headers.push((k, v));
            }
        }

        // native plugin request hooks run after the per-request JS
        // script, as an app-wide policy layer (e.g. injecting an
        // auth header for every request). method changes aren't
        // applied back in v1 - only url/headers/body/variables are.
        let plugin_ctx = rustrest_plugin_host::RequestContext {
            method: tab.method.to_string(),
            url: final_url,
            headers: filtered_headers,
            body: compiled_body,
            variables: script_vars.clone(),
        };
        let plugin_ctx = app.plugins.plugin_manager.run_pre_request_hooks(plugin_ctx);
        app.layout
            .console_logs
            .extend(app.plugins.plugin_manager.drain_logs());
        let final_url = plugin_ctx.url;
        let filtered_headers = plugin_ctx.headers;
        let compiled_body = plugin_ctx.body;

        let spec = crate::http_client::RequestSpec::new(final_url, tab.method.clone())
            .body_type(tab.body_type)
            .raw_body(compiled_body)
            .form_data(compiled_form_data)
            .binary_file_path(tab.binary_file_path.clone())
            .headers(filtered_headers)
            .cookies(filtered_cookies)
            .auth_raw(compiled_auth);

        let cancel_token = tab.cancel_token.clone();

        return crate::ui::tab::streaming::spawn_streaming(
            move |events_tx| async move {
                let stream_cancel_token = cancel_token.clone();

                match crate::http_client::send_request_auto(spec, cancel_token).await {
                    Ok(crate::http_client::SendOutcome::Complete(resp)) => {
                        let _ = events_tx.send(SendProgress::Complete(Ok(resp))).await;
                    }
                    Ok(crate::http_client::SendOutcome::EventStream {
                        status,
                        headers,
                        mut body,
                    }) => {
                        let _ = events_tx
                            .send(SendProgress::Sse(rustrest_sse::SseEvent::Open {
                                status,
                                headers: headers.into_iter().collect(),
                            }))
                            .await;
                        let mut parser = rustrest_sse::SseParser::new();
                        loop {
                            let chunk = tokio::select! {
                                c = body.next_chunk() => c,
                                _ = stream_cancel_token.cancelled() => {
                                    let _ = events_tx
                                        .send(SendProgress::Sse(rustrest_sse::SseEvent::Closed))
                                        .await;
                                    break;
                                }
                            };
                            match chunk {
                                Some(Ok(bytes)) => {
                                    for msg in parser.feed(&bytes) {
                                        let _ = events_tx
                                            .send(SendProgress::Sse(
                                                rustrest_sse::SseEvent::Message(msg),
                                            ))
                                            .await;
                                    }
                                }
                                Some(Err(e)) => {
                                    let _ = events_tx
                                        .send(SendProgress::Sse(rustrest_sse::SseEvent::Error(e)))
                                        .await;
                                    break;
                                }
                                None => {
                                    let _ = events_tx
                                        .send(SendProgress::Sse(rustrest_sse::SseEvent::Closed))
                                        .await;
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = events_tx.send(SendProgress::Complete(Err(e))).await;
                    }
                }
            },
            move |progress| match progress {
                SendProgress::Complete(res) => Message::ResponseReceived(tab_id, res),
                SendProgress::Sse(event) => Message::SseEvent(tab_id, event),
            },
            move |_| Message::None,
        );
    }
    Task::none()
}

enum SendProgress {
    Complete(Result<crate::http_client::HttpResponse, String>),
    Sse(rustrest_sse::SseEvent),
}

pub fn sse_stream_event(
    app: &mut Rustrest,
    tab_id: usize,
    event: rustrest_sse::SseEvent,
) -> Task<Message> {
    let Some(tab_state) = app.tabs.iter_mut().find(|t| t.tab.id == tab_id) else {
        return Task::none();
    };
    if !matches!(tab_state.content, WorkspaceContent::HttpRequest) {
        return Task::none();
    }
    let tab = &mut tab_state.tab;

    use crate::ui::tab::protocol_common::{LogEntry, push_capped};

    match event {
        rustrest_sse::SseEvent::Open { status, headers } => {
            tab.response = Some(Ok(crate::http_client::HttpResponse {
                status,
                body: String::new(),
                headers: headers.into_iter().collect(),
                elapsed: std::time::Duration::ZERO,
                test_results: Vec::new(),
                timings: Default::default(),
                request_size: 0,
                response_size: 0,
            }));
            tab.sse_active = true;
            tab.sse_log.clear();
            push_capped(
                &mut tab.sse_log,
                LogEntry::info(format!("Connected (HTTP {status})")),
            );
        }
        rustrest_sse::SseEvent::Message(msg) => {
            let label = match &msg.id {
                Some(id) => format!("{} (id: {id})", msg.event),
                None => msg.event.clone(),
            };
            push_capped(&mut tab.sse_log, LogEntry::incoming(label, msg.data));
        }
        rustrest_sse::SseEvent::Error(e) => {
            tab.is_loading = false;
            push_capped(&mut tab.sse_log, LogEntry::error(e));
        }
        rustrest_sse::SseEvent::Closed => {
            tab.is_loading = false;
            push_capped(&mut tab.sse_log, LogEntry::info("Closed"));
        }
    }

    iced::widget::operation::snap_to_end(crate::ui::tab::protocol_common::sse_log_id(tab_id))
}

pub fn response_received(
    app: &mut Rustrest,
    tab_id: usize,
    res: Result<crate::http_client::HttpResponse, String>,
) -> Task<Message> {
    if let Some(tab_state) = app.tabs.iter_mut().find(|t| t.tab.id == tab_id) {
        let tab = &mut tab_state.tab;
        tab.is_loading = false;

        match &res {
            Ok(resp) => {
                let initial_body = match tab.response_view {
                    ResponseView::Json => format_json_or_fallback(&resp.body),
                    ResponseView::Raw => resp.body.clone(),
                };
                tab.response_body_editor =
                    iced::widget::text_editor::Content::with_text(&initial_body);
            }
            Err(err_msg) => {
                tab.response_body_editor = iced::widget::text_editor::Content::with_text(err_msg);
            }
        }

        let mut test_results = Vec::new();
        if let Ok(resp) = &res {
            let script_text = tab.post_response_script.text();
            if !script_text.trim().is_empty() {
                let mut base_vars: std::collections::HashMap<String, String> = app
                    .env
                    .active_env_index
                    .and_then(|idx| app.env.environments.get(idx))
                    .map(|e| {
                        e.variables
                            .iter()
                            .filter(|v| v.is_active)
                            .map(|v| (v.key.clone(), v.value.clone()))
                            .collect()
                    })
                    .unwrap_or_default();

                if let Some(c_id) = tab.collection_id {
                    if let Some(col) = app.collections.iter().find(|c| c.id == c_id) {
                        for kv in col.get_native_variables() {
                            base_vars.entry(kv.key).or_insert(kv.value);
                        }
                    }
                }

                let base_vars_snapshot = base_vars.clone();

                let base_globals: std::collections::HashMap<String, String> = app
                    .env
                    .globals
                    .iter()
                    .filter(|v| v.is_active)
                    .map(|v| (v.key.clone(), v.value.clone()))
                    .collect();

                let exec_ctx = crate::script_engine::ScriptExecutionContext {
                    variables: base_vars,
                    globals: base_globals,
                    response_status: resp.status,
                    response_body: resp.body.clone(),
                    response_headers: resp.headers.clone(),
                };

                match crate::script_engine::ScriptRunner::run_post_response(&script_text, &exec_ctx)
                {
                    Ok((updated_vars, updated_globals, results, logs)) => {
                        app.layout.console_logs.extend(logs);
                        test_results = results;
                        if let Some(idx) = app.env.active_env_index {
                            if let Some(env) = app.env.environments.get_mut(idx) {
                                for (k, v) in updated_vars {
                                    if base_vars_snapshot.get(&k) == Some(&v) {
                                        continue;
                                    }
                                    if let Some(existing) =
                                        env.variables.iter_mut().find(|kv| kv.key == k)
                                    {
                                        existing.value = v;
                                        existing.is_active = true;
                                    } else {
                                        let mut kv = KeyValuePair::new(&k, &v);
                                        kv.is_active = true;
                                        env.variables.push(kv);
                                    }
                                }
                            }
                        }
                        for (k, v) in updated_globals {
                            if let Some(existing) =
                                app.env.globals.iter_mut().find(|kv| kv.key == k)
                            {
                                existing.value = v;
                                existing.is_active = true;
                            } else {
                                let mut kv = KeyValuePair::new(&k, &v);
                                kv.is_active = true;
                                app.env.globals.push(kv);
                            }
                        }
                    }
                    Err(e) => {
                        let toast_task = crate::ui::toast::toast::show_and_schedule(
                            &mut app.overlays.toast_manager,
                            e,
                            ToastStatus::Error,
                            crate::ui::toast::toast::TOAST_DURATION,
                        );
                        tab.response = Some(res);
                        return toast_task;
                    }
                }
            }
        }

        // native plugin response hooks run after the per-request JS
        // test script, same app-wide-policy ordering as the
        // pre-request side. response status/headers/body are
        // read-only here (mirroring `pm.response` in JS scripts);
        // only variables and additional test results are applied back.
        if let Ok(resp) = &res {
            let plugin_vars: std::collections::HashMap<String, String> = app
                .env
                .active_env_index
                .and_then(|idx| app.env.environments.get(idx))
                .map(|e| {
                    e.variables
                        .iter()
                        .filter(|v| v.is_active)
                        .map(|v| (v.key.clone(), v.value.clone()))
                        .collect()
                })
                .unwrap_or_default();
            let plugin_vars_snapshot = plugin_vars.clone();

            let plugin_ctx = rustrest_plugin_host::ResponseContext {
                status: resp.status,
                headers: resp.headers.clone(),
                body: resp.body.clone(),
                variables: plugin_vars,
                test_results: Vec::new(),
            };
            let plugin_ctx = app
                .plugins
                .plugin_manager
                .run_post_response_hooks(plugin_ctx);
            app.layout
                .console_logs
                .extend(app.plugins.plugin_manager.drain_logs());

            if let Some(idx) = app.env.active_env_index {
                if let Some(env) = app.env.environments.get_mut(idx) {
                    for (k, v) in plugin_ctx.variables {
                        if plugin_vars_snapshot.get(&k) == Some(&v) {
                            continue;
                        }
                        if let Some(existing) = env.variables.iter_mut().find(|kv| kv.key == k) {
                            existing.value = v;
                            existing.is_active = true;
                        } else {
                            let mut kv = KeyValuePair::new(&k, &v);
                            kv.is_active = true;
                            env.variables.push(kv);
                        }
                    }
                }
            }

            test_results.extend(plugin_ctx.test_results.into_iter().map(|t| {
                crate::http_client::TestResult {
                    name: t.name,
                    passed: t.passed,
                }
            }));
        }

        let mut res = res;
        if let Ok(resp) = &mut res {
            resp.test_results = test_results;
        }
        tab.response = Some(res);
    }
    Task::none()
}

pub fn tab_name_double_click(app: &mut Rustrest, idx: usize) -> Task<Message> {
    const DOUBLE_CLICK_THRESHOLD: std::time::Duration = std::time::Duration::from_millis(400);

    if idx < app.tabs.len() {
        app.active_tab_index = idx;
    }

    let is_double_click = matches!(
        app.workbench.last_tab_name_click,
        Some((last_idx, last_time))
            if last_idx == idx && last_time.elapsed() < DOUBLE_CLICK_THRESHOLD
    );

    if is_double_click {
        app.workbench.last_tab_name_click = None;
        if let Some(tab_state) = app.tabs.get_mut(idx) {
            tab_state.is_editing_name = true;
        }
        // the double click happened right on the name, so the
        // cursor is over the input the moment it appears
        app.workbench.tab_rename_input_hovered = true;
    } else {
        app.workbench.last_tab_name_click = Some((idx, std::time::Instant::now()));
    }
    Task::none()
}

pub fn tab_name_changed(app: &mut Rustrest, idx: usize, new_name: String) -> Task<Message> {
    if let Some(tab_state) = app.tabs.get_mut(idx) {
        tab_state.tab.name = new_name;
        tab_state.tab.dirty = true;
    }
    Task::none()
}

pub fn tab_name_save(app: &mut Rustrest, idx: usize) -> Task<Message> {
    finalize_tab_rename(app, idx);
    Task::none()
}

pub fn tab_rename_blur(app: &mut Rustrest) -> Task<Message> {
    if !app.workbench.tab_rename_input_hovered {
        if let Some(idx) = app.tabs.iter().position(|t| t.is_editing_name) {
            finalize_tab_rename(app, idx);
        }
    }
    Task::none()
}

pub fn tab_rename_input_hover(app: &mut Rustrest, is_hovered: bool) -> Task<Message> {
    app.workbench.tab_rename_input_hovered = is_hovered;
    Task::none()
}

pub fn tab_drag_started(app: &mut Rustrest, idx: usize) -> Task<Message> {
    app.active_tab_index = idx;
    app.workbench.dragging_tab_index = Some(idx);
    Task::none()
}

pub fn tab_drag_entered(app: &mut Rustrest, idx: usize) -> Task<Message> {
    if let Some(from) = app.workbench.dragging_tab_index {
        if from != idx && from < app.tabs.len() && idx < app.tabs.len() {
            let was_active = app.active_tab_index;
            let moved_was_active = was_active == from;

            let tab = app.tabs.remove(from);
            app.tabs.insert(idx, tab);
            app.workbench.dragging_tab_index = Some(idx);

            app.active_tab_index = if moved_was_active {
                idx
            } else if from < was_active && idx >= was_active {
                was_active - 1
            } else if from > was_active && idx <= was_active {
                was_active + 1
            } else {
                was_active
            };
        }
    }
    Task::none()
}

pub fn tab_drag_ended(app: &mut Rustrest) -> Task<Message> {
    app.workbench.dragging_tab_index = None;
    Task::none()
}

pub fn save_request_pressed(app: &mut Rustrest, tab_idx: usize) -> Task<Message> {
    if let Some(tab_state) = app.tabs.get(tab_idx) {
        if matches!(tab_state.content, WorkspaceContent::HttpRequest) {
            // if this request is already saved into a collection, sync its
            // current state back in place and flush the collection to disk
            // if it already has a known save location (git folder / file).
            if let (Some(req_id), Some(col_id)) =
                (tab_state.tab.request_id, tab_state.tab.collection_id)
            {
                app.sync_tab_to_collection(tab_idx);
                if let Some(tab_state) = app.tabs.get_mut(tab_idx) {
                    tab_state.tab.dirty = false;
                }
                if let Some(col) = app.collections.iter_mut().find(|c| c.id == col_id) {
                    if let Some(node) = find_request_mut(&mut col.item, req_id) {
                        node.unsaved = false;
                    }
                }
                return super::collections::persist_if_known_location(
                    app,
                    col_id,
                    "Request updated".to_string(),
                );
            }

            let default_name = if tab_state.tab.name.trim().is_empty() {
                "Untitled Request".to_string()
            } else {
                tab_state.tab.name.clone()
            };

            app.workbench.save_request_model = Some(SaveRequestModalState {
                tab_index: tab_idx,
                request_name: default_name,
                selected_collection_id: tab_state
                    .tab
                    .collection_id
                    .or_else(|| app.collections.first().map(|c| c.id)),
                selected_folder_path: Vec::new(),
            });
        }
    }
    Task::none()
}

pub fn save_request_modal_collection_selected(app: &mut Rustrest, col_id: usize) -> Task<Message> {
    if let Some(modal) = app.workbench.save_request_model.as_mut() {
        modal.selected_collection_id = Some(col_id);
        modal.selected_folder_path.clear();
    }
    Task::none()
}

pub fn save_request_modal_folder_selected(app: &mut Rustrest, path: Vec<String>) -> Task<Message> {
    if let Some(modal) = app.workbench.save_request_model.as_mut() {
        modal.selected_folder_path = path;
    }
    Task::none()
}

pub fn save_request_name_changed(app: &mut Rustrest, name: String) -> Task<Message> {
    if let Some(modal) = app.workbench.save_request_model.as_mut() {
        modal.request_name = name;
    }
    Task::none()
}

pub fn close_save_request_modal(app: &mut Rustrest) -> Task<Message> {
    app.workbench.save_request_model = None;
    Task::none()
}

pub fn save_request_confirmed(app: &mut Rustrest) -> Task<Message> {
    let Some(modal) = app.workbench.save_request_model.take() else {
        return Task::none();
    };
    let Some(col_id) = modal.selected_collection_id else {
        return Task::none();
    };

    let name = if modal.request_name.trim().is_empty() {
        "Untitled Request".to_string()
    } else {
        modal.request_name.clone()
    };

    let already_linked_req_id = app.tabs.get(modal.tab_index).and_then(|t| t.tab.request_id);

    if let Some(req_id) = already_linked_req_id {
        if let Some(tab_state) = app.tabs.get_mut(modal.tab_index) {
            tab_state.tab.name = name;
            tab_state.tab.dirty = false;
        }
        app.sync_tab_to_collection(modal.tab_index);
        if let Some(col) = app.collections.iter_mut().find(|c| c.id == col_id) {
            if let Some(node) = find_request_mut(&mut col.item, req_id) {
                node.unsaved = false;
            }
        }
        return super::collections::persist_if_known_location(
            app,
            col_id,
            "Request updated".to_string(),
        );
    }

    let req_id = app.next_request_id;
    app.next_request_id += 1;

    let request_node = if let Some(tab_state) = app.tabs.get_mut(modal.tab_index) {
        tab_state.tab.name = name.clone();
        tab_state.tab.request_id = Some(req_id);
        tab_state.tab.collection_id = Some(col_id);
        tab_state.tab.dirty = false;
        Some(tab_state.tab.to_postman_request_node(req_id, &name))
    } else {
        None
    };

    if let Some(request_node) = request_node {
        if let Some(col) = app.collections.iter_mut().find(|c| c.id == col_id) {
            insert_nested_request(
                &mut col.item,
                &modal.selected_folder_path,
                CollectionItem::Request(request_node),
            );

            let col_name = col.info.name.clone();
            return Task::done(Message::ShowToast(
                format!("Request saved to '{}'", col_name),
                ToastStatus::Success,
            ));
        }
    }

    Task::none()
}

pub fn save_active_request_shortcut(app: &mut Rustrest) -> Task<Message> {
    if let Some(tab_state) = app.tabs.get(app.active_tab_index) {
        match &tab_state.content {
            WorkspaceContent::HttpRequest => {
                super::update(app, Message::SaveRequestPressed(app.active_tab_index))
            }
            WorkspaceContent::CollectionRoot { collection_id, .. } => {
                super::update(app, Message::SaveCollectionPressed(*collection_id))
            }
            WorkspaceContent::Terminal { .. } => Task::none(),
            WorkspaceContent::RemoteFile { .. } => {
                let tab_id = tab_state.tab.id;
                super::update(app, Message::RemoteFileSavePressed(tab_id))
            }
            WorkspaceContent::Plugin { .. } => Task::none(),
            WorkspaceContent::PluginManager => Task::none(),
            WorkspaceContent::WebSocket(_)
            | WorkspaceContent::GraphQl(_)
            | WorkspaceContent::Grpc(_) => Task::none(),
        }
    } else {
        Task::none()
    }
}
