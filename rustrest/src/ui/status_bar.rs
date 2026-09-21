//! Reusable bottom status bar (Zed-style): a strip of small entries - a
//! spinner+label while something loads in the background, plain text, or a
//! clickable label acting as a lightweight prompt/action. Any part of the
//! app can push an entry via `StatusBarState::set` (keyed by a stable id,
//! so repeated calls update it in place) and remove it via `clear`.

use crate::app::Rustrest;
use crate::message::Message;
use iced::widget::{Space, button, container, row, text};
use iced::{Alignment, Element, Length, Padding};

struct StatusItem {
    id: String,
    label: String,
    spinner: bool,
    on_press: Option<Message>,
}

#[derive(Default)]
pub struct StatusBarState {
    items: Vec<StatusItem>,
}

impl StatusBarState {
    /// inserts or replaces the item with this id, keeping its position if
    /// it already existed - lets a caller update its own entry in place
    /// (e.g. a changing progress label) instead of stacking duplicates.
    pub fn set(&mut self, id: impl Into<String>, label: impl Into<String>, spinner: bool) {
        self.set_with_action(id, label, spinner, None);
    }

    /// same as `set`, but the entry is a clickable prompt/action rather
    /// than (or in addition to) a spinner.
    pub fn set_with_action(
        &mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        spinner: bool,
        on_press: Option<Message>,
    ) {
        let id = id.into();
        let item = StatusItem {
            id: id.clone(),
            label: label.into(),
            spinner,
            on_press,
        };
        match self.items.iter_mut().find(|i| i.id == id) {
            Some(existing) => *existing = item,
            None => self.items.push(item),
        }
    }

    pub fn clear(&mut self, id: &str) {
        self.items.retain(|i| i.id != id);
    }

    /// whether an entry with this id is currently shown - lets other UI
    /// (the sidebar, the plugin manager) key their own "still loading"
    /// state off the same status the bar itself is showing, instead of
    /// tracking it twice.
    pub fn is_active(&self, id: &str) -> bool {
        self.items.iter().any(|i| i.id == id)
    }

    /// whether any current entry wants the spinner animation ticking.
    pub fn has_active_spinner(&self) -> bool {
        self.items.iter().any(|i| i.spinner)
    }
}

/// renders the full bottom status bar: dynamic status entries (loading
/// spinners, prompts, etc. - see `StatusBarState`) on the left, and
/// fixed panel-toggle buttons (Console, Terminal) on the right, Zed-style.
pub fn render_status_bar(app: &Rustrest) -> Element<'_, Message> {
    let mut left = row![].spacing(16).align_y(Alignment::Center);

    for item in &app.status_bar.items {
        let entry: Element<'_, Message> = if item.spinner {
            super::spinner::spinner_with_label(app.spinner_tick, item.label.as_str())
        } else if let Some(msg) = item.on_press.clone() {
            button(text(item.label.as_str()).size(13))
                .style(button::text)
                .padding(0)
                .on_press(msg)
                .into()
        } else {
            text(item.label.as_str())
                .size(13)
                .color(iced::Color::from_rgb(0.5, 0.5, 0.5))
                .into()
        };
        left = left.push(entry);
    }

    // plugin-contributed entries (`Capability::StatusBarItem`): a plain
    // label, or a button dispatching the plugin's `on_command` hook when
    // it declares a `command_id` - the same dispatch path menu items and
    // command-palette entries already use.
    for (plugin_id, item) in app.plugins.plugin_manager.status_bar_items() {
        let entry: Element<'_, Message> = match item.command_id {
            Some(command_id) => button(text(item.label).size(13))
                .style(button::text)
                .padding(0)
                .on_press(Message::PluginCommand(plugin_id, command_id))
                .into(),
            None => text(item.label)
                .size(13)
                .color(iced::Color::from_rgb(0.5, 0.5, 0.5))
                .into(),
        };
        left = left.push(entry);
    }

    let console_collapsed = app.layout.console_collapsed;
    let console_label = if app.layout.console_logs.is_empty() {
        "Console".to_string()
    } else {
        format!("Console ({})", app.layout.console_logs.len())
    };
    let console_toggle = button(text(console_label).size(13))
        .style(if !console_collapsed {
            button::primary
        } else {
            button::secondary
        })
        .padding([4, 8])
        .on_press(Message::ToggleConsolePanel);

    let terminal_button = button(text("Terminal").size(13))
        .style(button::secondary)
        .padding([4, 8])
        .on_press(Message::NewTerminalTabPressed);

    let right = row![console_toggle, terminal_button]
        .spacing(6)
        .align_y(Alignment::Center);

    container(row![left, Space::new().width(Length::Fill), right].align_y(Alignment::Center))
        .width(Length::Fill)
        .padding(Padding {
            top: 6.0,
            bottom: 6.0,
            left: 4.0,
            right: 4.0,
        })
        .style(|theme: &iced::Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(palette.background.weak.color.into()),
                border: iced::Border {
                    color: palette.background.strong.color,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            }
        })
        .into()
}
