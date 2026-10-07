//! Rustrest Cloud notification center

use crate::app::Rustrest;
use crate::message::Message;
use crate::ui::modal::{card, danger_text_color, muted_text_color};
use iced::widget::text::Wrapping;
use iced::widget::{Column, Space, button, column, container, row, scrollable, stack, svg, text};
use iced::{Alignment, Border, Color, Element, Font, Length, Padding, Theme};
use rustrest_cloud::wire::Notification;

const PANEL_WIDTH: f32 = 380.0;
const LIST_MAX_HEIGHT: f32 = 420.0;

/// Lucide's bell, tinted with the theme's text color when drawn
static BELL_ICON: std::sync::LazyLock<svg::Handle> = std::sync::LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("../../../assets/icons/bell.svg").as_slice())
});

fn semibold() -> Font {
    Font {
        weight: iced::font::Weight::Semibold,
        ..Font::DEFAULT
    }
}

/// "just now", "5m ago", "3h ago", "2d ago", then the date
fn ago(created_at: &str) -> String {
    let Ok(at) = chrono::DateTime::parse_from_rfc3339(created_at) else {
        return String::new();
    };

    let secs = (chrono::Utc::now() - at.with_timezone(&chrono::Utc)).num_seconds();
    match secs {
        ..60 => "just now".to_string(),
        60..3_600 => format!("{}m ago", secs / 60),
        3_600..86_400 => format!("{}h ago", secs / 3_600),
        86_400..604_800 => format!("{}d ago", secs / 86_400),
        _ => at.format("%b %-d, %Y").to_string(),
    }
}

/// the bell shown in the titlebar while signed in, with the unread count
pub fn bell(app: &Rustrest) -> Option<Element<'_, Message>> {
    app.cloud.client.as_ref()?;
    let center = &app.cloud.notifications;
    let colors = crate::theme::colors();
    let (badge_bg, text_color, muted, selected) = (
        colors.error,
        colors.text,
        colors.text_muted,
        colors.element_selected,
    );

    let (open, unread) = (center.open, center.unread > 0);
    let bell = svg(BELL_ICON.clone())
        .width(16)
        .height(16)
        .style(move |_: &Theme, status| svg::Style {
            color: Some(match status {
                svg::Status::Hovered => text_color,
                _ if open || unread => text_color,
                _ => muted,
            }),
        });
    let icon = container(bell)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

    let content: Element<'_, Message> = if center.unread > 0 {
        let count = if center.unread > 99 {
            "99+".to_string()
        } else {
            center.unread.to_string()
        };
        let dot = container(text(count).size(9).font(semibold()).color(Color::WHITE))
            .padding([0, 4])
            .style(move |_: &Theme| container::Style {
                background: Some(badge_bg.into()),
                border: Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            });

        stack![
            icon,
            container(dot)
                .width(Length::Fill)
                .align_x(Alignment::End)
                .padding(Padding {
                    top: 4.0,
                    right: 4.0,
                    ..Padding::ZERO
                }),
        ]
        .into()
    } else {
        icon.into()
    };

    Some(
        button(content)
            .padding(0)
            .width(Length::Fixed(40.0))
            .height(Length::Fill)
            .style(move |_: &Theme, status| {
                let hover = Color {
                    a: 0.12,
                    ..text_color
                };
                let background = match status {
                    _ if open => Some(selected.into()),
                    button::Status::Hovered | button::Status::Pressed => Some(hover.into()),
                    _ => None,
                };
                button::Style {
                    background,
                    text_color,
                    ..Default::default()
                }
            })
            .on_press(Message::ToggleNotificationCenter)
            .into(),
    )
}

fn row_view(notification: &Notification) -> Element<'_, Message> {
    let colors = crate::theme::colors();
    let (accent, hover, text_color) = (colors.text_accent, colors.element_hover, colors.text);
    let unread = !notification.is_read();

    let mut title_row = row![].spacing(6).align_y(Alignment::Center);
    if unread {
        title_row = title_row.push(container(Space::new().width(7).height(7)).style(
            move |_: &Theme| container::Style {
                background: Some(accent.into()),
                border: Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        ));
    }
    title_row = title_row
        .push(
            text(notification.title.as_str())
                .size(13)
                .font(if unread { semibold() } else { Font::DEFAULT })
                .wrapping(Wrapping::WordOrGlyph)
                .width(Length::Fill),
        )
        .push(
            text(ago(&notification.created_at))
                .size(11)
                .style(|theme: &Theme| text::Style {
                    color: Some(muted_text_color(theme)),
                }),
        );

    let mut body = column![title_row].spacing(3);
    if !notification.body.is_empty() {
        body = body.push(
            text(notification.body.as_str())
                .size(12)
                .wrapping(Wrapping::WordOrGlyph)
                .style(|theme: &Theme| text::Style {
                    color: Some(muted_text_color(theme)),
                }),
        );
    }

    let open = button(body)
        .width(Length::Fill)
        .padding([8, 10])
        .style(move |_: &Theme, status| {
            let background = match status {
                button::Status::Hovered | button::Status::Pressed => Some(hover),
                _ if unread => Some(Color { a: 0.06, ..accent }),
                _ => None,
            };
            button::Style {
                background: background.map(Into::into),
                text_color,
                border: Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(Message::CloudNotificationPressed(notification.id.clone()));

    let dismiss = button(text("✕").size(11))
        .padding([4, 6])
        .style(button::text)
        .on_press(Message::CloudDismissNotification(notification.id.clone()));

    row![open, dismiss]
        .spacing(2)
        .align_y(Alignment::Start)
        .into()
}

/// the dropdown panel, anchored under the bell
pub fn view_panel(app: &Rustrest) -> Element<'_, Message> {
    let center = &app.cloud.notifications;

    let mut header = row![
        text("Notifications").size(15).font(semibold()),
        Space::new().width(Length::Fill),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    if center.unread > 0 {
        header = header.push(
            button(text("Mark all read").size(12))
                .padding([4, 8])
                .style(button::text)
                .on_press(Message::CloudMarkAllNotificationsRead),
        );
    }
    if !center.items.is_empty() {
        header = header.push(
            button(text("Clear").size(12))
                .padding([4, 8])
                .style(button::text)
                .on_press(Message::CloudClearNotifications),
        );
    }

    let placeholder = |label: String, danger: bool| -> Element<'_, Message> {
        container(
            text(label)
                .size(12)
                .style(move |theme: &Theme| text::Style {
                    color: Some(if danger {
                        danger_text_color(theme)
                    } else {
                        muted_text_color(theme)
                    }),
                }),
        )
        .width(Length::Fill)
        .padding([24, 0])
        .align_x(Alignment::Center)
        .into()
    };

    let content: Element<'_, Message> = if center.items.is_empty() {
        match (&center.error, center.loading) {
            (Some(err), _) => placeholder(format!("Couldn't load notifications: {err}"), true),
            (None, true) => placeholder("Loading…".to_string(), false),
            (None, false) => placeholder("You're all caught up".to_string(), false),
        }
    } else {
        let list = Column::with_children(center.items.iter().map(row_view)).spacing(2);
        scrollable(container(list).padding(Padding {
            right: 10.0,
            ..Padding::ZERO
        }))
        .height(Length::Shrink)
        .into()
    };

    let body = column![header, container(content).max_height(LIST_MAX_HEIGHT),]
        .spacing(8)
        .padding(12);

    container(card(body, PANEL_WIDTH))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::End)
        .padding(Padding {
            top: super::titlebar::TITLEBAR_HEIGHT + 4.0,
            right: 8.0,
            ..Padding::ZERO
        })
        .into()
}
