use super::types::KeyValuePair;
use iced::widget::{button, checkbox, column, row, scrollable, text_input};
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
            checkbox("", item.is_active).on_toggle(move |checked| {
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
                .style(iced::theme::Button::Destructive)
        ]
        .spacing(8)
        .align_items(Alignment::Center);

        content = content.push(row_element);
    }

    column![
        scrollable(content).height(Length::Fixed(150.0)),
        button(add_button_label).on_press(on_add).padding(8)
    ]
    .spacing(10)
    .into()
}
