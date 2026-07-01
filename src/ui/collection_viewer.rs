use crate::app::CollectionSubTab;
use crate::collection::PostmanCollection;
use crate::message::Message;
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Theme};

pub fn render_collection_root(
    collection_id: usize,
    collection_name: &str,
    active_sub_tab: &CollectionSubTab,
    collections: &[PostmanCollection],
) -> Element<'static, Message, Theme, iced::Renderer> {
    // find current live collection data
    let target_collection = collections.iter().find(|c| c.id == collection_id);

    // tab headers bavigation bar
    let tabs_nav = row![
        button(text("Overview"))
            .style(if *active_sub_tab == CollectionSubTab::Documentation {
                button::primary
            } else {
                button::secondary
            })
            .on_press(Message::CollectionSubTabSelected(
                CollectionSubTab::Documentation
            )),
        button(text("Variables"))
            .style(if *active_sub_tab == CollectionSubTab::Variables {
                button::primary
            } else {
                button::secondary
            })
            .on_press(Message::CollectionSubTabSelected(
                CollectionSubTab::Variables
            )),
    ]
    .spacing(10);

    // content pane layout
    let content_pane: Element<'static, Message, Theme, iced::Renderer> = match active_sub_tab {
        CollectionSubTab::Variables => {
            let mut vars_column: iced::widget::Column<'_, Message, Theme, iced::Renderer> =
                column![
                    row![
                        text("").width(Length::Shrink),
                        text("Variable Key").width(Length::FillPortion(2)),
                        text("Current Value").width(Length::FillPortion(3)),
                        text("Actions").width(Length::Shrink),
                    ]
                    .spacing(10)
                    .padding(5)
                ]
                .spacing(10);

            if let Some(col) = target_collection {
                if let Some(ref variables) = col.variable {
                    for (idx, var) in variables.iter().enumerate() {
                        let key_str = var.key.clone();
                        let val_str = match &var.value {
                            Some(serde_json::Value::String(s)) => s.clone(),
                            Some(other) => other.to_string().trim_matches('"').to_string(),
                            None => String::new(),
                        };

                        let is_disabled = var.r#type.as_deref() == Some("disabled");

                        let val_str_for_key_input = val_str.clone();
                        let key_str_for_val_input = key_str.clone();

                        let row_item = row![
                            checkbox(!is_disabled).on_toggle(move |checked| {
                                Message::CollectionVariableToggled {
                                    collection_id,
                                    index: idx,
                                    is_active: checked,
                                }
                            }),
                            text_input("Variable key...", &key_str)
                                .on_input(move |new_key| Message::CollectionVariableChanged {
                                    collection_id,
                                    index: idx,
                                    key: new_key,
                                    value: val_str_for_key_input.clone(),
                                })
                                .width(Length::FillPortion(2)),
                            text_input("Value...", &val_str)
                                .on_input(move |new_val| Message::CollectionVariableChanged {
                                    collection_id,
                                    index: idx,
                                    key: key_str_for_val_input.clone(),
                                    value: new_val,
                                })
                                .width(Length::FillPortion(3)),
                            button(text("X")).style(button::danger).on_press(
                                Message::DeleteCollectionVariablePressed(collection_id, idx)
                            ),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center);

                        vars_column = vars_column.push(row_item);
                    }
                }
            }

            column![
                scrollable(vars_column).height(Length::FillPortion(4)),
                button(text("+ Add Variable"))
                    .on_press(Message::AddCollectionVariablePressed(collection_id))
            ]
            .spacing(15)
            .into()
        }
        CollectionSubTab::Documentation => column![text(format!(
            "Documentation for collection: {}",
            collection_name
        ))]
        .into(),
    };

    column![
        text(collection_name.to_string()).size(28),
        tabs_nav,
        container(content_pane)
            .padding(10)
            .width(Length::Fill)
            .height(Length::Fill)
    ]
    .spacing(20)
    .padding(20)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
