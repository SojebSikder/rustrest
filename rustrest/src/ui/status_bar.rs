//! Reusable bottom status bar

use crate::app::Rustrest;
use crate::message::Message;
use crate::ui::tokens::{RADIUS_CONTROL, SPACE_SM, SPACE_XS, TEXT_BODY};
use iced::widget::{Space, button, column, container, row, text};
use iced::{Alignment, Element, Length};

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

/// fixed height of the bar
const BAR_HEIGHT: f32 = 26.0;

/// a flat status bar entry: muted text at rest, a soft wash on hover, and
/// a persistent wash + full-strength text while `active`
fn item_style(active: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |theme: &iced::Theme, status| {
        let colors = crate::theme::colors();
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);

        button::Style {
            background: if active {
                Some(colors.element_selected.into())
            } else if hovered {
                Some(colors.element_hover.into())
            } else {
                None
            },
            text_color: if active || hovered {
                colors.text
            } else {
                colors.text_muted
            },
            border: iced::Border {
                radius: RADIUS_CONTROL.into(),
                ..Default::default()
            },
            ..button::text(theme, status)
        }
    }
}

/// a clickable status bar entry with the shared flat styling
fn item_button<'a>(
    content: impl Into<Element<'a, Message>>,
    active: bool,
    on_press: Message,
) -> iced::widget::Button<'a, Message> {
    button(content)
        .style(item_style(active))
        .padding([2, 6])
        .on_press(on_press)
}

/// a colored dot and label telling whether we're connected to Cloud.
/// opens the cloud modal (sign in, teams, conflicts) when clicked.
fn cloud_indicator(app: &Rustrest) -> Element<'_, Message> {
    use crate::app::cloud::CloudStatus;
    let colors = crate::theme::colors();
    let email = app.cloud.email.as_deref().unwrap_or_default();
    let server = app
        .cloud
        .client
        .as_ref()
        .map(|c| c.base_url())
        .unwrap_or_default();

    let (dot, label, hint) = match app.cloud.status() {
        CloudStatus::SignedOut => (
            colors.text_muted,
            "Cloud",
            "Not signed in to Cloud".to_string(),
        ),
        CloudStatus::Expired => (
            colors.error,
            "Cloud: signed out",
            "Your session expired, sign in again to keep syncing".to_string(),
        ),
        CloudStatus::Connecting => (
            colors.warning,
            "Cloud",
            format!("Connecting to {server} as {email}..."),
        ),
        CloudStatus::Online => (
            colors.success,
            "Cloud",
            format!("Connected to {server} as {email}"),
        ),
        CloudStatus::Offline => (
            colors.error,
            "Cloud: offline",
            format!("Can't reach {server}, changes sync once it's back"),
        ),
    };

    let entry = item_button(
        row![text("●").size(8).color(dot), text(label).size(TEXT_BODY)]
            .spacing(5)
            .align_y(Alignment::Center),
        false,
        Message::OpenCloudModal,
    );
    super::tooltip::with_tooltip(entry, hint, iced::widget::tooltip::Position::Top)
}

/// renders the full bottom status bar
pub fn render_status_bar(app: &Rustrest) -> Element<'_, Message> {
    let colors = crate::theme::colors();
    let mut left = row![].spacing(SPACE_XS).align_y(Alignment::Center);

    for item in &app.status_bar.items {
        let entry: Element<'_, Message> = if item.spinner {
            container(super::spinner::spinner_with_label(
                app.spinner_tick,
                item.label.as_str(),
            ))
            .padding([2, 6])
            .into()
        } else if let Some(msg) = item.on_press.clone() {
            item_button(text(item.label.as_str()).size(TEXT_BODY), false, msg).into()
        } else {
            container(
                text(item.label.as_str())
                    .size(TEXT_BODY)
                    .color(colors.text_muted),
            )
            .padding([2, 6])
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
            Some(command_id) => item_button(
                text(item.label).size(TEXT_BODY),
                false,
                Message::PluginCommand(plugin_id, command_id),
            )
            .into(),
            None => container(text(item.label).size(TEXT_BODY).color(colors.text_muted))
                .padding([2, 6])
                .into(),
        };
        left = left.push(entry);
    }

    let console_label = if app.layout.console_logs.is_empty() {
        "Console".to_string()
    } else {
        format!("Console ({})", app.layout.console_logs.len())
    };
    let console_toggle = item_button(
        row![
            text("≡").size(TEXT_BODY),
            text(console_label).size(TEXT_BODY)
        ]
        .spacing(4)
        .align_y(Alignment::Center),
        !app.layout.console_collapsed,
        Message::ToggleConsolePanel,
    );

    let terminal_button = item_button(
        row![text(">_").size(TEXT_BODY), text("Terminal").size(TEXT_BODY)]
            .spacing(4)
            .align_y(Alignment::Center),
        false,
        Message::NewTerminalTabPressed,
    );

    let right = row![cloud_indicator(app), console_toggle, terminal_button]
        .spacing(SPACE_XS)
        .align_y(Alignment::Center);

    let bar = container(
        row![left, Space::new().width(Length::Fill), right]
            .height(Length::Fill)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fixed(BAR_HEIGHT))
    .padding([0.0, SPACE_SM])
    .style(|_theme: &iced::Theme| container::Style {
        background: Some(crate::theme::colors().status_bar_background.into()),
        ..Default::default()
    });

    // iced borders are all-or-nothing, so the top-only hairline is a 1px
    // strip stacked above the bar.
    let hairline = container(Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(|_theme: &iced::Theme| container::Style {
            background: Some(crate::theme::colors().border.into()),
            ..Default::default()
        });

    column![hairline, bar].into()
}
