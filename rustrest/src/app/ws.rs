//! WebSocket tab update handlers: connect/send/disconnect, and routing
//! inbound frames from the background session back into the owning tab's message log.

use super::{Rustrest, WorkspaceContent};
use crate::message::Message;
use crate::ui::tab::protocol_common::{LogEntry, push_capped, ws_log_id};
use crate::ui::tab::streaming::spawn_streaming;
use crate::ui::tab::ws::WsTabMessage;
use iced::Task;
use tokio::sync::mpsc;

fn scroll_to_bottom(tab_id: usize) -> Task<Message> {
    iced::widget::operation::snap_to_end(ws_log_id(tab_id))
}

fn active_ws_state(app: &mut Rustrest) -> Option<(usize, &mut crate::ui::tab::ws::WsTabState)> {
    let idx = app.active_tab_index;
    let tab_state = app.tabs.get_mut(idx)?;
    let tab_id = tab_state.tab.id;
    match &mut tab_state.content {
        WorkspaceContent::WebSocket(state) => Some((tab_id, state)),
        _ => None,
    }
}

fn is_ws_edit(msg: &WsTabMessage) -> bool {
    matches!(
        msg,
        WsTabMessage::UrlChanged(_)
            | WsTabMessage::HeaderChanged(_, _)
            | WsTabMessage::AddHeader
            | WsTabMessage::RemoveHeader(_)
    )
}

pub fn active_ws_message(app: &mut Rustrest, msg: WsTabMessage) -> Task<Message> {
    if is_ws_edit(&msg) {
        let idx = app.active_tab_index;
        if let Some(tab_state) = app.tabs.get_mut(idx) {
            tab_state.tab.dirty = true;
        }
    }

    let Some((tab_id, state)) = active_ws_state(app) else {
        return Task::none();
    };

    match msg {
        WsTabMessage::UrlChanged(url) => {
            state.url = url;
            Task::none()
        }
        WsTabMessage::HeaderChanged(idx, kv) => {
            if let Some(row) = state.headers.get_mut(idx) {
                *row = kv;
            }
            Task::none()
        }
        WsTabMessage::AddHeader => {
            state
                .headers
                .push(crate::ui::tab::types::KeyValuePair::new("", ""));
            Task::none()
        }
        WsTabMessage::RemoveHeader(idx) => {
            if idx < state.headers.len() {
                state.headers.remove(idx);
            }
            Task::none()
        }
        WsTabMessage::ComposeChanged(text) => {
            state.compose = text;
            Task::none()
        }
        WsTabMessage::Connect => {
            state.connecting = true;
            push_capped(
                &mut state.log,
                LogEntry::info(format!("Connecting to {}…", state.url)),
            );
            let url = state.url.clone();
            let headers = state.active_headers();
            let (out_tx, out_rx) = mpsc::unbounded_channel();
            state.outgoing_tx = Some(out_tx);

            Task::batch([
                scroll_to_bottom(tab_id),
                spawn_streaming(
                    move |events_tx| rustrest_ws::run_session(url, headers, out_rx, events_tx),
                    move |event| Message::WsEvent(tab_id, event),
                    move |_| Message::WsClosed(tab_id),
                ),
            ])
        }
        WsTabMessage::Disconnect => {
            if let Some(tx) = state.outgoing_tx.take() {
                let _ = tx.send(rustrest_ws::WsOutbound::Close);
            }
            state.connected = false;
            state.connecting = false;
            Task::none()
        }
        WsTabMessage::SendMessage => {
            if state.compose.is_empty() {
                return Task::none();
            }
            if let Some(tx) = &state.outgoing_tx {
                let _ = tx.send(rustrest_ws::WsOutbound::Text(state.compose.clone()));
                push_capped(&mut state.log, LogEntry::out(state.compose.clone()));
                state.compose.clear();
                return scroll_to_bottom(tab_id);
            }
            Task::none()
        }
    }
}

fn find_ws_state(app: &mut Rustrest, tab_id: usize) -> Option<&mut crate::ui::tab::ws::WsTabState> {
    let tab_state = app.tabs.iter_mut().find(|t| t.tab.id == tab_id)?;
    match &mut tab_state.content {
        WorkspaceContent::WebSocket(state) => Some(state),
        _ => None,
    }
}

pub fn ws_event(app: &mut Rustrest, tab_id: usize, event: rustrest_ws::WsEvent) -> Task<Message> {
    let Some(state) = find_ws_state(app, tab_id) else {
        return Task::none();
    };

    match event {
        rustrest_ws::WsEvent::Connected => {
            state.connecting = false;
            state.connected = true;
            push_capped(&mut state.log, LogEntry::info("Connected"));
        }
        rustrest_ws::WsEvent::Message(rustrest_ws::WsMessageKind::Text(text)) => {
            push_capped(&mut state.log, LogEntry::incoming("Text", text));
        }
        rustrest_ws::WsEvent::Message(rustrest_ws::WsMessageKind::Binary(bytes)) => {
            push_capped(
                &mut state.log,
                LogEntry::incoming("Binary", format!("{} bytes", bytes.len())),
            );
        }
        rustrest_ws::WsEvent::Error(e) => {
            push_capped(&mut state.log, LogEntry::error(e));
        }
        rustrest_ws::WsEvent::Closed(reason) => {
            state.connected = false;
            state.connecting = false;
            push_capped(
                &mut state.log,
                LogEntry::info(match reason {
                    Some(r) => format!("Closed: {r}"),
                    None => "Closed".to_string(),
                }),
            );
        }
    }
    scroll_to_bottom(tab_id)
}

pub fn ws_closed(app: &mut Rustrest, tab_id: usize) -> Task<Message> {
    if let Some(state) = find_ws_state(app, tab_id) {
        state.connected = false;
        state.connecting = false;
        state.outgoing_tx = None;
    }
    Task::none()
}
