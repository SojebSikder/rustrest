use super::{Rustrest, WorkspaceContent};
use crate::message::Message;
use crate::ui::tab::graphql::GraphQlTabMessage;
use crate::ui::tab::protocol_common::{LogEntry, graphql_subscription_log_id, push_capped};
use crate::ui::tab::streaming::spawn_streaming;
use iced::Task;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

fn active_gql_state(
    app: &mut Rustrest,
) -> Option<(usize, &mut crate::ui::tab::graphql::GraphQlTabState)> {
    let idx = app.active_tab_index;
    let tab_state = app.tabs.get_mut(idx)?;
    let tab_id = tab_state.tab.id;
    match &mut tab_state.content {
        WorkspaceContent::GraphQl(state) => Some((tab_id, state)),
        _ => None,
    }
}

fn parse_variables(text: &str) -> Option<serde_json::Value> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

fn is_graphql_edit(msg: &GraphQlTabMessage) -> bool {
    use iced::widget::text_editor::Action;
    match msg {
        GraphQlTabMessage::UrlChanged(_)
        | GraphQlTabMessage::HeaderChanged(_, _)
        | GraphQlTabMessage::AddHeader
        | GraphQlTabMessage::RemoveHeader(_)
        | GraphQlTabMessage::OperationNameChanged(_)
        | GraphQlTabMessage::OperationPicked(_, _)
        | GraphQlTabMessage::ToggleTreeNode(_)
        | GraphQlTabMessage::ExpandTreeNode(_) => true,
        GraphQlTabMessage::QueryAction(action) | GraphQlTabMessage::VariablesAction(action) => {
            matches!(action, Action::Edit(_))
        }
        _ => false,
    }
}

pub fn active_graphql_message(app: &mut Rustrest, msg: GraphQlTabMessage) -> Task<Message> {
    if is_graphql_edit(&msg) {
        let idx = app.active_tab_index;
        if let Some(tab_state) = app.tabs.get_mut(idx) {
            tab_state.tab.dirty = true;
        }
    }

    let Some((tab_id, state)) = active_gql_state(app) else {
        return Task::none();
    };

    match msg {
        GraphQlTabMessage::UrlChanged(url) => {
            state.url = url;
            Task::none()
        }
        GraphQlTabMessage::HeaderChanged(idx, kv) => {
            if let Some(row) = state.headers.get_mut(idx) {
                *row = kv;
            }
            Task::none()
        }
        GraphQlTabMessage::AddHeader => {
            state
                .headers
                .push(crate::ui::tab::types::KeyValuePair::new("", ""));
            Task::none()
        }
        GraphQlTabMessage::RemoveHeader(idx) => {
            if idx < state.headers.len() {
                state.headers.remove(idx);
            }
            Task::none()
        }
        GraphQlTabMessage::QueryAction(action) => {
            state.query.perform(action);
            Task::none()
        }
        GraphQlTabMessage::VariablesAction(action) => {
            state.variables.perform(action);
            Task::none()
        }
        GraphQlTabMessage::OperationNameChanged(name) => {
            state.operation_name = name;
            Task::none()
        }
        GraphQlTabMessage::Run => {
            state.is_loading = true;
            state.cancel_token = CancellationToken::new();
            let req = rustrest_graphql::GraphQlRequest {
                url: state.url.clone(),
                headers: state.active_headers(),
                query: state.query.text(),
                variables: parse_variables(&state.variables.text()),
                operation_name: (!state.operation_name.trim().is_empty())
                    .then(|| state.operation_name.clone()),
            };
            let cancel_token = state.cancel_token.clone();
            Task::perform(rustrest_graphql::execute(req, cancel_token), move |res| {
                Message::GraphQlResponseReceived(tab_id, res)
            })
        }
        GraphQlTabMessage::Cancel => {
            state.cancel_token.cancel();
            Task::none()
        }
        GraphQlTabMessage::FetchSchema => {
            state.schema_loading = true;
            let url = state.url.clone();
            let headers = state.active_headers();
            let cancel_token = CancellationToken::new();
            Task::perform(
                rustrest_graphql::introspect(url, headers, cancel_token),
                move |res| Message::GraphQlSchemaLoaded(tab_id, res),
            )
        }
        GraphQlTabMessage::Subscribe => {
            let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
            state.subscription_commands = Some(cmd_tx);
            state.subscribed = true;
            state.subscription_log.clear();
            let url = state.url.clone();
            let headers = state.active_headers();
            let query = state.query.text();
            let variables = parse_variables(&state.variables.text());
            let operation_name =
                (!state.operation_name.trim().is_empty()).then(|| state.operation_name.clone());

            spawn_streaming(
                move |events_tx| {
                    rustrest_graphql::run_subscription(
                        url,
                        headers,
                        query,
                        variables,
                        operation_name,
                        cmd_rx,
                        events_tx,
                    )
                },
                move |event| Message::GraphQlSubscriptionEvent(tab_id, event),
                move |_| {
                    Message::GraphQlSubscriptionEvent(
                        tab_id,
                        rustrest_graphql::SubscriptionEvent::Complete,
                    )
                },
            )
        }
        GraphQlTabMessage::StopSubscription => {
            if let Some(tx) = state.subscription_commands.take() {
                let _ = tx.send(rustrest_graphql::SubscriptionCommand::Stop);
            }
            state.subscribed = false;
            Task::none()
        }
        GraphQlTabMessage::OperationKindTabSelected(kind) => {
            state.selected_operation_kind = kind;
            Task::none()
        }
        GraphQlTabMessage::OperationPicked(kind, field_name) => {
            state.selected_field = Some((kind, field_name.clone()));
            state.selection_tree = state
                .schema
                .as_ref()
                .and_then(|schema| {
                    let field = schema
                        .operations(kind)
                        .into_iter()
                        .find(|f| f.name == field_name)?;
                    schema
                        .is_selectable(&field.named_type)
                        .then(|| schema.selection_children(&field.named_type))
                })
                .unwrap_or_default();
            regenerate_query(state);
            Task::none()
        }
        GraphQlTabMessage::ToggleTreeNode(path) => {
            if let Some(node) = rustrest_graphql::find_node_mut(&mut state.selection_tree, &path) {
                node.checked = !node.checked;
            }
            regenerate_query(state);
            Task::none()
        }
        GraphQlTabMessage::ExpandTreeNode(path) => {
            let schema = state.schema.clone();
            if let Some(node) = rustrest_graphql::find_node_mut(&mut state.selection_tree, &path) {
                node.expanded = !node.expanded;
                if node.expanded && node.children.is_empty() {
                    if let Some(schema) = &schema {
                        node.children = schema.selection_children(&node.named_type);
                    }
                }
            }
            regenerate_query(state);
            Task::none()
        }
    }
}

/// Rebuilds `state.query`/`state.variables` from `state.selected_field` and
/// the current checked/expanded state of `state.selection_tree`.
fn regenerate_query(state: &mut crate::ui::tab::graphql::GraphQlTabState) {
    let Some(schema) = &state.schema else { return };
    let Some((kind, field_name)) = &state.selected_field else {
        return;
    };
    let Some(field) = schema
        .operations(*kind)
        .into_iter()
        .find(|f| &f.name == field_name)
    else {
        return;
    };

    let (query, variables) = schema.build_query_from_selection(*kind, field, &state.selection_tree);
    state.query = iced::widget::text_editor::Content::with_text(&query);
    state.variables = iced::widget::text_editor::Content::with_text(&variables);
}

fn find_gql_state(
    app: &mut Rustrest,
    tab_id: usize,
) -> Option<&mut crate::ui::tab::graphql::GraphQlTabState> {
    let tab_state = app.tabs.iter_mut().find(|t| t.tab.id == tab_id)?;
    match &mut tab_state.content {
        WorkspaceContent::GraphQl(state) => Some(state),
        _ => None,
    }
}

pub fn response_received(
    app: &mut Rustrest,
    tab_id: usize,
    res: Result<rustrest_graphql::GraphQlResponse, String>,
) -> Task<Message> {
    if let Some(state) = find_gql_state(app, tab_id) {
        state.is_loading = false;
        state.response = Some(res.map(|r| r.body));
    }
    Task::none()
}

pub fn schema_loaded(
    app: &mut Rustrest,
    tab_id: usize,
    res: Result<rustrest_graphql::GraphQlResponse, String>,
) -> Task<Message> {
    if let Some(state) = find_gql_state(app, tab_id) {
        state.schema_loading = false;
        match res {
            Ok(resp) => match rustrest_graphql::Schema::parse(&resp.body) {
                Some(schema) => state.schema = Some(schema),
                None => {
                    state.response = Some(Err(
                        "Response did not contain a valid introspection schema".to_string(),
                    ))
                }
            },
            Err(e) => state.response = Some(Err(format!("Failed to fetch schema: {e}"))),
        }
    }
    Task::none()
}

pub fn subscription_event(
    app: &mut Rustrest,
    tab_id: usize,
    event: rustrest_graphql::SubscriptionEvent,
) -> Task<Message> {
    let Some(state) = find_gql_state(app, tab_id) else {
        return Task::none();
    };

    match event {
        rustrest_graphql::SubscriptionEvent::Connected => {
            push_capped(&mut state.subscription_log, LogEntry::info("Subscribed"));
        }
        rustrest_graphql::SubscriptionEvent::Data(value) => {
            let body = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
            push_capped(
                &mut state.subscription_log,
                LogEntry::incoming("Data", body),
            );
        }
        rustrest_graphql::SubscriptionEvent::Error(e) => {
            state.subscribed = false;
            push_capped(&mut state.subscription_log, LogEntry::error(e));
        }
        rustrest_graphql::SubscriptionEvent::Complete => {
            state.subscribed = false;
            push_capped(&mut state.subscription_log, LogEntry::info("Complete"));
        }
    }
    iced::widget::operation::snap_to_end(graphql_subscription_log_id(tab_id))
}
