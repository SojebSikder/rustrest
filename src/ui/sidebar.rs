use crate::app::Rustrest;
use crate::collection::collection::CollectionItem;
use crate::message::Message;
use iced::Padding;
use iced::widget::{
    Column, button, column, container, mouse_area, opaque, pick_list, row, scrollable, text,
    text_input,
};
use iced::{Alignment, Element, Font, Length};

pub fn render_sidebar(app: &Rustrest) -> Element<'_, Message> {
    let env_options: Vec<String> = app.environments.iter().map(|e| e.name.clone()).collect();
    let current_env_selection = app
        .active_env_index
        .and_then(|idx| app.environments.get(idx))
        .map(|e| e.name.clone());

    // build environment selector row with controls
    let mut env_row = row![
        pick_list(env_options, current_env_selection, |selected| {
            Message::EnvSelected(Some(selected))
        })
        .placeholder("No Environment")
        .width(Length::Fixed(140.0)),
        // add Environment button
        button(text("+").size(14))
            .on_press(Message::CreateEnvironmentPressed)
            .padding([4, 8])
            .style(button::secondary)
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    // show Edit and Delete buttons if an active environment is selected
    if let Some(active_idx) = app.active_env_index {
        env_row = env_row
            .push(
                button(text("⚙️").size(12))
                    .on_press(Message::EditEnvironmentPressed(active_idx))
                    .padding([4, 6])
                    .style(button::secondary),
            )
            .push(
                button(text("✕").size(12))
                    .on_press(Message::DeleteEnvironmentPressed(active_idx))
                    .padding([4, 6])
                    .style(button::danger),
            );
    }

    let env_selector = container(env_row).padding(Padding {
        top: 5.0,
        right: 0.0,
        bottom: 10.0,
        left: 0.0,
    });

    let mut sidebar_contents = column![env_selector].spacing(10);

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
                .on_press(Message::SidebarCollectionRootClicked(col_id))
                .on_right_press(Message::ShowCollectionContextMenu(col_id))
                .into()
            };

            let mut col_tree = column![collection_header_title].spacing(4);

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
            let path_for_right_click = current_path.clone();

            let is_editing_folder = app.editing_folder_collection_id == Some(collection_id)
                && app.editing_folder_path == current_path;

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
            let path_for_right_click = current_path.clone();
            let req_id = req_node.id;

            let req_layout = column![
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
                    folder_path: path_for_right_click,
                    request_id: req_id,
                })
            ];

            layout.push(req_layout)
        }
    }
}

pub fn render_context_menu_overlay<'a>(app: &Rustrest) -> Option<Element<'a, Message>> {
    let context_menu = app.active_context_menu.as_ref()?;

    // suppress the panel while the targeted item is mid-rename, since its
    // row is showing a text input instead of the label the menu anchors to
    let is_editing = match context_menu {
        crate::app::ContextMenu::Collection(id) => app.editing_collection_id == Some(*id),
        crate::app::ContextMenu::Folder { col_id, path } => {
            app.editing_folder_collection_id == Some(*col_id) && app.editing_folder_path == *path
        }
        crate::app::ContextMenu::Request { .. } => false,
    };
    if is_editing {
        return None;
    }

    let options: Vec<(&'a str, Message)> = match context_menu {
        crate::app::ContextMenu::Collection(id) => {
            let col_id = *id;
            vec![
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
                (
                    "Save as git folder...",
                    Message::InitGitCollectionPressed(col_id),
                ),
                ("Export As...", Message::ExportCollectionPressed(col_id)),
                ("Delete", Message::DeleteCollectionPressed(col_id)),
            ]
        }
        crate::app::ContextMenu::Folder { col_id, path } => {
            let collection_id = *col_id;
            vec![
                (
                    "Rename",
                    Message::RenameFolderPressed {
                        collection_id,
                        folder_path: path.clone(),
                    },
                ),
                (
                    "New Folder",
                    Message::AddFolderPressed {
                        collection_id,
                        parent_folder_path: path.clone(),
                    },
                ),
                (
                    "New Request",
                    Message::AddRequestPressed {
                        collection_id,
                        parent_folder_path: path.clone(),
                    },
                ),
                (
                    "Delete",
                    Message::DeleteFolderPressed {
                        collection_id,
                        folder_path: path.clone(),
                    },
                ),
            ]
        }
        crate::app::ContextMenu::Request {
            col_id,
            folder_path,
            req_id,
        } => vec![(
            "Delete",
            Message::DeleteRequestPressed {
                collection_id: *col_id,
                parent_folder_path: folder_path.clone(),
                request_id: *req_id,
            },
        )],
    };

    let dropdown = render_dropdown(options);
    let pos = app.context_menu_position;

    // spacer trick: pad down/right to the captured cursor position so the
    // panel appears to float at the click site instead of shifting layout.
    Some(
        column![
            container(text("")).height(Length::Fixed(pos.y)),
            row![
                container(text("")).width(Length::Fixed(pos.x)),
                opaque(dropdown)
            ]
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into(),
    )
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
