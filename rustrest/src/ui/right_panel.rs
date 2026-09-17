//! Renders the generic right-hand panel

use super::plugin_panel::render_node;
use crate::message::Message;
use iced::Element;
use iced::widget::{scrollable, text};
use rustrest_plugin_host::UiNode;

pub fn render_right_panel<'a>(
    plugin_id: &str,
    panel_id: &str,
    tree: Option<&'a UiNode>,
) -> Element<'a, Message> {
    match tree {
        Some(node) => scrollable(render_node(
            plugin_id,
            panel_id,
            node,
            Message::RightPanelEvent,
        ))
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .into(),
        None => text("(right panel unavailable)").into(),
    }
}
