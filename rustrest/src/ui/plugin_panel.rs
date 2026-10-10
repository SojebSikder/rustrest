//! Renders a plugin's declarative `UiNode` tree

use crate::message::Message;
use crate::ui::context_menu::with_context_menu;
use iced::widget::{
    Id, Space, button, checkbox, column, container, markdown, row, scrollable, text, text_input,
};
use iced::{Element, Font, Length, Padding, Theme};
use rustrest_plugin_host::{ButtonVariant, TextTone, UiEvent, UiLength, UiNode, UiStyle};
use std::collections::HashMap;

#[derive(Default)]
pub struct MarkdownCache {
    items: HashMap<String, Vec<markdown::Item>>,
}

impl MarkdownCache {
    /// parses any markdown in `trees` not seen before and drops entries no
    /// longer present, so the cache only ever holds what's on screen.
    pub fn refresh<'a>(&mut self, trees: impl IntoIterator<Item = &'a UiNode>) {
        fn collect<'a>(node: &'a UiNode, out: &mut Vec<&'a str>) {
            match node {
                UiNode::Markdown(source) => out.push(source),
                UiNode::Row(children) | UiNode::Column(children) => {
                    children.iter().for_each(|c| collect(c, out))
                }
                UiNode::Scrollable(inner)
                | UiNode::AutoScroll(inner)
                | UiNode::Styled { child: inner, .. } => collect(inner, out),
                _ => {}
            }
        }

        let mut sources = Vec::new();
        for tree in trees {
            collect(tree, &mut sources);
        }

        let mut items = HashMap::with_capacity(sources.len());
        for source in sources {
            let parsed = self
                .items
                .remove(source)
                .unwrap_or_else(|| markdown::parse(source).collect());
            items.insert(source.to_string(), parsed);
        }
        self.items = items;
    }
}

/// what `render_node` needs beyond the tree itself.
#[derive(Clone, Copy)]
pub struct RenderEnv<'a> {
    markdown: &'a MarkdownCache,
    theme: &'a Theme,
    style: UiStyle,
}

impl<'a> RenderEnv<'a> {
    pub fn new(markdown: &'a MarkdownCache, theme: &'a Theme) -> Self {
        Self {
            markdown,
            theme,
            style: UiStyle::default(),
        }
    }

    /// env for a node wrapped in `UiNode::Styled { style, .. }`.
    fn with_style(self, style: UiStyle) -> Self {
        Self {
            style: self.style.overlay(style),
            ..self
        }
    }

    /// env for a child of a `Row`/`Column`
    fn child(self) -> Self {
        Self {
            style: self.style.inherited(),
            ..self
        }
    }
}

fn length(value: UiLength) -> Length {
    match value {
        UiLength::Fill => Length::Fill,
        UiLength::Shrink => Length::Shrink,
        UiLength::Fixed(px) => Length::Fixed(px),
    }
}

fn padding([vertical, horizontal]: [f32; 2]) -> Padding {
    Padding::from([vertical, horizontal])
}

fn styled_text<'a>(
    content: String,
    style: UiStyle,
    default_size: Option<f32>,
    default_tone: TextTone,
) -> iced::widget::Text<'a> {
    let mut widget = text(content);
    if let Some(size) = style.text_size.or(default_size) {
        widget = widget.size(size);
    }
    if style.bold == Some(true) {
        widget = widget.font(Font {
            weight: iced::font::Weight::Bold,
            ..Font::DEFAULT
        });
    }

    match style.tone.unwrap_or(default_tone) {
        TextTone::Default => widget,
        TextTone::Muted => widget.style(text::secondary),
        TextTone::Primary => widget.style(text::primary),
        TextTone::Success => widget.style(text::success),
        TextTone::Warning => widget.style(text::warning),
        TextTone::Danger => widget.style(text::danger),
    }
}

/// wraps `element` in a container when the style sets any layout field the
/// element can't take natively (labels, markdown, checkboxes, ...).
fn with_layout<'a>(element: Element<'a, Message>, style: UiStyle) -> Element<'a, Message> {
    if style.width.is_none() && style.height.is_none() && style.padding.is_none() {
        return element;
    }
    let mut wrapper = container(element);
    if let Some(width) = style.width {
        wrapper = wrapper.width(length(width));
    }
    if let Some(height) = style.height {
        wrapper = wrapper.height(length(height));
    }
    if let Some(pad) = style.padding {
        wrapper = wrapper.padding(padding(pad));
    }
    wrapper.into()
}

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
        UiNode::Scrollable(inner) | UiNode::Styled { child: inner, .. } => {
            contains_autoscroll(inner)
        }
        _ => false,
    }
}

pub fn render_plugin_panel<'a>(
    plugin_id: &str,
    panel_id: &str,
    tree: Option<&'a UiNode>,
    env: RenderEnv<'a>,
) -> Element<'a, Message> {
    match tree {
        Some(node) => render_root(plugin_id, panel_id, node, env, Message::PluginPanelEvent),
        None => text("(plugin panel unavailable)").into(),
    }
}

pub(crate) fn render_root<'a>(
    plugin_id: &str,
    panel_id: &str,
    node: &'a UiNode,
    env: RenderEnv<'a>,
    to_message: fn(String, String, UiEvent) -> Message,
) -> Element<'a, Message> {
    if !contains_scrollable(node) {
        return scrollable(render_node(plugin_id, panel_id, node, env, to_message))
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }

    let spacing = env.style.spacing.unwrap_or(8.0);
    let pad = env.style.padding.map(padding).unwrap_or(Padding::ZERO);
    match node {
        // a styled root still gets the pinned full-height layout below
        UiNode::Styled { style, child } => render_root(
            plugin_id,
            panel_id,
            child,
            env.with_style(*style),
            to_message,
        ),
        UiNode::Column(children) => {
            let mut c = column![]
                .spacing(spacing)
                .padding(pad)
                .width(Length::Fill)
                .height(Length::Fill);
            for child in children {
                c = c.push(render_node(
                    plugin_id,
                    panel_id,
                    child,
                    env.child(),
                    to_message,
                ));
            }
            c.into()
        }
        UiNode::Row(children) => {
            let mut r = row![]
                .spacing(spacing)
                .padding(pad)
                .width(Length::Fill)
                .height(Length::Fill);
            for child in children {
                r = r.push(render_node(
                    plugin_id,
                    panel_id,
                    child,
                    env.child(),
                    to_message,
                ));
            }
            r.into()
        }
        _ => render_node(plugin_id, panel_id, node, env, to_message),
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
        UiNode::Styled { child, .. } => contains_scrollable(child),
        _ => false,
    }
}

/// translates a `UiNode` tree into real widgets
pub(crate) fn render_node<'a>(
    plugin_id: &str,
    panel_id: &str,
    node: &'a UiNode,
    env: RenderEnv<'a>,
    to_message: fn(String, String, UiEvent) -> Message,
) -> Element<'a, Message> {
    let plugin_id = plugin_id.to_string();
    let panel_id = panel_id.to_string();
    let style = env.style;
    let disabled = style.disabled == Some(true);

    match node {
        UiNode::Styled { style, child } => render_node(
            &plugin_id,
            &panel_id,
            child,
            env.with_style(*style),
            to_message,
        ),

        UiNode::Label(label) => with_layout(
            with_context_menu(
                styled_text(label.clone(), style, None, TextTone::Default),
                Message::ShowPluginTextContextMenu(label.clone()),
            ),
            style,
        ),

        UiNode::Muted(label) => with_layout(
            with_context_menu(
                styled_text(label.clone(), style, Some(11.0), TextTone::Muted),
                Message::ShowPluginTextContextMenu(label.clone()),
            ),
            style,
        ),

        UiNode::Markdown(source) => {
            let body: Element<'a, Message> = match env.markdown.items.get(source) {
                Some(items) => {
                    let settings = match style.text_size {
                        Some(size) => markdown::Settings::with_text_size(size, env.theme),
                        None => markdown::Settings::from(env.theme),
                    };
                    markdown::view(items, settings).map(Message::PluginLinkClicked)
                }
                // not cached yet (shouldn't happen) - fall back to plain text
                None => styled_text(source.clone(), style, None, TextTone::Default).into(),
            };
            with_layout(
                with_context_menu(body, Message::ShowPluginTextContextMenu(source.clone())),
                style,
            )
        }

        UiNode::Button { id, label, primary } => {
            let variant = style.variant.unwrap_or(if *primary {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            });
            // default tone lets the button's own style pick the text color
            let mut widget = button(styled_text(label.clone(), style, None, TextTone::Default))
                .padding(style.padding.map(padding).unwrap_or(Padding::from([6, 12])))
                .style(match variant {
                    ButtonVariant::Primary => button::primary,
                    ButtonVariant::Secondary => button::secondary,
                    ButtonVariant::Success => button::success,
                    ButtonVariant::Warning => button::warning,
                    ButtonVariant::Danger => button::danger,
                    ButtonVariant::Text => button::text,
                });
            if let Some(width) = style.width {
                widget = widget.width(length(width));
            }
            if let Some(height) = style.height {
                widget = widget.height(length(height));
            }
            if !disabled {
                widget = widget.on_press(to_message(
                    plugin_id,
                    panel_id,
                    UiEvent::Clicked(id.clone()),
                ));
            }
            widget.into()
        }

        UiNode::TextInput {
            id,
            value,
            placeholder,
            on_submit,
        } => {
            let input_id = id.clone();
            let mut widget = text_input(placeholder, value)
                .padding(style.padding.map(padding).unwrap_or(Padding::new(8.0)))
                .secure(style.secure == Some(true));
            if let Some(size) = style.text_size {
                widget = widget.size(size);
            }
            if let Some(width) = style.width {
                widget = widget.width(length(width));
            }
            if !disabled {
                widget = widget.on_input({
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
            }
            widget.into()
        }

        UiNode::Checkbox { id, label, checked } => {
            let id = id.clone();
            let mut widget = checkbox(*checked).label(label.clone());
            if let Some(size) = style.text_size {
                widget = widget.text_size(size);
            }
            if !disabled {
                widget = widget.on_toggle(move |new_value| {
                    to_message(
                        plugin_id.clone(),
                        panel_id.clone(),
                        UiEvent::Toggled(id.clone(), new_value),
                    )
                });
            }
            with_layout(widget.into(), style)
        }

        UiNode::List(items) => {
            let mut col = column![].spacing(style.spacing.unwrap_or(4.0));
            for item in items {
                col = col.push(styled_text(item.clone(), style, None, TextTone::Default));
            }
            with_layout(col.into(), style)
        }

        UiNode::Row(children) => {
            let mut r = row![].spacing(style.spacing.unwrap_or(8.0));
            if let Some(pad) = style.padding {
                r = r.padding(padding(pad));
            }
            if let Some(width) = style.width {
                r = r.width(length(width));
            }
            if let Some(height) = style.height {
                r = r.height(length(height));
            }
            for child in children {
                r = r.push(render_node(
                    &plugin_id,
                    &panel_id,
                    child,
                    env.child(),
                    to_message,
                ));
            }
            r.into()
        }

        UiNode::Column(children) => {
            let mut c = column![].spacing(style.spacing.unwrap_or(8.0));
            if let Some(pad) = style.padding {
                c = c.padding(padding(pad));
            }
            if let Some(width) = style.width {
                c = c.width(length(width));
            }
            if let Some(height) = style.height {
                c = c.height(length(height));
            }
            for child in children {
                c = c.push(render_node(
                    &plugin_id,
                    &panel_id,
                    child,
                    env.child(),
                    to_message,
                ));
            }
            c.into()
        }

        UiNode::HorizontalSpacer => Space::new().width(Length::Fill).into(),
        UiNode::VerticalSpacer => Space::new().height(Length::Fill).into(),
        UiNode::FixedSpace { width, height } => Space::new()
            .width(Length::Fixed(*width))
            .height(Length::Fixed(*height))
            .into(),

        UiNode::Scrollable(inner) => scrollable(render_node(
            &plugin_id,
            &panel_id,
            inner,
            env.child(),
            to_message,
        ))
        .width(style.width.map(length).unwrap_or(Length::Fill))
        .height(style.height.map(length).unwrap_or(Length::Fill))
        .into(),

        UiNode::AutoScroll(inner) => scrollable(render_node(
            &plugin_id,
            &panel_id,
            inner,
            env.child(),
            to_message,
        ))
        .width(style.width.map(length).unwrap_or(Length::Fill))
        .height(style.height.map(length).unwrap_or(Length::Fill))
        .id(autoscroll_id(&plugin_id, &panel_id))
        .into(),
    }
}
