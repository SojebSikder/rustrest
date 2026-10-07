use crate::app::Rustrest;
use crate::message::{Message, MultilineFieldKind, ResizeKind};
use crate::ui::context_menu::{FieldTarget, with_context_menu};
use crate::ui::modal::{card, muted_text_color};
use crate::ui::multiline_input::multiline_input;
use crate::ui::tooltip::with_tooltip;
use iced::widget::{
    Space, button, checkbox, column, container, row, scrollable, text, text_input, tooltip,
};
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

    let muted = |label: String| {
        text(label).size(11).style(|theme: &Theme| text::Style {
            color: Some(muted_text_color(theme)),
        })
    };

    // cloud: where it syncs, or a way to share it
    let cloud_row: Option<Element<Message>> = match &env.cloud {
        Some(link) => {
            let status = if app.cloud.env_runs.contains(&link.team_id) {
                "Syncing with Rustrest Cloud..."
            } else if link.rev == 0 {
                "Uploading to Rustrest Cloud..."
            } else {
                "Synced with Rustrest Cloud. Changes are shared when you close the editor."
            };
            Some(
                row![
                    muted(status.to_string()).width(Length::Fill),
                    button(text("Stop syncing").size(12))
                        .on_press(Message::CloudUnlinkEnvironment(env_idx))
                        .padding([4, 10])
                        .style(button::secondary),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            )
        }
        None if app.cloud.client.is_some() => Some(
            row![
                muted("Only on this machine.".to_string()).width(Length::Fill),
                button(text("Upload to Cloud...").size(12))
                    .on_press(Message::OpenCloudEnvUpload(env_idx))
                    .padding([4, 10])
                    .style(button::secondary),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        ),
        None => None,
    };
    // whether each value syncs is only worth asking once there's a cloud
    let show_sync = env.cloud.is_some() || app.cloud.client.is_some();

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
    let mut headers = row![
        container(header_text("ACTIVE", Length::Fill))
            .width(Length::Fixed(50.0))
            .align_x(Alignment::Center),
        header_text("KEY", Length::FillPortion(1)),
        header_text("VALUE", Length::FillPortion(1)),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    if show_sync {
        headers = headers.push(
            container(header_text("SYNC", Length::Fill))
                .width(Length::Fixed(50.0))
                .align_x(Alignment::Center),
        );
    }
    headers = headers.push(
        container(header_text("ACTION", Length::Fill))
            .width(Length::Fixed(50.0))
            .align_x(Alignment::Center),
    );
    var_rows = var_rows.push(headers);

    // render each Key-Value pair
    for (var_idx, var) in env.variables.iter().enumerate() {
        let is_local = env.is_local(&var.key);
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
                if is_local && show_sync {
                    "Local value..."
                } else {
                    "Value..."
                },
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

        let mut row_item = row![active_checkbox, key_input, value_input]
            .spacing(10)
            .align_y(Alignment::Center);
        if show_sync {
            // a variable needs a key before it can be told apart from the others
            let sync_checkbox =
                checkbox(!is_local).on_toggle_maybe((!var.key.trim().is_empty()).then_some(
                    move |sync| Message::EnvVariableSyncToggled {
                        env_idx,
                        var_idx,
                        sync,
                    },
                ));
            let hint = if is_local {
                "Local: the value stays on this machine, teammates only see the key"
            } else {
                "Synced: the value is shared with your team"
            };
            row_item = row_item.push(
                container(with_tooltip(sync_checkbox, hint, tooltip::Position::Left))
                    .width(Length::Fixed(50.0))
                    .align_x(Alignment::Center),
            );
        }
        let row_item = row_item.push(delete_btn);

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

    let mut content = column![header].spacing(16).padding(24);
    if let Some(cloud_row) = cloud_row {
        content = content.push(cloud_row);
    }
    let mut footer = row![add_var_btn].spacing(12).align_y(Alignment::Center);
    if show_sync {
        footer = footer.push(Space::new().width(Length::Fill)).push(muted(
            "Unchecked values stay on this machine; teammates only see the key.".to_string(),
        ));
    }
    let content = content
        .push(scrollable(var_rows).height(Length::Fixed(260.0)))
        .push(footer);

    // wrap in a card that swallows clicks so they don't close the modal
    Some(card(content, 640.0))
}
