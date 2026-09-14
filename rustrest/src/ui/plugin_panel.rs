//! Renders a plugin's declarative `UiNode` tree (see `rustrest-plugin-api`)
//! as real `iced` widgets, and maps interaction back into
//! `Message::PluginPanelEvent`. Plugins run sandboxed in wasm and have no
//! access to `iced::Element`, so this translation layer is the only way a
//! plugin's sidebar panel gets rendered.

use crate::message::Message;
use iced::Element;
use iced::widget::{button, checkbox, column, row, scrollable, text, text_input};
use rustrest_plugin_host::{UiEvent, UiNode};

pub fn render_plugin_panel<'a>(
    plugin_id: &str,
    panel_id: &str,
    tree: Option<&'a UiNode>,
) -> Element<'a, Message> {
    match tree {
        Some(node) => scrollable(render_node(plugin_id, panel_id, node))
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .into(),
        None => text("(plugin panel unavailable)").into(),
    }
}

fn render_node<'a>(plugin_id: &str, panel_id: &str, node: &'a UiNode) -> Element<'a, Message> {
    let plugin_id = plugin_id.to_string();
    let panel_id = panel_id.to_string();

    match node {
        UiNode::Label(label) => text(label.clone()).into(),

        UiNode::Button { id, label } => button(text(label.clone()))
            .on_press(Message::PluginPanelEvent(
                plugin_id,
                panel_id,
                UiEvent::Clicked(id.clone()),
            ))
            .into(),

        UiNode::TextInput {
            id,
            value,
            placeholder,
        } => {
            let id = id.clone();
            text_input(placeholder, value)
                .on_input(move |new_value| {
                    Message::PluginPanelEvent(
                        plugin_id.clone(),
                        panel_id.clone(),
                        UiEvent::Changed(id.clone(), new_value),
                    )
                })
                .into()
        }

        UiNode::Checkbox { id, label, checked } => {
            let id = id.clone();
            checkbox(*checked)
                .label(label.clone())
                .on_toggle(move |new_value| {
                    Message::PluginPanelEvent(
                        plugin_id.clone(),
                        panel_id.clone(),
                        UiEvent::Toggled(id.clone(), new_value),
                    )
                })
                .into()
        }

        UiNode::List(items) => {
            let mut col = column![].spacing(4);
            for item in items {
                col = col.push(text(item.clone()));
            }
            col.into()
        }

        UiNode::Row(children) => {
            let mut r = row![].spacing(8);
            for child in children {
                r = r.push(render_node(&plugin_id, &panel_id, child));
            }
            r.into()
        }

        UiNode::Column(children) => {
            let mut c = column![].spacing(8);
            for child in children {
                c = c.push(render_node(&plugin_id, &panel_id, child));
            }
            c.into()
        }
    }
}
