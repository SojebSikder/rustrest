//! Renders the generic right-hand panel

use super::plugin_panel::{RenderEnv, render_root};
use crate::message::Message;
use iced::Element;
use iced::widget::text;
use rustrest_plugin_host::UiNode;

pub fn render_right_panel<'a>(
    plugin_id: &str,
    panel_id: &str,
    tree: Option<&'a UiNode>,
    env: RenderEnv<'a>,
) -> Element<'a, Message> {
    match tree {
        Some(node) => render_root(plugin_id, panel_id, node, env, Message::RightPanelEvent),
        None => text("(right panel unavailable)").into(),
    }
}
