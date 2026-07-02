use crate::app::Rustrest;
use crate::collection::CollectionItem;
use crate::message::Message;
use iced::Padding;
use iced::widget::{Column, button, column, container, pick_list, row, scrollable, text};
use iced::{Alignment, Element, Font, Length};

pub fn render_sidebar(app: &Rustrest) -> Element<'_, Message> {
    let env_options: Vec<String> = app.environments.iter().map(|e| e.name.clone()).collect();
    let current_env_selection = app
        .active_env_index
        .and_then(|idx| app.environments.get(idx))
        .map(|e| e.name.clone());

    let env_selector = row![
        pick_list(env_options, current_env_selection, |selected| {
            Message::EnvSelected(Some(selected))
        })
        .placeholder("No Environment")
        .width(Length::Fixed(180.0))
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let mut sidebar_contents = column![
        row![
            button("Import")
                .on_press(Message::ImportCollectionPressed)
                .padding(6)
                .width(Length::FillPortion(1)),
            button("+ New Col")
                .on_press(Message::CreateNewCollectionPressed)
                .padding(6)
                .width(Length::FillPortion(1)),
        ]
        .spacing(5),
        container(env_selector).padding(Padding {
            top: 5.0,
            right: 0.0,
            bottom: 10.0,
            left: 0.0,
        }),
    ]
    .spacing(10);

    if app.collections.is_empty() {
        sidebar_contents = sidebar_contents.push(
            text("No collections imported yet.")
                .size(11)
                .style(text::secondary),
        );
    } else {
        for col in &app.collections {
            let col_id = col.id;

            let collection_header = row![
                button(
                    text(format!("📁 {}", col.info.name))
                        .font(Font {
                            weight: iced::font::Weight::Bold,
                            ..Font::DEFAULT
                        })
                        .size(14),
                )
                .on_press(Message::SidebarCollectionRootClicked(col_id))
                .style(button::text)
                .padding([4, 2]),
                button(text("+F").size(11))
                    .on_press(Message::AddFolderPressed {
                        collection_id: col_id,
                        parent_folder_path: Vec::new()
                    })
                    .style(button::text),
                button(text("+R").size(11))
                    .on_press(Message::AddRequestPressed {
                        collection_id: col_id,
                        parent_folder_path: Vec::new()
                    })
                    .style(button::text),
                button(text("🗑").size(11))
                    .on_press(Message::DeleteCollectionPressed(col_id))
                    .style(button::text)
            ]
            .spacing(5)
            .align_y(Alignment::Center);

            let mut col_tree = column![collection_header].spacing(4);

            for item in &col.item {
                col_tree = render_sidebar_item(col_tree, item, col_id, Vec::new());
            }
            sidebar_contents = sidebar_contents.push(col_tree);
        }
    }

    container(scrollable(sidebar_contents))
        .width(Length::Fixed(260.0))
        .height(Length::Fill)
        .padding(10)
        .style(container::bordered_box)
        .into()
}

fn render_sidebar_item<'a>(
    layout: Column<'a, Message>,
    item: &'a CollectionItem,
    collection_id: usize,
    mut current_path: Vec<String>,
) -> Column<'a, Message> {
    match item {
        CollectionItem::Folder {
            name,
            item: sub_items,
        } => {
            current_path.push(name.clone());

            let path_for_add_folder = current_path.clone();
            let path_for_add_req = current_path.clone();
            let path_for_delete = current_path.clone();

            let folder_header = row![
                text(format!("📁 {}", name)).size(14),
                button(text("+F").size(10))
                    .on_press(Message::AddFolderPressed {
                        collection_id,
                        parent_folder_path: path_for_add_folder,
                    })
                    .style(button::text),
                button(text("+R").size(10))
                    .on_press(Message::AddRequestPressed {
                        collection_id,
                        parent_folder_path: path_for_add_req,
                    })
                    .style(button::text),
                button(text("🗑").size(10))
                    .on_press(Message::DeleteFolderPressed {
                        collection_id,
                        folder_path: path_for_delete,
                    })
                    .style(button::text)
            ]
            .spacing(5)
            .align_y(Alignment::Center);

            let mut folder_layout = column![folder_header].spacing(3).padding(Padding {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: 10.0,
            });

            for sub in sub_items {
                folder_layout =
                    render_sidebar_item(folder_layout, sub, collection_id, current_path.clone());
            }
            layout.push(folder_layout)
        }
        CollectionItem::Request(req_node) => {
            let req_clone = req_node.clone();
            let label = format!("{} - {}", req_node.request.method, req_node.name);
            let path_for_delete_req = current_path.clone();
            let req_id = req_node.id;

            layout.push(
                row![
                    button(text(label).size(13))
                        .on_press(Message::SidebarRequestClicked(req_clone))
                        .style(button::text)
                        .padding([2, 5]),
                    button(text("🗑").size(10))
                        .on_press(Message::DeleteRequestPressed {
                            collection_id,
                            parent_folder_path: path_for_delete_req,
                            request_id: req_id,
                        })
                        .style(button::text)
                ]
                .spacing(5)
                .align_y(Alignment::Center)
                .padding(Padding {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 10.0,
                }),
            )
        }
    }
}
