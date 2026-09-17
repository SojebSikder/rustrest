//! "Export via Plugin" modal: shown when a collection is exported and more
//! than one installed plugin/format is available, so the user can pick
//! which one to use (a single available option is used directly, without
//! this modal - see `Message::ExportViaPluginPressed`).

use crate::app::Rustrest;
use crate::message::Message;
use crate::ui::modal::card;
use iced::widget::{button, column, container, text};
use iced::{Font, Length};

pub fn view_export_plugin_picker(app: &Rustrest) -> Option<iced::Element<'_, Message>> {
    let (col_id, formats) = app.plugins.export_plugin_picker.as_ref()?;

    let title = text("Export via Plugin").size(18).font(Font {
        weight: iced::font::Weight::Bold,
        ..Font::DEFAULT
    });

    let mut list = column![].spacing(8);
    for (plugin_id, format) in formats {
        list = list.push(
            button(text(format.title.clone()).size(14))
                .on_press(Message::ExportCollectionViaPluginPressed(
                    *col_id,
                    plugin_id.clone(),
                    format.id.clone(),
                    format.extensions.clone(),
                ))
                .padding([8, 12])
                .width(Length::Fill)
                .style(button::secondary),
        );
    }

    let close_btn = button(text("Cancel").size(14))
        .on_press(Message::CloseExportPluginPicker)
        .padding([8, 16])
        .style(button::secondary);

    let body = column![
        title,
        list,
        container(close_btn)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right),
    ]
    .spacing(18)
    .padding(24);

    Some(card(body, 340.0))
}
