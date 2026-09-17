//! Renders a plugin's declarative `UiNode` tree

use crate::message::Message;
use crate::ui::context_menu::with_context_menu;
use iced::widget::{Id, Space, button, checkbox, column, row, scrollable, text, text_input};
use iced::{Element, Length};
use rustrest_plugin_host::{UiEvent, UiNode};

/// id of a plugin panel's `UiNode::AutoScroll` region (there's at most one
/// per panel today), used to snap it to the bottom whenever the panel's
/// tree is replaced with one that still contains an `AutoScroll` node.
pub fn autoscroll_id(plugin_id: &str, panel_id: &str) -> Id {
    Id::from(format!("plugin-autoscroll-{plugin_id}-{panel_id}"))
}

/// true if `node` contains a `UiNode::AutoScroll` anywhere in its tree -
/// checked after a panel's tree is replaced to decide whether to snap
/// `autoscroll_id` to the bottom.
pub fn contains_autoscroll(node: &UiNode) -> bool {
    match node {
        UiNode::AutoScroll(_) => true,
        UiNode::Row(children) | UiNode::Column(children) => {
            children.iter().any(contains_autoscroll)
        }
        UiNode::Scrollable(inner) => contains_autoscroll(inner),
        _ => false,
    }
}

pub fn render_plugin_panel<'a>(
    plugin_id: &str,
    panel_id: &str,
    tree: Option<&'a UiNode>,
) -> Element<'a, Message> {
    match tree {
        Some(node) => render_root(plugin_id, panel_id, node, Message::PluginPanelEvent),
        None => text("(plugin panel unavailable)").into(),
    }
}

pub(crate) fn render_root<'a>(
    plugin_id: &str,
    panel_id: &str,
    node: &'a UiNode,
    to_message: fn(String, String, UiEvent) -> Message,
) -> Element<'a, Message> {
    if !contains_scrollable(node) {
        return scrollable(render_node(plugin_id, panel_id, node, to_message))
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }

    match node {
        UiNode::Column(children) => {
            let mut c = column![]
                .spacing(8)
                .width(Length::Fill)
                .height(Length::Fill);
            for child in children {
                c = c.push(render_node(plugin_id, panel_id, child, to_message));
            }
            c.into()
        }
        UiNode::Row(children) => {
            let mut r = row![].spacing(8).width(Length::Fill).height(Length::Fill);
            for child in children {
                r = r.push(render_node(plugin_id, panel_id, child, to_message));
            }
            r.into()
        }
        _ => render_node(plugin_id, panel_id, node, to_message),
    }
}

/// true if `node` contains a `UiNode::Scrollable`/`UiNode::AutoScroll`
/// anywhere in its tree.
fn contains_scrollable(node: &UiNode) -> bool {
    match node {
        UiNode::Scrollable(_) | UiNode::AutoScroll(_) => true,
        UiNode::Row(children) | UiNode::Column(children) => {
            children.iter().any(contains_scrollable)
        }
        _ => false,
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
        UiNode::Label(label) => with_context_menu(
            text(label.clone()),
            Message::ShowPluginTextContextMenu(label.clone()),
        ),

        UiNode::Muted(label) => with_context_menu(
            text(label.clone()).size(11).style(text::secondary),
            Message::ShowPluginTextContextMenu(label.clone()),
        ),

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
            on_submit,
        } => {
            let input_id = id.clone();
            let mut widget = text_input(placeholder, value).padding(8).on_input({
                let plugin_id = plugin_id.clone();
                let panel_id = panel_id.clone();
                move |new_value| {
                    to_message(
                        plugin_id.clone(),
                        panel_id.clone(),
                        UiEvent::Changed(input_id.clone(), new_value),
                    )
                }
            });
            if let Some(submit_id) = on_submit {
                widget = widget.on_submit(to_message(
                    plugin_id,
                    panel_id,
                    UiEvent::Clicked(submit_id.clone()),
                ));
            }
            widget.into()
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

        UiNode::HorizontalSpacer => Space::new().width(Length::Fill).into(),
        UiNode::VerticalSpacer => Space::new().height(Length::Fill).into(),
        UiNode::FixedSpace { width, height } => Space::new()
            .width(Length::Fixed(*width))
            .height(Length::Fixed(*height))
            .into(),

        UiNode::Scrollable(inner) => {
            scrollable(render_node(&plugin_id, &panel_id, inner, to_message))
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }

        UiNode::AutoScroll(inner) => scrollable(render_node(&plugin_id, &panel_id, inner, to_message))
            .width(Length::Fill)
            .height(Length::Fill)
            .id(autoscroll_id(&plugin_id, &panel_id))
            .into(),
    }
}
