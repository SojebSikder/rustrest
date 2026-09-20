use super::protocol_common::{LogEntry, active_pairs, message_log, simple_kv_editor, url_bar};
use super::types::KeyValuePair;
use crate::message::MultilineFieldKind;
use crate::ui::multiline_input::multiline_input;
use crate::ui::resize_handle::{DividerOrientation, resize_handle};
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};
use rustrest_graphql::{OperationKind, Schema, SelectionNode};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct GraphQlTabState {
    pub url: String,
    pub headers: Vec<KeyValuePair>,
    pub query: iced::widget::text_editor::Content,
    pub variables: iced::widget::text_editor::Content,
    pub operation_name: String,
    pub response: Option<Result<String, String>>,
    pub is_loading: bool,
    pub cancel_token: CancellationToken,
    pub schema: Option<Schema>,
    pub schema_loading: bool,
    pub selected_operation_kind: OperationKind,
    /// the root field currently being built in the explorer tree, if any.
    pub selected_field: Option<(OperationKind, String)>,
    /// the checkbox tree for `selected_field`'s return type.
    pub selection_tree: Vec<SelectionNode>,
    pub subscription_log: Vec<LogEntry>,
    pub subscribed: bool,
    pub subscription_commands:
        Option<tokio::sync::mpsc::UnboundedSender<rustrest_graphql::SubscriptionCommand>>,
}

impl Default for GraphQlTabState {
    fn default() -> Self {
        Self {
            url: String::new(),
            headers: vec![KeyValuePair::new("Content-Type", "application/json")],
            query: iced::widget::text_editor::Content::with_text("query {\n  \n}"),
            variables: iced::widget::text_editor::Content::with_text("{}"),
            operation_name: String::new(),
            response: None,
            is_loading: false,
            cancel_token: CancellationToken::new(),
            schema: None,
            schema_loading: false,
            selected_operation_kind: OperationKind::Query,
            selected_field: None,
            selection_tree: Vec::new(),
            subscription_log: Vec::new(),
            subscribed: false,
            subscription_commands: None,
        }
    }
}

impl GraphQlTabState {
    pub fn active_headers(&self) -> Vec<(String, String)> {
        active_pairs(&self.headers)
    }

    pub fn is_subscription(&self) -> bool {
        self.query.text().trim_start().starts_with("subscription")
    }
}

#[derive(Debug, Clone)]
pub enum GraphQlTabMessage {
    UrlChanged(String),
    HeaderChanged(usize, KeyValuePair),
    AddHeader,
    RemoveHeader(usize),
    QueryAction(iced::widget::text_editor::Action),
    VariablesAction(iced::widget::text_editor::Action),
    OperationNameChanged(String),
    Run,
    Cancel,
    FetchSchema,
    Subscribe,
    StopSubscription,
    /// switches which operation list (queries/mutations/subscriptions) is shown.
    OperationKindTabSelected(OperationKind),
    /// a root field was clicked in the operations list; starts building it
    /// in the explorer tree.
    OperationPicked(OperationKind, String),
    /// toggles whether the field at this path (root-first child indices) is
    /// included in the generated selection set.
    ToggleTreeNode(Vec<usize>),
    /// expands/collapses an object-typed field in the tree, lazily fetching
    /// its children from the schema the first time it's expanded.
    ExpandTreeNode(Vec<usize>),
}

#[allow(clippy::too_many_arguments)]
pub fn view<'a>(
    state: &'a GraphQlTabState,
    tab_id: usize,
    wrap: impl Fn(GraphQlTabMessage) -> crate::message::Message + Copy + 'static,
    request_pane_height: f32,
    on_resize_start: crate::message::Message,
    multiline_height: impl Fn(MultilineFieldKind) -> f32 + Copy + 'a,
    on_multiline_resize_start: impl Fn(MultilineFieldKind) -> crate::message::Message + Copy + 'a,
) -> Element<'a, crate::message::Message> {
    use crate::message::Message;

    let is_sub = state.is_subscription();
    let (action_label, on_action): (&str, Option<Message>) = if is_sub {
        if state.subscribed {
            ("Subscribed", None)
        } else {
            ("Subscribe", Some(wrap(GraphQlTabMessage::Subscribe)))
        }
    } else if state.is_loading {
        ("Running…", None)
    } else {
        ("Run", Some(wrap(GraphQlTabMessage::Run)))
    };

    let cancel = if is_sub {
        state
            .subscribed
            .then_some(("Stop", wrap(GraphQlTabMessage::StopSubscription)))
    } else {
        state
            .is_loading
            .then_some(("Cancel", wrap(GraphQlTabMessage::Cancel)))
    };

    let bar = url_bar(
        &state.url,
        move |u| wrap(GraphQlTabMessage::UrlChanged(u)),
        action_label,
        on_action,
        cancel.as_ref().map(|(l, _)| *l),
        cancel.map(|(_, m)| m),
    );

    let schema_row = row![
        button(text(if state.schema_loading {
            "Loading schema…"
        } else {
            "Fetch Schema"
        }))
        .on_press_maybe((!state.schema_loading).then(|| wrap(GraphQlTabMessage::FetchSchema)))
        .padding([6, 12]),
        text_input("Operation name (optional)", &state.operation_name)
            .on_input(move |v| wrap(GraphQlTabMessage::OperationNameChanged(v)))
            .padding(8)
            .width(Length::Fixed(220.0)),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let query_editor = multiline_input(
        "query { ... }",
        &state.query,
        8,
        multiline_height(MultilineFieldKind::GraphQlQuery(tab_id)),
        move |action| wrap(GraphQlTabMessage::QueryAction(action)),
        Message::None,
        on_multiline_resize_start(MultilineFieldKind::GraphQlQuery(tab_id)),
    );

    let variables_editor = multiline_input(
        "{}",
        &state.variables,
        8,
        multiline_height(MultilineFieldKind::GraphQlVariables(tab_id)),
        move |action| wrap(GraphQlTabMessage::VariablesAction(action)),
        Message::None,
        on_multiline_resize_start(MultilineFieldKind::GraphQlVariables(tab_id)),
    );

    let editors_pane = column![
        text("Query").size(13),
        container(query_editor).style(container::bordered_box),
        text("Variables (JSON)").size(13),
        container(variables_editor).style(container::bordered_box),
    ]
    .spacing(6)
    .width(Length::FillPortion(3));

    let operations_pane = render_operations_pane(state, wrap).width(Length::FillPortion(2));

    let headers_pane = column![
        text("Headers").size(13),
        simple_kv_editor(
            &state.headers,
            move |i, kv| wrap(GraphQlTabMessage::HeaderChanged(i, kv)),
            wrap(GraphQlTabMessage::AddHeader),
            move |i| wrap(GraphQlTabMessage::RemoveHeader(i)),
        ),
    ]
    .spacing(6);

    let configuration_pane = column![
        bar,
        schema_row,
        row![operations_pane, editors_pane]
            .spacing(12)
            .height(Length::Fill),
        headers_pane,
    ]
    .spacing(12)
    .height(Length::Fill);

    let bottom: Element<'_, Message> = if is_sub {
        message_log(
            &state.subscription_log,
            crate::ui::tab::protocol_common::graphql_subscription_log_id(tab_id),
        )
    } else {
        let body = match &state.response {
            Some(Ok(json)) => json.clone(),
            Some(Err(e)) => e.clone(),
            None => String::new(),
        };
        scrollable(text(body).size(12))
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    };

    column![
        container(configuration_pane).height(Length::Fixed(request_pane_height)),
        resize_handle(DividerOrientation::Horizontal, on_resize_start),
        container(bottom)
            .height(Length::Fill)
            .width(Length::Fill)
            .padding(10)
            .style(container::bordered_box),
    ]
    .spacing(10)
    .height(Length::Fill)
    .into()
}

fn render_operations_pane<'a>(
    state: &'a GraphQlTabState,
    wrap: impl Fn(GraphQlTabMessage) -> crate::message::Message + Copy + 'static,
) -> iced::widget::Column<'a, crate::message::Message> {
    let mut kind_tabs = row![].spacing(6);
    for kind in OperationKind::ALL {
        let is_selected = state.selected_operation_kind == kind;
        kind_tabs = kind_tabs.push(
            button(text(kind.label()).size(11))
                .on_press(wrap(GraphQlTabMessage::OperationKindTabSelected(kind)))
                .padding(4)
                .style(if is_selected {
                    button::primary
                } else {
                    button::text
                }),
        );
    }

    let Some(schema) = &state.schema else {
        return column![
            text("Operations").size(13),
            kind_tabs,
            text("Fetch the schema to list operations.")
                .size(11)
                .color(iced::Color::from_rgb(0.6, 0.6, 0.6)),
        ]
        .spacing(6);
    };

    let kind = state.selected_operation_kind;
    let fields = schema.operations(kind);
    let mut content = column![].spacing(2);

    if fields.is_empty() {
        content = content.push(
            text("No operations of this kind.")
                .size(11)
                .color(iced::Color::from_rgb(0.6, 0.6, 0.6)),
        );
    }

    for field in fields {
        let name = field.name.clone();
        let is_selected_field = state.selected_field.as_ref() == Some(&(kind, name.clone()));
        content = content.push(
            button(text(name.clone()).size(12))
                .on_press(wrap(GraphQlTabMessage::OperationPicked(kind, name)))
                .padding(4)
                .style(if is_selected_field {
                    button::primary
                } else {
                    button::text
                })
                .width(Length::Fill),
        );

        if is_selected_field {
            content = content.push(
                container(render_tree(&state.selection_tree, Vec::new(), wrap)).padding(
                    iced::Padding {
                        top: 2.0,
                        right: 0.0,
                        bottom: 2.0,
                        left: 16.0,
                    },
                ),
            );
        }
    }

    column![
        text("Operations").size(13),
        kind_tabs,
        scrollable(content).height(Length::Fill),
    ]
    .spacing(6)
}

/// Recursively renders the checkbox/expand tree for one level of
/// `SelectionNode`s; `path` is the sequence of child indices from the root
/// down to (but not including) `nodes`, used to address a click back to the
/// exact node via `ToggleTreeNode`/`ExpandTreeNode`.
fn render_tree<'a>(
    nodes: &'a [SelectionNode],
    path: Vec<usize>,
    wrap: impl Fn(GraphQlTabMessage) -> crate::message::Message + Copy + 'static,
) -> iced::widget::Column<'a, crate::message::Message> {
    let mut col = column![].spacing(2);

    for (index, node) in nodes.iter().enumerate() {
        let mut node_path = path.clone();
        node_path.push(index);

        let mut node_row = row![].spacing(4).align_y(Alignment::Center);

        if node.is_object {
            let expand_path = node_path.clone();
            node_row = node_row.push(
                button(text(if node.expanded { "▾" } else { "▸" }).size(10))
                    .on_press(wrap(GraphQlTabMessage::ExpandTreeNode(expand_path)))
                    .padding(2)
                    .style(button::text),
            );
        } else {
            node_row = node_row.push(text(" ").size(10).width(Length::Fixed(18.0)));
        }

        let toggle_path = node_path.clone();
        node_row = node_row.push(
            checkbox(node.checked)
                .on_toggle(move |_| wrap(GraphQlTabMessage::ToggleTreeNode(toggle_path.clone()))),
        );
        node_row = node_row.push(text(node.field_name.clone()).size(12));

        col = col.push(node_row);

        if node.is_object && node.expanded {
            col = col.push(
                container(render_tree(&node.children, node_path, wrap)).padding(iced::Padding {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 16.0,
                }),
            );
        }
    }

    col
}
