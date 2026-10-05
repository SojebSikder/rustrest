use crate::app::Rustrest;
use crate::message::{Message, MultilineFieldKind, ResizeKind};
use crate::ui::context_menu::{FieldTarget, with_context_menu};
use crate::ui::modal::{card, muted_text_color};
use crate::ui::multiline_input::multiline_input;
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Font, Length, Theme};

pub fn render_env_editor(app: &Rustrest) -> Option<Element<'_, Message>> {
    let env_idx = app.env.editing_env_index?;
    let env = app.env.environments.get(env_idx)?;

    // header with editable name, rename toggle, and close button
    let name_display: Element<Message> = if app.env.editing_env_name {
        with_context_menu(
            text_input("Environment name...", &env.name)
                .on_input(move |name| Message::EnvNameChanged(env_idx, name))
                .on_submit(Message::SaveEnvNamePressed(env_idx))
                .padding([6, 10])
                .size(15)
                .width(Length::Fill),
            Message::ShowTextFieldContextMenu(FieldTarget::EnvName(env_idx), env.name.clone()),
        )
    } else {
        text(&env.name)
            .size(16)
            .font(Font {
                weight: iced::font::Weight::Bold,
                ..Font::DEFAULT
            })
            .width(Length::Fill)
            .into()
    };

    let rename_btn: Element<Message> = if app.env.editing_env_name {
        button(text("✓").size(12))
            .on_press(Message::SaveEnvNamePressed(env_idx))
            .padding([6, 10])
            .style(button::primary)
            .into()
    } else {
        button(text("✎").size(12))
            .on_press(Message::RenameEnvironmentPressed(env_idx))
            .padding([6, 10])
            .style(button::secondary)
            .into()
    };

    let header = row![
        name_display,
        rename_btn,
        button(text("✕").size(12))
            .on_press(Message::CloseEnvEditorPressed)
            .padding([6, 10])
            .style(button::secondary)
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let mut var_rows = column![].spacing(6);

    let header_text = |label: &'static str, width: Length| {
        text(label)
            .width(width)
            .size(11)
            .font(Font {
                weight: iced::font::Weight::Bold,
                ..Font::DEFAULT
            })
            .style(|theme: &Theme| text::Style {
                color: Some(muted_text_color(theme)),
            })
    };

    // render table headers
    var_rows = var_rows.push(
        row![
            container(header_text("ACTIVE", Length::Fill))
                .width(Length::Fixed(50.0))
                .align_x(Alignment::Center),
            header_text("KEY", Length::FillPortion(1)),
            header_text("VALUE", Length::FillPortion(1)),
            container(header_text("ACTION", Length::Fill))
                .width(Length::Fixed(50.0))
                .align_x(Alignment::Center),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    );

    // render each Key-Value pair
    for (var_idx, var) in env.variables.iter().enumerate() {
        let active_checkbox = container(checkbox(var.is_active).on_toggle(move |is_active| {
            Message::EnvVariableToggled {
                env_idx,
                var_idx,
                is_active,
            }
        }))
        .width(Length::Fixed(50.0))
        .align_x(Alignment::Center);

        let key_input = container(with_context_menu(
            text_input("Key...", &var.key)
                .on_input(move |key| Message::EnvVariableKeyChanged {
                    env_idx,
                    var_idx,
                    key,
                })
                .padding([6, 8])
                .width(Length::Fill),
            Message::ShowTextFieldContextMenu(
                FieldTarget::EnvVarKey { env_idx, var_idx },
                var.key.clone(),
            ),
        ))
        .width(Length::FillPortion(1));

        let value_input: Element<Message> = match app.env.env_var_value_contents.get(var_idx) {
            Some(value_content) => container(multiline_input(
                "Value...",
                value_content,
                4,
                app.layout
                    .multiline_height(MultilineFieldKind::EnvVarValue { env_idx, var_idx }),
                move |action| Message::EnvVariableValueEditorAction {
                    env_idx,
                    var_idx,
                    action,
                },
                Message::ShowTextFieldContextMenu(
                    FieldTarget::EnvVarValue { env_idx, var_idx },
                    var.value.clone(),
                ),
                Message::ResizeDragStarted(ResizeKind::MultilineField(
                    MultilineFieldKind::EnvVarValue { env_idx, var_idx },
                )),
            ))
            .width(Length::FillPortion(1))
            .into(),
            None => text("").into(),
        };

        let delete_btn = container(
            button(text("✕").size(12))
                .on_press(Message::DeleteEnvVariablePressed { env_idx, var_idx })
                .style(button::text)
                .padding([4, 8]),
        )
        .width(Length::Fixed(50.0))
        .align_x(Alignment::Center);

        let row_item = row![active_checkbox, key_input, value_input, delete_btn]
            .spacing(10)
            .align_y(Alignment::Center);

        var_rows = var_rows.push(row_item);
    }

    // add Variable button
    let add_var_btn = button(
        row![text("+").size(13), text("Add Variable").size(12)]
            .spacing(6)
            .align_y(Alignment::Center),
    )
    .on_press(Message::AddEnvVariablePressed(env_idx))
    .style(button::secondary)
    .padding([6, 14]);

    let content = column![
        header,
        scrollable(var_rows).height(Length::Fixed(260.0)),
        add_var_btn
    ]
    .spacing(16)
    .padding(24);

    // wrap in a card that swallows clicks so they don't close the modal
    Some(card(content, 640.0))
}
