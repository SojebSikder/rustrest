use crate::app::Rustrest;
use crate::collection::collection::{CollectionItem, PostmanRequestNode, PostmanResponseExample};
use crate::message::{Message, SidebarDragItem, SidebarDropTarget};
use crate::ui::context_menu::{FieldTarget, with_context_menu};
use crate::ui::unsaved::{
    collection_is_unsaved, folder_is_unsaved, request_is_unsaved, unsaved_dot,
};
use iced::Padding;
use iced::widget::{
    Column, button, column, container, mouse_area, pick_list, row, scrollable, text, text_input,
};
use iced::{Alignment, Element, Font, Length};

pub fn render_sidebar(app: &Rustrest) -> Element<'_, Message> {
    let mut sidebar_contents = column![].spacing(10);

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
            let is_collapsed_col = app.collapsed_collections.contains(&col_id);

            let collection_header_title: Element<'_, Message> = if is_editing_col {
                row![
                    with_context_menu(
                        text_input("Collection Name...", &col.info.name)
                            .on_input(move |txt| Message::CollectionNameChanged(col_id, txt))
                            .on_submit(Message::SaveCollectionNamePressed(col_id))
                            .width(Length::Fixed(120.0))
                            .padding(2),
                        Message::ShowTextFieldContextMenu(
                            FieldTarget::CollectionName(col_id),
                            col.info.name.clone(),
                        ),
                    ),
                    button(text("💾").size(11))
                        .on_press(Message::SaveCollectionNamePressed(col_id))
                        .style(button::text)
                ]
                .spacing(5)
                .align_y(Alignment::Center)
                .into()
            } else {
                let collapse_arrow =
                    button(text(if is_collapsed_col { "▶" } else { "▼" }).size(10))
                        .on_press(Message::ToggleCollectionCollapsed(col_id))
                        .style(button::text)
                        .padding(2);

                let mut header_row = row![
                    collapse_arrow,
                    text(format!("📁 {}", col.info.name))
                        .font(Font {
                            weight: iced::font::Weight::Bold,
                            ..Font::DEFAULT
                        })
                        .size(14),
                ]
                .spacing(4)
                .align_y(Alignment::Center);

                if collection_is_unsaved(app, col) {
                    header_row = header_row.push(unsaved_dot());
                }

                if col.storage_dir.is_some() {
                    let has_changes = app
                        .git_status_cache
                        .get(&col_id)
                        .map(|r| matches!(r, Ok(s) if !s.files.is_empty()))
                        .unwrap_or(false);

                    header_row = header_row.push(text("🌿").size(11));
                    if has_changes {
                        header_row = header_row.push(
                            text("●")
                                .size(9)
                                .color(iced::Color::from_rgb(0.85, 0.55, 0.10)),
                        );
                    }
                }

                mouse_area(container(header_row).padding([4, 2]))
                    .on_press(Message::SidebarCollectionRootClicked(col_id))
                    .on_right_press(Message::ShowCollectionContextMenu(col_id))
                    .on_release(Message::SidebarDropped(SidebarDropTarget::CollectionRoot(
                        col_id,
                    )))
                    .into()
            };

            let mut col_tree = column![collection_header_title].spacing(4);

            if !is_collapsed_col {
                for item in &col.item {
                    col_tree = render_sidebar_item(app, col_tree, item, col_id, Vec::new());
                }
            }
            sidebar_contents = sidebar_contents.push(col_tree);
        }
    }

    container(scrollable(sidebar_contents))
        .width(Length::Fixed(app.sidebar_width))
        .height(Length::Fill)
        .padding(10)
        .style(container::bordered_box)
        .into()
}

pub fn render_env_selector(app: &Rustrest) -> Element<'_, Message> {
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
        .width(Length::Fixed(150.0)),
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

    container(env_row)
        .padding(Padding {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        })
        .into()
}

pub fn render_workspace_selector(app: &Rustrest) -> Element<'_, Message> {
    let active_id = app.active_workspace_id;
    let active_name = app
        .workspaces
        .iter()
        .find(|w| w.id == active_id)
        .map(|w| w.name.clone());

    let is_editing = app.editing_workspace_id == Some(active_id);

    let content: Element<'_, Message> = if is_editing {
        let current_name = active_name.clone().unwrap_or_default();
        row![
            with_context_menu(
                text_input("Workspace Name...", &current_name)
                    .on_input(move |txt| Message::WorkspaceNameChanged(active_id, txt))
                    .on_submit(Message::SaveWorkspaceNamePressed(active_id))
                    .width(Length::Fixed(120.0))
                    .padding(2),
                Message::ShowTextFieldContextMenu(
                    FieldTarget::WorkspaceName(active_id),
                    current_name.clone(),
                ),
            ),
            button(text("💾").size(11))
                .on_press(Message::SaveWorkspaceNamePressed(active_id))
                .style(button::text)
        ]
        .spacing(5)
        .align_y(Alignment::Center)
        .into()
    } else {
        let workspace_options: Vec<String> =
            app.workspaces.iter().map(|w| w.name.clone()).collect();

        let mut ws_row = row![
            pick_list(workspace_options, active_name, |selected| {
                Message::WorkspaceSelected(selected)
            })
            .placeholder("Workspace")
            .width(Length::Fixed(140.0)),
            button(text("+").size(14))
                .on_press(Message::CreateWorkspacePressed)
                .padding([4, 8])
                .style(button::secondary),
            button(text("✎").size(12))
                .on_press(Message::RenameWorkspacePressed(active_id))
                .padding([4, 6])
                .style(button::secondary),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        if app.workspaces.len() > 1 {
            ws_row = ws_row.push(
                button(text("✕").size(12))
                    .on_press(Message::DeleteWorkspacePressed(active_id))
                    .padding([4, 6])
                    .style(button::danger),
            );
        }

        ws_row.into()
    };

    container(content)
        .padding(Padding {
            top: 0.0,
            right: 0.0,
            bottom: 5.0,
            left: 0.0,
        })
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
            let path_for_toggle = current_path.clone();
            let path_for_drag = current_path.clone();
            let path_for_drop = current_path.clone();

            let is_editing_folder = app.editing_folder_collection_id == Some(collection_id)
                && app.editing_folder_path == current_path;
            let is_collapsed = app
                .collapsed_folders
                .contains(&(collection_id, current_path.clone()));

            let folder_title: Element<'_, Message> = if is_editing_folder {
                row![
                    with_context_menu(
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
                        Message::ShowTextFieldContextMenu(
                            FieldTarget::FolderName {
                                collection_id,
                                folder_path: path_for_right_click.clone(),
                            },
                            folder.name.clone(),
                        ),
                    ),
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
                let collapse_arrow = button(text(if is_collapsed { "▶" } else { "▼" }).size(10))
                    .on_press(Message::ToggleFolderCollapsed {
                        collection_id,
                        folder_path: path_for_toggle,
                    })
                    .style(button::text)
                    .padding(2);

                let mut title_row =
                    row![collapse_arrow, text(format!("📁 {}", folder.name)).size(14)]
                        .spacing(4)
                        .align_y(Alignment::Center);

                if folder_is_unsaved(app, &folder.item) {
                    title_row = title_row.push(unsaved_dot());
                }

                mouse_area(container(title_row).padding([2, 0]))
                    .on_press(Message::SidebarDragStarted(SidebarDragItem::Folder {
                        collection_id,
                        path: path_for_drag,
                    }))
                    .on_right_press(Message::ShowFolderContextMenu {
                        collection_id,
                        folder_path: path_for_right_click,
                    })
                    .on_release(Message::SidebarDropped(SidebarDropTarget::Folder {
                        collection_id,
                        folder_path: path_for_drop,
                    }))
                    .into()
            };

            let mut folder_layout = column![folder_title].spacing(3).padding(Padding {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: 10.0,
            });

            if !is_collapsed {
                for sub in &folder.item {
                    folder_layout = render_sidebar_item(
                        app,
                        folder_layout,
                        sub,
                        collection_id,
                        current_path.clone(),
                    );
                }
            }
            layout.push(folder_layout)
        }
        CollectionItem::Request(req_node) => {
            let req_clone = req_node.clone();
            let label = format!("{} - {}", req_node.request.method, req_node.name);
            let path_for_right_click = current_path.clone();
            let path_for_drag = current_path.clone();
            let path_for_drop = current_path.clone();
            let req_id = req_node.id;

            let mut label_row = row![text(label).size(13)]
                .spacing(4)
                .align_y(Alignment::Center);
            if request_is_unsaved(app, req_node) {
                label_row = label_row.push(unsaved_dot());
            }

            let mut req_layout = column![
                mouse_area(container(label_row).padding(Padding {
                    top: 2.0,
                    right: 0.0,
                    bottom: 2.0,
                    left: 15.0,
                }))
                .on_press(Message::SidebarRequestClicked {
                    req_node: req_clone,
                    collection_id,
                    parent_path: path_for_drag,
                })
                .on_right_press(Message::ShowRequestContextMenu {
                    collection_id,
                    folder_path: path_for_right_click,
                    request_id: req_id,
                })
                .on_release(Message::SidebarDropped(
                    SidebarDropTarget::Request {
                        collection_id,
                        parent_path: path_for_drop,
                        request_id: req_id,
                    }
                ))
            ]
            .spacing(2);

            if let Some(examples) = &req_node.response {
                for (index, example) in examples.iter().enumerate() {
                    req_layout = req_layout.push(render_saved_response_row(
                        app,
                        req_node,
                        collection_id,
                        index,
                        example,
                    ));
                }
            }

            layout.push(req_layout)
        }
    }
}

/// renders one saved response row nested under its parent request
fn render_saved_response_row<'a>(
    app: &'a Rustrest,
    req_node: &'a PostmanRequestNode,
    collection_id: usize,
    index: usize,
    example: &'a PostmanResponseExample,
) -> Element<'a, Message> {
    let request_id = req_node.id;
    let is_editing = app.editing_saved_response == Some((collection_id, request_id, index));

    if is_editing {
        row![
            text_input("Response Name...", &example.name)
                .on_input(move |txt| Message::SavedResponseNameChanged {
                    collection_id,
                    request_id,
                    index,
                    new_name: txt,
                })
                .on_submit(Message::SaveSavedResponseNamePressed)
                .width(Length::Fixed(120.0))
                .padding(2)
                .size(12),
            button(text("💾").size(10))
                .on_press(Message::SaveSavedResponseNamePressed)
                .style(button::text)
        ]
        .spacing(5)
        .align_y(Alignment::Center)
        .padding(Padding {
            top: 2.0,
            right: 0.0,
            bottom: 2.0,
            left: 30.0,
        })
        .into()
    } else {
        let title_row = row![
            text("📄").size(11),
            text(example.name.clone())
                .size(12)
                .color(iced::Color::from_rgb(0.55, 0.55, 0.55)),
        ]
        .spacing(4)
        .align_y(Alignment::Center);

        mouse_area(container(title_row).padding(Padding {
            top: 2.0,
            right: 0.0,
            bottom: 2.0,
            left: 30.0,
        }))
        .on_press(Message::SidebarSavedResponseClicked {
            req_node: req_node.clone(),
            collection_id,
            index,
        })
        .on_right_press(Message::ShowSavedResponseContextMenu {
            collection_id,
            request_id,
            index,
        })
        .into()
    }
}

pub use crate::ui::context_menu::render_context_menu_overlay;
