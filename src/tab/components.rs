use super::types::{FormDataRow, FormDataType, KeyValuePair};
use iced::widget::{button, checkbox, column, pick_list, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

pub fn kv_editor_pane<'a, Message>(
    pairs: &[KeyValuePair],
    add_button_label: &'a str,
    on_change: impl Fn(usize, KeyValuePair) -> Message + Copy + 'a,
    on_add: Message,
    on_remove: impl Fn(usize) -> Message + Copy + 'a,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let mut content = column![].spacing(5);

    for (idx, item) in pairs.iter().enumerate() {
        let item_clone = item.clone();
        let key_clone = item.key.clone();
        let val_clone = item.value.clone();

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
            text_input("Value", &item.value)
                .on_input(move |v| {
                    on_change(
                        idx,
                        KeyValuePair {
                            is_active: item_clone.is_active,
                            key: item_clone.key.clone(),
                            value: v,
                        },
                    )
                })
                .padding(8),
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

pub fn form_data_editor_pane<'a, Message>(
    rows: &'a [FormDataRow],
    on_change: impl Fn(usize, FormDataRow) -> Message + Copy + 'a,
    on_type_change: impl Fn(usize, FormDataType) -> Message + Copy + 'a,
    on_file_pick: impl Fn(usize) -> Message + Copy + 'a,
    on_add: Message,
    on_remove: impl Fn(usize) -> Message + Copy + 'a,
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
            FormDataType::Text => {
                let text_item_clone = item.clone();
                text_input("Value", &item.value)
                    .on_input(move |v| {
                        on_change(
                            idx,
                            FormDataRow {
                                is_active: text_item_clone.is_active,
                                key: text_item_clone.key.clone(),
                                value: v,
                                field_type: text_item_clone.field_type,
                            },
                        )
                    })
                    .padding(8)
                    .into()
            }
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

        let row_element = row![
            checkbox(item.is_active).on_toggle(move |checked| {
                on_change(
                    idx,
                    FormDataRow {
                        is_active: checked,
                        key: cb_item_clone.key.clone(),
                        value: cb_item_clone.value.clone(),
                        field_type: cb_item_clone.field_type,
                    },
                )
            }),
            text_input("Key", &item.key)
                .on_input(move |k| {
                    on_change(
                        idx,
                        FormDataRow {
                            is_active: ki_item_clone.is_active,
                            key: k,
                            value: ki_item_clone.value.clone(),
                            field_type: ki_item_clone.field_type,
                        },
                    )
                })
                .padding(8)
                .width(Length::Fixed(150.0)),
            type_picker,
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
        button("Add Form Field").on_press(on_add).padding(8)
    ]
    .spacing(10)
    .into()
}
