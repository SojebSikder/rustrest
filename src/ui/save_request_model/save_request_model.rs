use crate::{app::Rustrest, message::Message};
use iced::widget::{button, column, container, pick_list, row, text, text_input};
use iced::{Border, Color, Element, Font, Length, Shadow, Theme, Vector};

pub fn view_save_request_modal(app: &Rustrest) -> Option<Element<Message>> {
    let modal = app.save_request_model.as_ref()?;

    let collection_options: Vec<String> = app
        .collections
        .iter()
        .map(|c| c.info.name.clone())
        .collect();

    let selected_label = modal
        .selected_collection_id
        .and_then(|id| app.collections.iter().find(|c| c.id == id))
        .map(|c| c.info.name.clone());

    let title = text("Save Request").size(18).font(Font {
        weight: iced::font::Weight::Bold,
        ..Font::DEFAULT
    });

    let name_label = text("Request name")
        .size(13)
        .color(Color::from_rgb(0.55, 0.55, 0.6));
    let name_input = text_input("e.g. Get user profile", &modal.request_name)
        .on_input(Message::SaveRequestNameChanged)
        .padding(10)
        .size(14)
        .width(Length::Fill);

    let collection_label = text("Save to collection")
        .size(13)
        .color(Color::from_rgb(0.55, 0.55, 0.6));
    let collection_picker = pick_list(collection_options, selected_label, |picked_name| {
        let col_id = app
            .collections
            .iter()
            .find(|c| c.info.name == picked_name)
            .map(|c| c.id)
            .unwrap_or_default();
        Message::SaveRequestModalCollectionSelected(col_id)
    })
    .placeholder("Choose a collection...")
    .padding(10)
    .width(Length::Fill);

    let no_collections_hint: Option<Element<Message>> = if app.collections.is_empty() {
        Some(
            text("You don't have any collections yet. Create one first.")
                .size(12)
                .color(Color::from_rgb(0.8, 0.4, 0.3))
                .into(),
        )
    } else {
        None
    };

    let cancel_btn = button(text("Cancel").size(14))
        .on_press(Message::CloseSaveRequestModal)
        .padding([8, 16])
        .style(button::secondary);

    let save_btn = button(text("Save").size(14))
        .on_press_maybe(
            modal
                .selected_collection_id
                .map(|_| Message::SaveRequestConfirmed),
        )
        .padding([8, 16])
        .style(button::primary);

    let footer = row![cancel_btn, save_btn].spacing(10).width(Length::Fill);

    let mut body = column![
        title,
        column![name_label, name_input].spacing(6),
        column![collection_label, collection_picker].spacing(6),
    ]
    .spacing(18);

    if let Some(hint) = no_collections_hint {
        body = body.push(hint);
    }

    body = body.push(
        container(footer)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right),
    );

    let card = container(body.spacing(18).padding(24))
        .width(Length::Fixed(380.0))
        .style(|_theme: &Theme| container::Style {
            background: Some(Color::from_rgb(0.13, 0.13, 0.15).into()),
            border: Border {
                color: Color::from_rgb(0.25, 0.25, 0.28),
                width: 1.0,
                radius: 10.0.into(),
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
                offset: Vector::new(0.0, 6.0),
                blur_radius: 24.0,
            },
            ..Default::default()
        });

    Some(card.into())
}
