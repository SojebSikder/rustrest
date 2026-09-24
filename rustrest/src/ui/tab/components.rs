use super::types::{FormDataRow, FormDataType, KeyValuePair};
use crate::message::{KvValueField, MultilineFieldKind};
use crate::ui::context_menu::with_context_menu;
use crate::ui::multiline_input::multiline_input;
use iced::widget::{
    button, checkbox, column, pick_list, row, scrollable, space, text, text_editor, text_input,
};
use iced::{Alignment, Element, Length};

// TODO: later will reduce arguments
#[allow(clippy::too_many_arguments)]
pub fn kv_editor_pane<'a, Message>(
    pairs: &[KeyValuePair],
    value_contents: &'a [text_editor::Content],
    add_button_label: &'a str,
    on_change: impl Fn(usize, KeyValuePair) -> Message + Copy + 'a,
    on_value_action: impl Fn(usize, text_editor::Action) -> Message + Copy + 'a,
    on_add: Message,
    on_remove: impl Fn(usize) -> Message + Copy + 'a,
    on_show_key_menu: impl Fn(usize, String) -> Message + Copy + 'a,
    on_show_value_menu: impl Fn(usize, String) -> Message + Copy + 'a,
    tab_id: usize,
    field: KvValueField,
    get_height: impl Fn(MultilineFieldKind) -> f32 + Copy + 'a,
    on_resize_start: impl Fn(MultilineFieldKind) -> Message + Copy + 'a,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let mut content = column![].spacing(5);

    for (idx, item) in pairs.iter().enumerate() {
        let item_clone = item.clone();
        let key_clone = item.key.clone();
        let val_clone = item.value.clone();

        let value_field = match value_contents.get(idx) {
            Some(value_content) => {
                let key = MultilineFieldKind::KvValue {
                    tab_id,
                    field,
                    row: idx,
                };
                multiline_input(
                    "Value",
                    value_content,
                    8,
                    get_height(key),
                    move |action| on_value_action(idx, action),
                    on_show_value_menu(idx, item.value.clone()),
                    on_resize_start(key),
                )
            }
            None => text_input("Value", &item.value).padding(8).into(),
        };

        let row_element = row![
            checkbox(item.is_active).on_toggle(move |checked| {
                on_change(
                    idx,
                    KeyValuePair {
                        is_active: checked,
                        key: key_clone.clone(),
                        value: val_clone.clone(),
                    },
                )
            }),
            with_context_menu(
                text_input("Key", &item.key)
                    .on_input(move |k| {
                        on_change(
                            idx,
                            KeyValuePair {
                                is_active: item_clone.is_active,
                                key: k,
                                value: item_clone.value.clone(),
                            },
                        )
                    })
                    .padding(8),
                on_show_key_menu(idx, item.key.clone()),
            ),
            value_field,
            button("Delete")
                .on_press(on_remove(idx))
                .padding(8)
                .style(button::danger)
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        content = content.push(row_element);
    }

    column![
        scrollable(content).height(Length::Fixed(150.0)),
        button(add_button_label).on_press(on_add).padding(8)
    ]
    .spacing(10)
    .into()
}

#[allow(clippy::too_many_arguments)]
pub fn form_data_editor_pane<'a, Message>(
    rows: &'a [FormDataRow],
    value_contents: &'a [text_editor::Content],
    on_change: impl Fn(usize, FormDataRow) -> Message + Copy + 'a,
    on_value_action: impl Fn(usize, text_editor::Action) -> Message + Copy + 'a,
    on_type_change: impl Fn(usize, FormDataType) -> Message + Copy + 'a,
    on_file_pick: impl Fn(usize) -> Message + Copy + 'a,
    on_add: Message,
    on_remove: impl Fn(usize) -> Message + Copy + 'a,
    on_show_key_menu: impl Fn(usize, String) -> Message + Copy + 'a,
    on_show_value_menu: impl Fn(usize, String) -> Message + Copy + 'a,
    tab_id: usize,
    get_height: impl Fn(MultilineFieldKind) -> f32 + Copy + 'a,
    on_resize_start: impl Fn(MultilineFieldKind) -> Message + Copy + 'a,
    show_content_type: bool,
    on_toggle_content_type: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let mut content = column![].spacing(5);

    for (idx, item) in rows.iter().enumerate() {
        // dropdown type picker (Text vs File)
        let type_picker = pick_list(&FormDataType::ALL[..], Some(item.field_type), move |t| {
            on_type_change(idx, t)
        })
        .padding(6);

        // dynamically toggle value input field based on selected type
        let value_field: Element<'a, Message> = match item.field_type {
            FormDataType::Text => match value_contents.get(idx) {
                Some(value_content) => {
                    let key = MultilineFieldKind::FormDataValue { tab_id, row: idx };
                    multiline_input(
                        "Value",
                        value_content,
                        8,
                        get_height(key),
                        move |action| on_value_action(idx, action),
                        on_show_value_menu(idx, item.value.clone()),
                        on_resize_start(key),
                    )
                }
                None => text_input("Value", &item.value).padding(8).into(),
            },
            FormDataType::File => {
                let display_path = if item.value.is_empty() {
                    "No file selected"
                } else {
                    &item.value
                };
                row![
                    button(text("Select File").size(12))
                        .padding(6)
                        .on_press(on_file_pick(idx)),
                    text(display_path).size(12).width(Length::Fill)
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .width(Length::Fill)
                .into()
            }
        };

        let cb_item_clone = item.clone();
        let ki_item_clone = item.clone();
        let ct_item_clone = item.clone();

        let mut row_element = row![
            checkbox(item.is_active).on_toggle(move |checked| {
                on_change(
                    idx,
                    FormDataRow {
                        is_active: checked,
                        ..cb_item_clone.clone()
                    },
                )
            }),
            with_context_menu(
                text_input("Key", &item.key)
                    .on_input(move |k| {
                        on_change(
                            idx,
                            FormDataRow {
                                key: k,
                                ..ki_item_clone.clone()
                            },
                        )
                    })
                    .padding(8)
                    .width(Length::Fixed(150.0)),
                on_show_key_menu(idx, item.key.clone()),
            ),
            type_picker,
            value_field,
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        // per-part Content-Type, e.g. `application/json` for a JSON text
        // field sent alongside files. blank means the default: no header for
        // text, a type guessed from the extension for files, the placeholder shows which.
        if show_content_type {
            let placeholder = match item.field_type {
                FormDataType::File if !item.value.is_empty() => {
                    let file_name = std::path::Path::new(&item.value)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default();
                    format!("Auto ({})", rustrest_core::http::guess_mime(file_name))
                }
                _ => "Auto".to_string(),
            };
            row_element = row_element.push(
                text_input(&placeholder, &item.content_type)
                    .on_input(move |content_type| {
                        on_change(
                            idx,
                            FormDataRow {
                                content_type,
                                ..ct_item_clone.clone()
                            },
                        )
                    })
                    .padding(8)
                    .width(Length::Fixed(170.0)),
            );
        }

        row_element = row_element.push(
            button("Delete")
                .on_press(on_remove(idx))
                .padding(8)
                .style(button::danger),
        );

        content = content.push(row_element);
    }

    let toggle_label = if show_content_type {
        "Hide Content-Type"
    } else {
        "Show Content-Type"
    };

    column![
        row![
            space::horizontal(),
            button(text(toggle_label).size(12))
                .on_press(on_toggle_content_type)
                .padding([2, 8])
                .style(button::text),
        ],
        scrollable(content).height(Length::Fixed(150.0)),
        button("Add Form Field").on_press(on_add).padding(8)
    ]
    .spacing(6)
    .into()
}
