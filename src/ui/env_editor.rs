use crate::app::Rustrest;
use crate::message::Message;
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

pub fn render_env_editor(app: &Rustrest) -> Option<Element<'_, Message>> {
    let env_idx = app.editing_env_index?;
    let env = app.environments.get(env_idx)?;

    // header with title and close button
    let header = row![
        text(format!("Environment: {}", env.name)).size(16),
        button(text("✕").size(12))
            .on_press(Message::CloseEnvEditorPressed)
            .style(button::secondary)
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let mut var_rows = column![].spacing(6);

    // render table headers
    var_rows = var_rows.push(
        row![
            text("Active").width(Length::Fixed(50.0)).size(12),
            text("Key").width(Length::FillPortion(1)).size(12),
            text("Value").width(Length::FillPortion(1)).size(12),
            text("Action").width(Length::Fixed(60.0)).size(12),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    );

    // render each Key-Value pair
    for (var_idx, var) in env.variables.iter().enumerate() {
        let active_checkbox =
            checkbox(var.is_active).on_toggle(move |is_active| Message::EnvVariableToggled {
                env_idx,
                var_idx,
                is_active,
            });

        let key_input = text_input("Key...", &var.key)
            .on_input(move |key| Message::EnvVariableKeyChanged {
                env_idx,
                var_idx,
                key,
            })
            .padding(4);

        let value_input = text_input("Value...", &var.value)
            .on_input(move |value| Message::EnvVariableValueChanged {
                env_idx,
                var_idx,
                value,
            })
            .padding(4);

        let delete_btn = button(text("✕").size(12))
            .on_press(Message::DeleteEnvVariablePressed { env_idx, var_idx })
            .style(button::danger)
            .padding([4, 8]);

        let row_item = row![active_checkbox, key_input, value_input, delete_btn,]
            .spacing(10)
            .align_y(Alignment::Center);

        var_rows = var_rows.push(row_item);
    }

    // add Variable button
    let add_var_btn = button(text("+ Add Variable").size(12))
        .on_press(Message::AddEnvVariablePressed(env_idx))
        .style(button::secondary)
        .padding([6, 12]);

    let content = column![
        header,
        scrollable(var_rows).height(Length::Fixed(250.0)),
        add_var_btn
    ]
    .spacing(15);

    // wrap in a card container modal overlay
    Some(
        container(content)
            .padding(20)
            .width(Length::Fixed(550.0))
            .style(container::bordered_box)
            .into(),
    )
}
