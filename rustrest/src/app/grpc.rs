use super::{Rustrest, WorkspaceContent};
use crate::message::Message;
use crate::ui::tab::grpc::GrpcTabMessage;
use crate::ui::tab::protocol_common::{LogEntry, grpc_log_id, push_capped};
use crate::ui::tab::streaming::spawn_streaming;
use iced::Task;

fn scroll_to_bottom(tab_id: usize) -> Task<Message> {
    iced::widget::operation::snap_to_end(grpc_log_id(tab_id))
}

fn active_grpc_state(
    app: &mut Rustrest,
) -> Option<(usize, &mut crate::ui::tab::grpc::GrpcTabState)> {
    let idx = app.active_tab_index;
    let tab_state = app.tabs.get_mut(idx)?;
    let tab_id = tab_state.tab.id;
    match &mut tab_state.content {
        WorkspaceContent::Grpc(state) => Some((tab_id, state)),
        _ => None,
    }
}

fn is_grpc_edit(msg: &GrpcTabMessage) -> bool {
    match msg {
        GrpcTabMessage::EndpointChanged(_)
        | GrpcTabMessage::UseTlsToggled(_)
        | GrpcTabMessage::UseReflectionModeSelected
        | GrpcTabMessage::ProtoFilesPicked(_)
        | GrpcTabMessage::ServiceSelected(_)
        | GrpcTabMessage::MethodSelected(_)
        | GrpcTabMessage::MetadataChanged(_, _)
        | GrpcTabMessage::AddMetadata
        | GrpcTabMessage::RemoveMetadata(_)
        | GrpcTabMessage::Reset => true,
        GrpcTabMessage::RequestJsonAction(action) => {
            matches!(action, iced::widget::text_editor::Action::Edit(_))
        }
        _ => false,
    }
}

pub fn active_grpc_message(app: &mut Rustrest, msg: GrpcTabMessage) -> Task<Message> {
    if is_grpc_edit(&msg) {
        let idx = app.active_tab_index;
        if let Some(tab_state) = app.tabs.get_mut(idx) {
            tab_state.tab.dirty = true;
        }
    }

    let Some((tab_id, state)) = active_grpc_state(app) else {
        return Task::none();
    };

    match msg {
        GrpcTabMessage::EndpointChanged(endpoint) => {
            state.endpoint = endpoint;
            Task::none()
        }
        GrpcTabMessage::UseTlsToggled(use_tls) => {
            state.use_tls = use_tls;
            Task::none()
        }
        GrpcTabMessage::UseReflectionModeSelected => {
            state.proto_files.clear();
            Task::none()
        }
        GrpcTabMessage::PickProtoFiles => {
            if let Some(files) = rfd::FileDialog::new()
                .add_filter("Protocol Buffers", &["proto"])
                .pick_files()
            {
                return Task::done(Message::ActiveGrpcMessage(
                    GrpcTabMessage::ProtoFilesPicked(files),
                ));
            }
            Task::none()
        }
        GrpcTabMessage::ProtoFilesPicked(files) => {
            state.proto_files = files;
            Task::none()
        }
        GrpcTabMessage::Discover => {
            state.discovering = true;
            state.log.clear();
            let endpoint = state.endpoint.clone();
            let use_tls = state.use_tls;
            let source = if state.proto_files.is_empty() {
                rustrest_grpc::ProtoSource::Reflection
            } else {
                rustrest_grpc::ProtoSource::Files(state.proto_files.clone())
            };
            Task::perform(
                rustrest_grpc::discover(endpoint, use_tls, source),
                move |res| Message::GrpcDiscovered(tab_id, res),
            )
        }
        GrpcTabMessage::ServiceSelected(name) => {
            state.selected_service = Some(name);
            state.selected_method = None;
            Task::none()
        }
        GrpcTabMessage::MethodSelected(name) => {
            state.selected_method = Some(name);
            if let Some(method) = state.selected_method_info() {
                let skeleton = rustrest_grpc::json_skeleton(&method.input);
                let json = serde_json::to_string_pretty(&skeleton).unwrap_or_default();
                state.request_json = iced::widget::text_editor::Content::with_text(&json);
            }
            Task::none()
        }
        GrpcTabMessage::MetadataChanged(idx, kv) => {
            if let Some(row) = state.metadata.get_mut(idx) {
                *row = kv;
            }
            Task::none()
        }
        GrpcTabMessage::AddMetadata => {
            state
                .metadata
                .push(crate::ui::tab::types::KeyValuePair::new("", ""));
            Task::none()
        }
        GrpcTabMessage::RemoveMetadata(idx) => {
            if idx < state.metadata.len() {
                state.metadata.remove(idx);
            }
            Task::none()
        }
        GrpcTabMessage::RequestJsonAction(action) => {
            state.request_json.perform(action);
            Task::none()
        }
        GrpcTabMessage::Invoke => {
            let Some(method) = state.selected_method_info().cloned() else {
                return Task::none();
            };
            state.invoking = true;
            state.log.clear();
            let endpoint = state.endpoint.clone();
            let use_tls = state.use_tls;
            let request_json = state.request_json.text();
            let metadata = crate::ui::tab::protocol_common::active_pairs(&state.metadata);

            spawn_streaming(
                move |events_tx| {
                    rustrest_grpc::invoke(
                        endpoint,
                        use_tls,
                        method,
                        request_json,
                        metadata,
                        events_tx,
                    )
                },
                move |result| Message::GrpcResponse(tab_id, result),
                move |_| Message::GrpcInvokeFinished(tab_id),
            )
        }
        GrpcTabMessage::Reset => {
            *state = Default::default();
            Task::none()
        }
    }
}

fn find_grpc_state(
    app: &mut Rustrest,
    tab_id: usize,
) -> Option<&mut crate::ui::tab::grpc::GrpcTabState> {
    let tab_state = app.tabs.iter_mut().find(|t| t.tab.id == tab_id)?;
    match &mut tab_state.content {
        WorkspaceContent::Grpc(state) => Some(state),
        _ => None,
    }
}

pub fn discovered(
    app: &mut Rustrest,
    tab_id: usize,
    res: Result<rustrest_grpc::GrpcTarget, String>,
) -> Task<Message> {
    if let Some(state) = find_grpc_state(app, tab_id) {
        state.discovering = false;
        match res {
            Ok(target) => {
                push_capped(
                    &mut state.log,
                    LogEntry::info(format!("Discovered {} service(s)", target.services.len())),
                );
                state.target = Some(target);
            }
            Err(e) => {
                push_capped(&mut state.log, LogEntry::error(e));
            }
        }
    }
    scroll_to_bottom(tab_id)
}

pub fn response(app: &mut Rustrest, tab_id: usize, res: Result<String, String>) -> Task<Message> {
    if let Some(state) = find_grpc_state(app, tab_id) {
        match res {
            Ok(json) => push_capped(&mut state.log, LogEntry::incoming("Response", json)),
            Err(e) => push_capped(&mut state.log, LogEntry::error(e)),
        }
    }
    scroll_to_bottom(tab_id)
}

pub fn invoke_finished(app: &mut Rustrest, tab_id: usize) -> Task<Message> {
    if let Some(state) = find_grpc_state(app, tab_id) {
        state.invoking = false;
    }
    Task::none()
}
