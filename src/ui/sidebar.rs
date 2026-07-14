use crate::app::Rustrest;
use crate::collection::collection::CollectionItem;
use crate::message::Message;
use iced::Padding;
use iced::widget::{
    Column, button, column, container, mouse_area, pick_list, row, scrollable, text, text_input,
};
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
        // row![
        //     button("Import")
        //         .on_press(Message::ImportCollectionPressed)
        //         .padding(6)
        //         .width(Length::FillPortion(1)),
        //     button("+ Collection")
        //         .on_press(Message::CreateNewCollectionPressed)
        //         .padding(6)
        //         .width(Length::FillPortion(1)),
        // ]
        // .spacing(5),
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
            let is_editing_col = app.editing_collection_id == Some(col_id);

            // determine if this collection's context menu dropdown is open
            let show_dropdown = matches!(&app.active_context_menu, Some(crate::app::ContextMenu::Collection(id)) if *id == col_id);

            let collection_header_title: Element<'_, Message> = if is_editing_col {
                row![
                    text_input("Collection Name...", &col.info.name)
                        .on_input(move |txt| Message::CollectionNameChanged(col_id, txt))
                        .on_submit(Message::SaveCollectionNamePressed(col_id))
                        .width(Length::Fixed(120.0))
                        .padding(2),
                    button(text("💾").size(11))
                        .on_press(Message::SaveCollectionNamePressed(col_id))
                        .style(button::text)
                ]
                .spacing(5)
                .align_y(Alignment::Center)
                .into()
            } else {
                // wrap the standard header view with mouse_area to intercept right click
                mouse_area(
                    container(
                        text(format!("📁 {}", col.info.name))
                            .font(Font {
                                weight: iced::font::Weight::Bold,
                                ..Font::DEFAULT
                            })
                            .size(14),
                    )
                    .padding([4, 2]),
                )
                .on_press(Message::SidebarCollectionRootClicked(col_id)) // left click opens it
                .on_right_press(Message::ShowCollectionContextMenu(col_id)) // right click opens dropdown
                .into()
            };

            let mut col_tree = column![collection_header_title].spacing(4);

            // render dropdown if active
            if show_dropdown && !is_editing_col {
                let dropdown = render_dropdown(vec![
                    ("Rename", Message::RenameCollectionPressed(col_id)),
                    (
                        "New Folder",
                        Message::AddFolderPressed {
                            collection_id: col_id,
                            parent_folder_path: Vec::new(),
                        },
                    ),
                    (
                        "New Request",
                        Message::AddRequestPressed {
                            collection_id: col_id,
                            parent_folder_path: Vec::new(),
                        },
                    ),
                    ("Save Collection", Message::SaveCollectionPressed(col_id)),
                    ("Export As...", Message::ExportCollectionPressed(col_id)),
                    ("Delete", Message::DeleteCollectionPressed(col_id)),
                ]);
                col_tree = col_tree.push(dropdown);
            }

            for item in &col.item {
                col_tree = render_sidebar_item(app, col_tree, item, col_id, Vec::new());
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
    app: &'a Rustrest,
    layout: Column<'a, Message>,
    item: &'a CollectionItem,
    collection_id: usize,
    mut current_path: Vec<String>,
) -> Column<'a, Message> {
    match item {
        CollectionItem::Folder(folder) => {
            current_path.push(folder.name.clone());

            let path_for_change = current_path.clone();
            let path_for_save = current_path.clone();
            let path_for_rename_trigger = current_path.clone();
            let path_for_add_folder = current_path.clone();
            let path_for_add_req = current_path.clone();
            let path_for_delete = current_path.clone();
            let path_for_right_click = current_path.clone();

            let is_editing_folder = app.editing_folder_collection_id == Some(collection_id)
                && app.editing_folder_path == current_path;

            // determine if this folder's dropdown is open
            let show_dropdown = matches!(&app.active_context_menu, Some(crate::app::ContextMenu::Folder { col_id, path }) if *col_id == collection_id && *path == current_path);

            let folder_title: Element<'_, Message> = if is_editing_folder {
                row![
                    text_input("Folder Name...", &folder.name)
                        .on_input(move |txt| Message::FolderNameChanged {
                            collection_id,
                            folder_path: path_for_change.clone(),
                            new_name: txt,
                        })
                        .on_submit(Message::SaveFolderNamePressed {
                            collection_id,
                            folder_path: path_for_save.clone(),
                        })
                        .width(Length::Fixed(110.0))
                        .padding(2),
                    button(text("💾").size(10))
                        .on_press(Message::SaveFolderNamePressed {
                            collection_id,
                            folder_path: current_path.clone(),
                        })
                        .style(button::text)
                ]
                .spacing(5)
                .align_y(Alignment::Center)
                .into()
            } else {
                mouse_area(container(text(format!("📁 {}", folder.name)).size(14)).padding([2, 0]))
                    .on_right_press(Message::ShowFolderContextMenu {
                        collection_id,
                        folder_path: path_for_right_click,
                    })
                    .into()
            };

            let mut folder_layout = column![folder_title].spacing(3).padding(Padding {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: 10.0,
            });

            if show_dropdown && !is_editing_folder {
                let dropdown = render_dropdown(vec![
                    (
                        "Rename",
                        Message::RenameFolderPressed {
                            collection_id,
                            folder_path: path_for_rename_trigger,
                        },
                    ),
                    (
                        "New Folder",
                        Message::AddFolderPressed {
                            collection_id,
                            parent_folder_path: path_for_add_folder,
                        },
                    ),
                    (
                        "New Request",
                        Message::AddRequestPressed {
                            collection_id,
                            parent_folder_path: path_for_add_req,
                        },
                    ),
                    (
                        "Delete",
                        Message::DeleteFolderPressed {
                            collection_id,
                            folder_path: path_for_delete,
                        },
                    ),
                ]);
                folder_layout = folder_layout.push(dropdown);
            }

            for sub in &folder.item {
                folder_layout = render_sidebar_item(
                    app,
                    folder_layout,
                    sub,
                    collection_id,
                    current_path.clone(),
                );
            }
            layout.push(folder_layout)
        }
        CollectionItem::Request(req_node) => {
            let req_clone = req_node.clone();
            let label = format!("{} - {}", req_node.request.method, req_node.name);
            let path_for_delete_req = current_path.clone();
            let req_id = req_node.id;

            let show_dropdown = matches!(&app.active_context_menu, Some(crate::app::ContextMenu::Request { col_id, req_id: id }) if *col_id == collection_id && *id == req_id);

            let mut req_layout = column![
                mouse_area(
                    container(
                        button(text(label).size(13))
                            .on_press(Message::SidebarRequestClicked(req_clone))
                            .style(button::text)
                            .padding([2, 5])
                    )
                    .padding(Padding {
                        top: 0.0,
                        right: 0.0,
                        bottom: 0.0,
                        left: 10.0,
                    })
                )
                .on_right_press(Message::ShowRequestContextMenu {
                    collection_id,
                    request_id: req_id,
                })
            ];

            if show_dropdown {
                let dropdown = render_dropdown(vec![(
                    "Delete",
                    Message::DeleteRequestPressed {
                        collection_id,
                        parent_folder_path: path_for_delete_req,
                        request_id: req_id,
                    },
                )]);
                req_layout = req_layout.push(dropdown);
            }

            layout.push(req_layout)
        }
    }
}

fn render_dropdown<'a>(options: Vec<(&'a str, Message)>) -> Element<'a, Message> {
    let mut menu = column![].spacing(2);

    for (label, message) in options {
        menu = menu.push(
            button(
                text(label)
                    .size(12)
                    .width(Length::Fill)
                    .style(text::primary),
            )
            .on_press(message)
            .padding([4, 8])
            .style(button::text)
            .width(Length::Fill),
        );
    }

    container(menu)
        .padding(4)
        .width(Length::Fixed(140.0))
        .style(container::bordered_box)
        .into()
}
