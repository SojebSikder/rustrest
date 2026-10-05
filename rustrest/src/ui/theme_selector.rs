//! Theme selector: a searchable list of every installed theme that previews the highlighted one live

use crate::app::Rustrest;
use crate::message::Message;
use crate::ui::modal::card;
use iced::widget::{Id, Space, button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Font, Length};
use rustrest_command_palette::{Command, PaletteState, filter};

/// stable widget id so opening the selector can focus its search box.
pub fn input_id() -> Id {
    Id::new("rustrest-theme-selector-input")
}

/// every installed theme as a palette entry (action = theme name).
fn entries(app: &Rustrest) -> Vec<Command<String>> {
    app.theme
        .registry
        .all()
        .iter()
        .map(|t| {
            let mut subtitle = t.source.label();
            if let Some(author) = &t.author {
                subtitle = format!("{subtitle} · {author}");
            }
            Command::new(t.name.clone(), t.name.clone(), t.name.clone()).with_subtitle(subtitle)
        })
        .collect()
}

pub fn matches_for(app: &Rustrest, state: &PaletteState) -> Vec<Command<String>> {
    let entries = entries(app);
    filter(&entries, &state.query)
        .into_iter()
        .cloned()
        .collect()
}

pub fn view<'a>(app: &'a Rustrest, state: &'a PaletteState) -> Element<'a, Message> {
    let matches = matches_for(app, state);
    let colors = crate::theme::colors();
    let selected_name = app.theme.selection.resolve(app.theme.system).0.to_string();

    let input = text_input("Select Theme...", &state.query)
        .id(input_id())
        .on_input(Message::ThemeSelectorQueryChanged)
        .on_submit(Message::ThemeSelectorConfirm)
        .padding(10)
        .size(15);

    let mut list = column![].spacing(2);
    if matches.is_empty() {
        list = list.push(
            container(text("No matching themes").size(12).color(colors.text_muted)).padding(8),
        );
    }
    for (idx, cmd) in matches.iter().enumerate() {
        let is_highlighted = idx == state.selected;
        let appearance = app
            .theme
            .registry
            .get(&cmd.action)
            .map(|t| {
                if t.appearance.is_dark() {
                    "Dark"
                } else {
                    "Light"
                }
            })
            .unwrap_or_default();
        let current = if cmd.action == selected_name {
            "  ✓"
        } else {
            ""
        };

        let content = row![
            column![
                text(format!("{}{current}", cmd.title)).size(13),
                text(cmd.subtitle.clone().unwrap_or_default())
                    .size(11)
                    .color(colors.text_muted),
            ]
            .spacing(2),
            Space::new().width(Length::Fill),
            text(appearance).size(11).color(colors.text_muted),
        ]
        .align_y(Alignment::Center);

        list = list.push(
            button(content)
                .on_press(Message::ThemeSelectorItemClicked(cmd.action.clone()))
                .width(Length::Fill)
                .padding(8)
                .style(move |theme: &iced::Theme, status| {
                    if is_highlighted {
                        button::Style {
                            background: Some(theme.extended_palette().primary.weak.color.into()),
                            text_color: theme.extended_palette().primary.weak.text,
                            border: iced::Border {
                                radius: 6.0.into(),
                                ..Default::default()
                            },
                            ..button::text(theme, status)
                        }
                    } else {
                        button::Style {
                            border: iced::Border {
                                radius: 6.0.into(),
                                ..Default::default()
                            },
                            ..button::text(theme, status)
                        }
                    }
                }),
        );
    }

    let title = text("Select Theme").size(14).font(Font {
        weight: iced::font::Weight::Bold,
        ..Font::DEFAULT
    });
    let hint = text("↑↓ to preview · Enter to keep · Esc to revert")
        .size(11)
        .color(colors.text_muted);

    let body = column![
        row![title, Space::new().width(Length::Fill), hint].align_y(Alignment::Center),
        input,
        scrollable(list).height(Length::Fixed(360.0)),
    ]
    .spacing(10)
    .padding(20)
    .align_x(Alignment::Start);

    card(body, 480.0)
}
