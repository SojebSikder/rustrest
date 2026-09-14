//! "Manage Plugins" modal: lists every discovered plugin, lets the user
//! enable/disable it, and surfaces load errors so a broken plugin doesn't
//! just silently vanish.

use crate::app::Rustrest;
use crate::message::Message;
use crate::ui::modal::card;
use iced::widget::{button, checkbox, column, container, row, scrollable, text};
use iced::{Alignment, Color, Element, Font, Length};

pub fn view_plugin_manager(app: &Rustrest) -> Element<'_, Message> {
    let title = text("Manage Plugins").size(18).font(Font {
        weight: iced::font::Weight::Bold,
        ..Font::DEFAULT
    });

    let installed = app.plugin_manager.installed();
    let mut list = column![].spacing(10);

    if installed.is_empty() {
        list = list.push(
            text(format!(
                "No plugins found in {}",
                app.plugin_manager.plugins_dir().display()
            ))
            .size(12)
            .color(Color::from_rgb(0.6, 0.6, 0.65)),
        );
    }

    for plugin in installed {
        let id = plugin.id().to_string();
        let row_content: Element<'_, Message> = match &plugin.manifest {
            Some(manifest) => {
                let header = row![
                    checkbox(plugin.enabled)
                        .label(manifest.name.clone())
                        .on_toggle(move |enabled| {
                            Message::TogglePluginEnabled(id.clone(), enabled)
                        }),
                    text(format!("v{}", manifest.version))
                        .size(11)
                        .color(Color::from_rgb(0.6, 0.6, 0.65)),
                    text(format!("by {}", manifest.author))
                        .size(11)
                        .color(Color::from_rgb(0.6, 0.6, 0.65)),
                ]
                .spacing(8)
                .align_y(Alignment::Center);

                column![
                    header,
                    text(manifest.description.clone())
                        .size(11)
                        .color(Color::from_rgb(0.6, 0.6, 0.65)),
                ]
                .spacing(4)
                .into()
            }
            None => column![
                text(format!("{} (failed to load)", plugin.dir_name))
                    .size(13)
                    .color(Color::from_rgb(0.85, 0.35, 0.35)),
                text(plugin.load_error.clone().unwrap_or_default())
                    .size(11)
                    .color(Color::from_rgb(0.85, 0.35, 0.35)),
            ]
            .spacing(4)
            .into(),
        };

        list = list.push(
            container(row_content)
                .padding(10)
                .width(Length::Fill)
                .style(container::bordered_box),
        );
    }

    let close_btn = button(text("Close").size(14))
        .on_press(Message::ClosePluginManagerPressed)
        .padding([8, 16])
        .style(button::secondary);

    let body = column![
        title,
        scrollable(list).height(Length::Fixed(320.0)),
        container(close_btn)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right),
    ]
    .spacing(18)
    .padding(24);

    card(body, 460.0)
}
