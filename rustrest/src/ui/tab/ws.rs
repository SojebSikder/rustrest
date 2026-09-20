use super::protocol_common::{
    LogEntry, active_pairs, message_log, simple_kv_editor, url_bar, ws_log_id,
};
use super::types::KeyValuePair;
use crate::ui::resize_handle::{DividerOrientation, resize_handle};
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Alignment, Element, Length};

#[derive(Debug, Clone)]
pub struct WsTabState {
    pub url: String,
    pub headers: Vec<KeyValuePair>,
    pub compose: String,
    pub log: Vec<LogEntry>,
    pub connected: bool,
    pub connecting: bool,
    pub outgoing_tx: Option<tokio::sync::mpsc::UnboundedSender<rustrest_ws::WsOutbound>>,
}

impl Default for WsTabState {
    fn default() -> Self {
        Self {
            url: "wss://echo.websocket.org".to_string(),
            headers: vec![KeyValuePair::new("", "")],
            compose: String::new(),
            log: Vec::new(),
            connected: false,
            connecting: false,
            outgoing_tx: None,
        }
    }
}

impl WsTabState {
    pub fn active_headers(&self) -> Vec<(String, String)> {
        active_pairs(&self.headers)
    }
}

#[derive(Debug, Clone)]
pub enum WsTabMessage {
    UrlChanged(String),
    HeaderChanged(usize, KeyValuePair),
    AddHeader,
    RemoveHeader(usize),
    ComposeChanged(String),
    Connect,
    Disconnect,
    SendMessage,
}

pub fn view(
    state: &WsTabState,
    tab_id: usize,
    wrap: impl Fn(WsTabMessage) -> crate::message::Message + Copy + 'static,
    request_pane_height: f32,
    on_resize_start: crate::message::Message,
) -> Element<'_, crate::message::Message> {
    use crate::message::Message;

    let (action_label, on_action): (&str, Option<Message>) = if state.connecting {
        ("Connecting…", None)
    } else if state.connected {
        ("Connected", None)
    } else {
        ("Connect", Some(wrap(WsTabMessage::Connect)))
    };

    let bar = url_bar(
        &state.url,
        move |u| wrap(WsTabMessage::UrlChanged(u)),
        action_label,
        on_action,
        state.connected.then_some("Disconnect"),
        state.connected.then_some(wrap(WsTabMessage::Disconnect)),
    );

    let headers_pane = column![
        text("Headers").size(13),
        simple_kv_editor(
            &state.headers,
            move |i, kv| wrap(WsTabMessage::HeaderChanged(i, kv)),
            wrap(WsTabMessage::AddHeader),
            move |i| wrap(WsTabMessage::RemoveHeader(i)),
        ),
    ]
    .spacing(6);

    let compose_row = row![
        text_input("Message to send", &state.compose)
            .on_input(move |v| wrap(WsTabMessage::ComposeChanged(v)))
            .on_submit(wrap(WsTabMessage::SendMessage))
            .padding(10)
            .width(Length::Fill),
        button("Send")
            .on_press_maybe(state.connected.then(|| wrap(WsTabMessage::SendMessage)))
            .padding([8, 16])
            .style(button::primary),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let configuration_pane = column![bar, headers_pane].spacing(12).height(Length::Fill);

    column![
        container(configuration_pane).height(Length::Fixed(request_pane_height)),
        resize_handle(DividerOrientation::Horizontal, on_resize_start),
        container(message_log(&state.log, ws_log_id(tab_id)))
            .height(Length::Fill)
            .width(Length::Fill)
            .padding(10)
            .style(container::bordered_box),
        compose_row,
    ]
    .spacing(10)
    .height(Length::Fill)
    .into()
}
