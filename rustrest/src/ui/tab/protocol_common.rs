//! Widgets shared across the WebSocket/SSE/GraphQL/gRPC tabs: a connect/send
//! bar, a lightweight header-row editor, and a timestamped message/event log.

use super::types::KeyValuePair;
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

pub fn url_bar<'a, Message>(
    url: &str,
    on_url_change: impl Fn(String) -> Message + 'a,
    action_label: &'a str,
    on_action: Option<Message>,
    cancel_label: Option<&'a str>,
    on_cancel: Option<Message>,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let mut bar = row![
        text_input("Enter URL", url)
            .on_input(on_url_change)
            .padding(10)
            .width(Length::Fill),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let mut action_btn = button(text(action_label).size(13)).padding([8, 16]);
    if let Some(msg) = on_action {
        action_btn = action_btn.on_press(msg);
    }
    bar = bar.push(action_btn.style(button::success));

    if let (Some(label), Some(msg)) = (cancel_label, on_cancel) {
        bar = bar.push(
            button(text(label).size(13))
                .on_press(msg)
                .padding([8, 16])
                .style(button::danger),
        );
    }

    bar.into()
}

/// A single-line key/value row editor for headers/metadata
pub fn simple_kv_editor<'a, Message>(
    pairs: &'a [KeyValuePair],
    on_change: impl Fn(usize, KeyValuePair) -> Message + Copy + 'a,
    on_add: Message,
    on_remove: impl Fn(usize) -> Message + Copy + 'a,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let mut content = column![].spacing(5);

    for (idx, item) in pairs.iter().enumerate() {
        let item_clone = item.clone();
        let key_clone = item.key.clone();
        let val_clone = item.value.clone();

        let row_element = row![
            checkbox(item.is_active).on_toggle(move |checked| {
                on_change(
                    idx,
                    KeyValuePair {
                        is_active: checked,
                        key: key_clone.clone(),
                        value: val_clone.clone(),
                    },
                )
            }),
            text_input("Key", &item.key)
                .on_input(move |k| {
                    on_change(
                        idx,
                        KeyValuePair {
                            is_active: item_clone.is_active,
                            key: k,
                            value: item_clone.value.clone(),
                        },
                    )
                })
                .padding(8),
            text_input("Value", &item.value)
                .on_input(move |v| {
                    on_change(
                        idx,
                        KeyValuePair {
                            is_active: item.is_active,
                            key: item.key.clone(),
                            value: v,
                        },
                    )
                })
                .padding(8),
            button("Delete")
                .on_press(on_remove(idx))
                .padding(8)
                .style(button::danger)
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        content = content.push(row_element);
    }

    column![
        scrollable(content).height(Length::Fixed(120.0)),
        button("Add Row").on_press(on_add).padding(8)
    ]
    .spacing(10)
    .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogDirection {
    Out,
    In,
    Info,
    Error,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub direction: LogDirection,
    pub label: String,
    pub body: String,
}

impl LogEntry {
    pub fn out(body: impl Into<String>) -> Self {
        Self {
            direction: LogDirection::Out,
            label: "Sent".to_string(),
            body: body.into(),
        }
    }

    pub fn incoming(label: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            direction: LogDirection::In,
            label: label.into(),
            body: body.into(),
        }
    }

    pub fn info(body: impl Into<String>) -> Self {
        Self {
            direction: LogDirection::Info,
            label: "Info".to_string(),
            body: body.into(),
        }
    }

    pub fn error(body: impl Into<String>) -> Self {
        Self {
            direction: LogDirection::Error,
            label: "Error".to_string(),
            body: body.into(),
        }
    }
}

/// The maximum number of entries any [`message_log`] caller keeps around -
/// past this, oldest entries should be dropped as new ones arrive, so a
/// high-frequency stream can't grow the log, and the render cost of the widgets under it, without bound.
pub const MESSAGE_LOG_CAP: usize = 500;

/// Pushes `entry` onto `log`, dropping the oldest entry first if that would
/// put it over [`MESSAGE_LOG_CAP`].
pub fn push_capped(log: &mut Vec<LogEntry>, entry: LogEntry) {
    if log.len() >= MESSAGE_LOG_CAP {
        log.remove(0);
    }
    log.push(entry);
}

pub fn ws_log_id(tab_id: usize) -> iced::widget::Id {
    iced::widget::Id::from(format!("ws-log-{tab_id}"))
}

pub fn grpc_log_id(tab_id: usize) -> iced::widget::Id {
    iced::widget::Id::from(format!("grpc-log-{tab_id}"))
}

pub fn graphql_subscription_log_id(tab_id: usize) -> iced::widget::Id {
    iced::widget::Id::from(format!("graphql-subscription-log-{tab_id}"))
}

pub fn sse_log_id(tab_id: usize) -> iced::widget::Id {
    iced::widget::Id::from(format!("sse-log-{tab_id}"))
}

/// A scrollable, color-coded log of in/out messages - used by the WS
/// message log, the SSE event feed, the gRPC streaming response log, and
/// the GraphQL subscription feed. `id` should be a stable per-tab
/// `scrollable::Id` so the caller can snap it to the bottom whenever a new entry arrives.
pub fn message_log<'a, Message: 'a>(
    entries: &'a [LogEntry],
    id: iced::widget::Id,
) -> Element<'a, Message> {
    let mut content = column![].spacing(6);

    for entry in entries {
        let (prefix_color, label_text): (iced::Color, String) = match entry.direction {
            LogDirection::Out => (
                iced::Color::from_rgb(0.3, 0.6, 1.0),
                format!("→ {}", entry.label),
            ),
            LogDirection::In => (
                iced::Color::from_rgb(0.3, 0.8, 0.4),
                format!("← {}", entry.label),
            ),
            LogDirection::Info => (iced::Color::from_rgb(0.6, 0.6, 0.6), entry.label.clone()),
            LogDirection::Error => (iced::Color::from_rgb(0.9, 0.3, 0.3), entry.label.clone()),
        };

        content = content.push(
            container(
                column![
                    text(label_text).size(11).color(prefix_color),
                    text(entry.body.clone()).size(12),
                ]
                .spacing(2),
            )
            .padding(8)
            .width(Length::Fill)
            .style(container::bordered_box),
        );
    }

    scrollable(content).height(Length::Fill).id(id).into()
}

/// Filters to the active header/metadata pairs and resolves them to plain
/// `(key, value)` tuples - the shape every protocol client expects.
pub fn active_pairs(pairs: &[KeyValuePair]) -> Vec<(String, String)> {
    pairs
        .iter()
        .filter(|p| p.is_active && !p.key.trim().is_empty())
        .map(|p| (p.key.clone(), p.value.clone()))
        .collect()
}
