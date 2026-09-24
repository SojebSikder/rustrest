//! "Release Notes" tab (Help > View Release Notes): the GitHub release notes
//! for the running version, rendered in-app as markdown

use crate::APP_VERSION;
use crate::message::Message;
use crate::ui::modal::{danger_text_color, muted_text_color};
use crate::ui::spinner::spinner_with_label;
use iced::widget::{button, column, container, markdown, row, scrollable, text};
use iced::{Alignment, Element, Font, Length, Theme};

#[derive(Debug, Clone)]
pub enum ReleaseNotesState {
    Loading,
    Loaded {
        markdown: String,
        items: Vec<markdown::Item>,
    },
    Failed(String),
}

impl ReleaseNotesState {
    pub fn from_result(result: Result<String, String>) -> Self {
        match result {
            Ok(markdown) => Self::Loaded {
                items: markdown::parse(&markdown).collect(),
                markdown,
            },
            Err(err) => Self::Failed(err),
        }
    }
}

pub fn render_release_notes_tab<'a>(
    state: &'a ReleaseNotesState,
    theme: &Theme,
    spinner_tick: u64,
) -> Element<'a, Message> {
    let title = text(format!("Release Notes v{APP_VERSION}"))
        .size(18)
        .font(Font {
            weight: iced::font::Weight::Bold,
            ..Font::DEFAULT
        });

    let mut actions = row![].spacing(8);
    if let ReleaseNotesState::Loaded { markdown, .. } = state {
        actions = actions.push(
            button(text("Copy").size(12))
                .on_press(Message::CopyToClipboard(markdown.clone()))
                .padding([4, 10])
                .style(button::secondary),
        );
    }
    let refresh = button(text("Refresh").size(12))
        .padding([4, 10])
        .style(button::secondary);
    actions = actions.push(if matches!(state, ReleaseNotesState::Loading) {
        refresh
    } else {
        refresh.on_press(Message::ViewReleaseNotes)
    });

    let header = row![
        title,
        container(actions)
            .width(Length::Fill)
            .align_x(Alignment::End)
    ]
    .align_y(Alignment::Center);

    let body: Element<'a, Message> = match state {
        ReleaseNotesState::Loading => spinner_with_label(spinner_tick, "Fetching release notes..."),
        ReleaseNotesState::Failed(err) => text(format!("Could not load release notes: {err}"))
            .size(13)
            .style(|theme: &Theme| text::Style {
                color: Some(danger_text_color(theme)),
            })
            .into(),
        ReleaseNotesState::Loaded { items, .. } if items.is_empty() => {
            text("This release has no notes.")
                .size(13)
                .style(|theme: &Theme| text::Style {
                    color: Some(muted_text_color(theme)),
                })
                .into()
        }
        ReleaseNotesState::Loaded { items, .. } => scrollable(
            container(markdown::view(items, theme).map(Message::ReleaseNotesLinkClicked))
                .padding(12)
                .width(Length::Fill),
        )
        .height(Length::Fill)
        .into(),
    };

    column![header, body]
        .spacing(12)
        .padding(8)
        .height(Length::Fill)
        .into()
}
