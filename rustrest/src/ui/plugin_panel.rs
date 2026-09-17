//! Renders a plugin's declarative `UiNode` tree

use crate::message::Message;
use iced::widget::{Space, button, checkbox, column, row, scrollable, text, text_input};
use iced::{Element, Length};
use rustrest_plugin_host::{UiEvent, UiNode};

pub fn render_plugin_panel<'a>(
    plugin_id: &str,
    panel_id: &str,
    tree: Option<&'a UiNode>,
) -> Element<'a, Message> {
    match tree {
        Some(node) => scrollable(render_node(
            plugin_id,
            panel_id,
            node,
            Message::PluginPanelEvent,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .into(),
        None => text("(plugin panel unavailable)").into(),
    }
}

/// translates a `UiNode` tree into real widgets
pub(crate) fn render_node<'a>(
    plugin_id: &str,
    panel_id: &str,
    node: &'a UiNode,
    to_message: fn(String, String, UiEvent) -> Message,
) -> Element<'a, Message> {
    let plugin_id = plugin_id.to_string();
    let panel_id = panel_id.to_string();

    match node {
        UiNode::Label(label) => text(label.clone()).into(),

        UiNode::Muted(label) => text(label.clone()).size(11).style(text::secondary).into(),

        UiNode::Button { id, label, primary } => button(text(label.clone()))
            .padding([6, 12])
            .style(if *primary {
                button::primary
            } else {
                button::secondary
            })
            .on_press(to_message(
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
                .padding(8)
                .on_input(move |new_value| {
                    to_message(
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
                    to_message(
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
                r = r.push(render_node(&plugin_id, &panel_id, child, to_message));
            }
            r.into()
        }

        UiNode::Column(children) => {
            let mut c = column![].spacing(8);
            for child in children {
                c = c.push(render_node(&plugin_id, &panel_id, child, to_message));
            }
            c.into()
        }

        UiNode::Spacer => Space::new().width(Length::Fill).height(Length::Fill).into(),
    }
}
