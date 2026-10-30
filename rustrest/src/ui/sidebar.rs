use crate::app::Rustrest;
use crate::collection::collection::{
    CollectionItem, PostmanCollection, PostmanRequestNode, PostmanResponseExample,
};
use crate::message::{Message, SidebarDropTarget, SidebarItemKey};
use crate::ui::context_menu::{FieldTarget, with_context_menu};
use crate::ui::modal::muted_text_color;
use crate::ui::spinner::spinner_with_label;
use crate::ui::tokens::{
    SPACE_MD, SPACE_SM, SPACE_XS, TEXT_BASE, TEXT_BODY, TEXT_CAPTION, TEXT_SMALL, card_surface,
    icon_button, input_style, list_row, primary_button, secondary_button, semibold,
};
use crate::ui::unsaved::{
    collection_is_unsaved, folder_is_unsaved, request_is_unsaved, unsaved_dot,
};
use iced::Padding;
use iced::widget::{
    Column, Space, button, column, container, mouse_area, pick_list, row, scrollable, text,
    text_input,
};
use iced::{Alignment, Element, Font, Length};

fn sidebar_click_message(app: &Rustrest, key: SidebarItemKey, default: Message) -> Message {
    let modifiers = app.sidebar.current_modifiers;
    if modifiers.shift() {
        Message::SidebarItemRangeSelect(key)
    } else if modifiers.command() {
        Message::SidebarItemToggleSelect(key)
    } else {
        default
    }
}

/// walks the sidebar tree in the same order it's rendered (respecting
/// collapsed collections/folders) so Shift-click range-selection can slice
/// between an anchor row and the clicked row.
pub fn flatten_visible_sidebar_items(app: &Rustrest) -> Vec<SidebarItemKey> {
    let query = search_query(app);
    let query = query.as_deref();
    let mut out = Vec::new();

    for col in &app.collections {
        let Some(filter) = collection_filter(col, query) else {
            continue;
        };
        out.push(SidebarItemKey::Collection(col.id));
        if query.is_none() && app.sidebar.collapsed_collections.contains(&col.id) {
            continue;
        }
        let mut path = Vec::new();
        for item in &col.item {
            flatten_sidebar_item(app, item, col.id, &mut path, query, filter, &mut out);
        }
    }
    out
}

fn flatten_sidebar_item(
    app: &Rustrest,
    item: &CollectionItem,
    collection_id: usize,
    current_path: &mut Vec<String>,
    query: Option<&str>,
    filter: Option<&str>,
    out: &mut Vec<SidebarItemKey>,
) {
    if !item_visible(item, filter) {
        return;
    }
    match item {
        CollectionItem::Folder(folder) => {
            current_path.push(folder.name.clone());
            out.push(SidebarItemKey::Folder {
                collection_id,
                path: current_path.clone(),
            });
            let is_collapsed = query.is_none()
                && app
                    .sidebar
                    .collapsed_folders
                    .contains(&(collection_id, current_path.clone()));
            if !is_collapsed {
                let filter = child_filter(&folder.name, filter);
                for sub in &folder.item {
                    flatten_sidebar_item(app, sub, collection_id, current_path, query, filter, out);
                }
            }
            current_path.pop();
        }
        CollectionItem::Request(req) => {
            out.push(SidebarItemKey::Request {
                collection_id,
                parent_path: current_path.clone(),
                request_id: req.id,
            });
        }
    }
}

pub fn search_input_id() -> iced::widget::Id {
    iced::widget::Id::new("rustrest-sidebar-search")
}

/// the active search text, lowercased; `None` when the search box is empty
fn search_query(app: &Rustrest) -> Option<String> {
    let query = app.sidebar.search_query.trim();
    (!query.is_empty()).then(|| query.to_lowercase())
}

fn matches_query(haystack: &str, query: &str) -> bool {
    haystack.to_lowercase().contains(query)
}

/// a request matches on its name, method or URL
fn request_matches(req: &PostmanRequestNode, query: &str) -> bool {
    matches_query(&req.name, query)
        || matches_query(&req.request.method, query)
        || req
            .request
            .url
            .as_ref()
            .is_some_and(|url| matches_query(&url.to_string(), query))
}

/// whether an item shows up under `filter`: a request that matches, or a
/// folder whose name matches or that contains a match. `None` shows everything
fn item_visible(item: &CollectionItem, filter: Option<&str>) -> bool {
    let Some(query) = filter else {
        return true;
    };
    match item {
        CollectionItem::Request(req) => request_matches(req, query),
        CollectionItem::Folder(folder) => {
            matches_query(&folder.name, query)
                || folder.item.iter().any(|sub| item_visible(sub, filter))
        }
    }
}

/// a folder whose own name matches shows all of its contents
fn child_filter<'q>(folder_name: &str, filter: Option<&'q str>) -> Option<&'q str> {
    filter.filter(|query| !matches_query(folder_name, query))
}

fn collection_filter<'q>(
    col: &PostmanCollection,
    query: Option<&'q str>,
) -> Option<Option<&'q str>> {
    let Some(q) = query else {
        return Some(None);
    };
    if matches_query(&col.info.name, q) {
        Some(None)
    } else if col.item.iter().any(|item| item_visible(item, query)) {
        Some(query)
    } else {
        None
    }
}

fn render_search_box(app: &Rustrest) -> Element<'_, Message> {
    let mut search_row = row![with_context_menu(
        text_input("Search collections", &app.sidebar.search_query)
            .id(search_input_id())
            .on_input(Message::SidebarSearchChanged)
            .size(TEXT_BODY)
            .style(input_style)
            .padding([4, 8])
            .width(Length::Fill),
        Message::ShowTextFieldContextMenu(
            FieldTarget::SidebarSearch,
            app.sidebar.search_query.clone(),
        ),
    )]
    .spacing(SPACE_XS)
    .align_y(Alignment::Center);

    if !app.sidebar.search_query.is_empty() {
        search_row = search_row.push(
            button(text("✕").size(TEXT_BODY))
                .on_press(Message::SidebarSearchChanged(String::new()))
                .style(icon_button)
                .padding([2, 6]),
        );
    }

    search_row.into()
}

pub fn render_sidebar(app: &Rustrest) -> Element<'_, Message> {
    let mut sidebar_contents = column![].spacing(SPACE_MD);
    let query = search_query(app);
    let query = query.as_deref();

    if app.status_bar.is_active("startup-workspace") {
        sidebar_contents =
            sidebar_contents.push(spinner_with_label(app.spinner_tick, "Loading workspace..."));
    } else if app.collections.is_empty() {
        sidebar_contents =
            sidebar_contents.push(text("No collections imported yet.").size(11).style(
                |theme: &iced::Theme| text::Style {
                    color: Some(muted_text_color(theme)),
                },
            ));
    } else {
        let mut any_match = false;
        for col in &app.collections {
            let Some(filter) = collection_filter(col, query) else {
                continue;
            };
            any_match = true;
            let col_id = col.id;
            let is_editing_col = app.sidebar.editing_collection_id == Some(col_id);
            // a search expands everything it shows
            let is_collapsed_col =
                query.is_none() && app.sidebar.collapsed_collections.contains(&col_id);

            let collection_header_title: Element<'_, Message> = if is_editing_col {
                row![
                    with_context_menu(
                        text_input("Collection Name...", &col.info.name)
                            .on_input(move |txt| Message::CollectionNameChanged(col_id, txt))
                            .on_submit(Message::SaveCollectionNamePressed(col_id))
                            .size(TEXT_BASE)
                            .style(input_style)
                            .width(Length::Fixed(120.0))
                            .padding(2),
                        Message::ShowTextFieldContextMenu(
                            FieldTarget::CollectionName(col_id),
                            col.info.name.clone(),
                        ),
                    ),
                    button(text("Save").size(11))
                        .on_press(Message::SaveCollectionNamePressed(col_id))
                        .style(button::text)
                ]
                .spacing(5)
                .align_y(Alignment::Center)
                .into()
            } else {
                let remote_profile_id = col.remote_dir.as_ref().map(|r| r.profile_id);
                let is_online = remote_profile_id
                    .map(|id| app.remote.remote_sessions.contains_key(&id))
                    .unwrap_or(true);

                let collapse_arrow =
                    button(text(if is_collapsed_col { "▶" } else { "▼" }).size(TEXT_CAPTION))
                        .on_press(Message::ToggleCollectionCollapsed(col_id))
                        .style(icon_button)
                        .padding(2);

                let name_color = if is_online {
                    None
                } else {
                    Some(crate::theme::colors().text_muted)
                };

                let mut header_row = row![
                    collapse_arrow,
                    text(&col.info.name)
                        .font(semibold())
                        .size(TEXT_BASE)
                        .style(move |_theme: &iced::Theme| text::Style { color: name_color }),
                ]
                .spacing(SPACE_XS)
                .align_y(Alignment::Center);

                if collection_is_unsaved(app, col) {
                    header_row = header_row.push(unsaved_dot());
                }

                if col.remote_dir.is_none() && app.is_git_backed(col_id) {
                    let has_changes = app
                        .git
                        .git_status_cache
                        .get(&col_id)
                        .map(|r| matches!(r, Ok(s) if !s.files.is_empty()))
                        .unwrap_or(false);

                    header_row = header_row.push(
                        text("git")
                            .size(10)
                            .font(Font::MONOSPACE)
                            .color(crate::theme::colors().text_muted),
                    );
                    if has_changes {
                        header_row = header_row
                            .push(text("●").size(9).color(crate::theme::colors().modified));
                    }
                }

                if app.cloud.linked.contains_key(&col_id) {
                    header_row = header_row.push(cloud_badge(app, col_id));
                }

                if let Some(profile_id) = remote_profile_id {
                    header_row = header_row.push(
                        text("ssh")
                            .size(10)
                            .font(Font::MONOSPACE)
                            .color(crate::theme::colors().text_muted),
                    );
                    if !is_online {
                        header_row = header_row.push(Space::new().width(Length::Fill));
                        header_row = header_row.push(
                            button(text("Connect").size(10))
                                .on_press(Message::RemoteConnectPressed(profile_id))
                                .padding([1, 6])
                                .style(button::secondary),
                        );
                    }
                }

                let collection_key = SidebarItemKey::Collection(col_id);
                let is_row_selected = app.sidebar.selected_sidebar_items.contains(&collection_key);
                let is_row_hovered =
                    app.sidebar.hovered_sidebar_item.as_ref() == Some(&collection_key);

                mouse_area(
                    container(header_row)
                        .padding([4, 6])
                        .style(list_row(is_row_selected, is_row_hovered)),
                )
                .on_enter(Message::SidebarItemHovered(Some(collection_key.clone())))
                .on_exit(Message::SidebarItemHovered(None))
                .on_press(sidebar_click_message(
                    app,
                    collection_key,
                    Message::SidebarCollectionRootClicked(col_id),
                ))
                .on_right_press(Message::ShowCollectionContextMenu(col_id))
                .on_release(Message::SidebarDropped(SidebarDropTarget::CollectionRoot(
                    col_id,
                )))
                .into()
            };

            let mut col_tree = column![collection_header_title].spacing(SPACE_XS);

            if !is_collapsed_col {
                for item in &col.item {
                    col_tree =
                        render_sidebar_item(app, col_tree, item, col_id, Vec::new(), query, filter);
                }
            }
            sidebar_contents = sidebar_contents.push(col_tree);
        }

        if !any_match {
            sidebar_contents = sidebar_contents.push(
                text(format!(
                    "No results for \"{}\"",
                    app.sidebar.search_query.trim()
                ))
                .size(11)
                .style(|theme: &iced::Theme| text::Style {
                    color: Some(muted_text_color(theme)),
                }),
            );
        }
    }

    sidebar_contents = sidebar_contents.push(render_plugins_section(app));

    let content: Element<'_, Message> = if app.collections.is_empty() {
        scrollable(sidebar_contents).into()
    } else {
        column![render_search_box(app), scrollable(sidebar_contents)]
            .spacing(SPACE_SM)
            .into()
    };

    container(content)
        .width(Length::Fixed(app.layout.sidebar_width))
        .height(Length::Fill)
        .padding(SPACE_MD)
        .style(card_surface)
        .into()
}

/// "cloud" tag on a synced collection, with a dot showing its sync state
fn cloud_badge(app: &Rustrest, col_id: usize) -> Element<'_, Message> {
    use crate::app::cloud::CloudStatus;
    let colors = crate::theme::colors();
    let conflicts = app.cloud.conflict_count(col_id);

    let (dot, hint) = match app.cloud.status() {
        CloudStatus::SignedOut | CloudStatus::Expired => (
            colors.error,
            "Not signed in to Cloud, changes aren't syncing".to_string(),
        ),
        CloudStatus::Offline => (
            colors.text_muted,
            "Offline, changes sync once Cloud is reachable".to_string(),
        ),
        _ if app.cloud.is_syncing(col_id) => (colors.info, "Syncing...".to_string()),
        _ if conflicts > 0 => (
            colors.conflict,
            format!("{conflicts} change(s) conflict with the cloud"),
        ),
        _ if app.cloud.waiting.contains(&col_id) => {
            (colors.modified, "Save to sync your changes".to_string())
        }
        CloudStatus::Connecting => (colors.warning, "Connecting to Cloud...".to_string()),
        CloudStatus::Online => (colors.success, "Synced with Rustrest Cloud".to_string()),
    };

    let badge = row![
        text("cloud")
            .size(10)
            .font(Font::MONOSPACE)
            .color(colors.text_muted),
        text("●").size(8).color(dot),
    ]
    .spacing(3)
    .align_y(Alignment::Center);
    crate::ui::tooltip::with_tooltip(badge, hint, iced::widget::tooltip::Position::Bottom)
}

fn render_plugins_section(app: &Rustrest) -> Element<'_, Message> {
    let mut section = column![
        row![
            text("PLUGINS")
                .size(TEXT_CAPTION)
                .style(|theme: &iced::Theme| text::Style {
                    color: Some(muted_text_color(theme)),
                }),
            Space::new().width(Length::Fill),
            button(text("Manage").size(TEXT_SMALL))
                .on_press(Message::OpenPluginManagerPressed)
                .style(button::text)
                .padding([2, 4]),
        ]
        .align_y(Alignment::Center)
    ]
    .spacing(SPACE_XS);

    for plugin in app.plugins.plugin_manager.installed() {
        if !plugin.is_active() {
            continue;
        }
        let Some(manifest) = &plugin.manifest else {
            continue;
        };
        for cap in &manifest.capabilities {
            if let rustrest_plugin_host::Capability::SidebarPanel(panel) = cap {
                let plugin_id = plugin.id().to_string();
                let panel_id = panel.id.clone();
                section = section.push(
                    button(text(panel.title.clone()).size(12))
                        .on_press(Message::OpenPluginPanel(plugin_id, panel_id))
                        .style(button::text)
                        .padding(Padding::from([2, 4]))
                        .width(Length::Fill),
                );
            }
        }
    }

    section.into()
}

/// an environment as a pick-list entry. picked by index, since a cloud
/// environment can share its name with a local one
#[derive(Debug, Clone, PartialEq, Eq)]
struct EnvChoice {
    idx: usize,
    label: String,
}

impl std::fmt::Display for EnvChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

pub fn render_env_selector(app: &Rustrest) -> Element<'_, Message> {
    let env_options: Vec<EnvChoice> = app
        .env
        .environments
        .iter()
        .enumerate()
        .map(|(idx, e)| EnvChoice {
            idx,
            label: if e.cloud.is_some() {
                format!("{} (cloud)", e.name)
            } else {
                e.name.clone()
            },
        })
        .collect();
    let current_env_selection = app
        .env
        .active_env_index
        .and_then(|idx| env_options.get(idx).cloned());

    // build environment selector row with controls
    let mut env_row = row![
        pick_list(env_options, current_env_selection, |selected| {
            Message::EnvSelected(Some(selected.idx))
        })
        .placeholder("No Environment")
        .width(Length::Fixed(150.0))
        .padding([4, 8]),
        // add Environment button
        button(text("+").size(TEXT_BASE))
            .on_press(Message::CreateEnvironmentPressed)
            .padding([4, 8])
            .style(secondary_button)
    ]
    .spacing(SPACE_SM)
    .align_y(Alignment::Center);

    // show Edit and Delete buttons if an active environment is selected
    if let Some(active_idx) = app.env.active_env_index {
        env_row = env_row
            .push(
                button(text("⛭").size(TEXT_BASE))
                    .on_press(Message::EditEnvironmentPressed(active_idx))
                    .padding([4, 8])
                    .style(secondary_button),
            )
            .push(
                button(text("✕").size(TEXT_BODY))
                    .on_press(Message::DeleteEnvironmentPressed(active_idx))
                    .padding([4, 8])
                    .style(secondary_button),
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
    let active_id = app.workspace.active_workspace_id;
    let active_name = app
        .workspace
        .workspaces
        .iter()
        .find(|w| w.id == active_id)
        .map(|w| w.name.clone());

    let is_editing = app.workspace.editing_workspace_id == Some(active_id);

    let content: Element<'_, Message> = if is_editing {
        let current_name = active_name.clone().unwrap_or_default();
        row![
            with_context_menu(
                text_input("Workspace Name...", &current_name)
                    .on_input(move |txt| Message::WorkspaceNameChanged(active_id, txt))
                    .on_submit(Message::SaveWorkspaceNamePressed(active_id))
                    .size(TEXT_BASE)
                    .style(input_style)
                    .width(Length::Fixed(140.0))
                    .padding([4, 8]),
                Message::ShowTextFieldContextMenu(
                    FieldTarget::WorkspaceName(active_id),
                    current_name.clone(),
                ),
            ),
            button(text("Save").size(TEXT_BASE))
                .on_press(Message::SaveWorkspaceNamePressed(active_id))
                .padding([4, 12])
                .style(primary_button)
        ]
        .spacing(SPACE_SM)
        .align_y(Alignment::Center)
        .into()
    } else {
        let workspace_options: Vec<String> = app
            .workspace
            .workspaces
            .iter()
            .map(|w| w.name.clone())
            .collect();

        let mut ws_row = row![
            pick_list(workspace_options, active_name, |selected| {
                Message::WorkspaceSelected(selected)
            })
            .placeholder("Workspace")
            .width(Length::Fixed(140.0))
            .padding([4, 8]),
            button(text("+").size(TEXT_BASE))
                .on_press(Message::CreateWorkspacePressed)
                .padding([4, 8])
                .style(secondary_button),
            button(text("✎").size(TEXT_BODY))
                .on_press(Message::RenameWorkspacePressed(active_id))
                .padding([4, 8])
                .style(secondary_button),
        ]
        .spacing(SPACE_SM)
        .align_y(Alignment::Center);

        if app.workspace.workspaces.len() > 1 {
            ws_row = ws_row.push(
                button(text("✕").size(TEXT_BODY))
                    .on_press(Message::DeleteWorkspacePressed(active_id))
                    .padding([4, 8])
                    .style(secondary_button),
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
    query: Option<&str>,
    filter: Option<&str>,
) -> Column<'a, Message> {
    if !item_visible(item, filter) {
        return layout;
    }
    match item {
        CollectionItem::Folder(folder) => {
            current_path.push(folder.name.clone());

            let path_for_change = current_path.clone();
            let path_for_save = current_path.clone();
            let path_for_right_click = current_path.clone();
            let path_for_toggle = current_path.clone();
            let path_for_drag = current_path.clone();
            let path_for_drop = current_path.clone();

            let is_editing_folder = app.sidebar.editing_folder_collection_id == Some(collection_id)
                && app.sidebar.editing_folder_path == current_path;
            // a search expands everything it shows
            let is_collapsed = query.is_none()
                && app
                    .sidebar
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
                            .size(TEXT_BASE)
                            .style(input_style)
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
                    button(text("Save").size(10))
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
                let collapse_arrow =
                    button(text(if is_collapsed { "▶" } else { "▼" }).size(TEXT_CAPTION))
                        .on_press(Message::ToggleFolderCollapsed {
                            collection_id,
                            folder_path: path_for_toggle,
                        })
                        .style(button::text)
                        .padding(2);

                let mut title_row = row![collapse_arrow, text(&folder.name).size(TEXT_BASE)]
                    .spacing(SPACE_XS)
                    .align_y(Alignment::Center);

                if folder_is_unsaved(app, &folder.item) {
                    title_row = title_row.push(unsaved_dot());
                }

                let folder_key = SidebarItemKey::Folder {
                    collection_id,
                    path: current_path.clone(),
                };
                let is_row_selected = app.sidebar.selected_sidebar_items.contains(&folder_key);
                let is_row_hovered = app.sidebar.hovered_sidebar_item.as_ref() == Some(&folder_key);

                mouse_area(
                    container(title_row)
                        .padding([3, 6])
                        .style(list_row(is_row_selected, is_row_hovered)),
                )
                .on_enter(Message::SidebarItemHovered(Some(folder_key.clone())))
                .on_exit(Message::SidebarItemHovered(None))
                .on_press(sidebar_click_message(
                    app,
                    folder_key,
                    Message::SidebarFolderClicked {
                        collection_id,
                        folder_path: path_for_drag,
                    },
                ))
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
                let filter = child_filter(&folder.name, filter);
                for sub in &folder.item {
                    folder_layout = render_sidebar_item(
                        app,
                        folder_layout,
                        sub,
                        collection_id,
                        current_path.clone(),
                        query,
                        filter,
                    );
                }
            }
            layout.push(folder_layout)
        }
        CollectionItem::Request(req_node) => {
            let req_clone = req_node.clone();
            let path_for_right_click = current_path.clone();
            let path_for_drag = current_path.clone();
            let path_for_drop = current_path.clone();
            let req_id = req_node.id;

            let is_editing_request = app.sidebar.editing_request_collection_id
                == Some(collection_id)
                && app.sidebar.editing_request_id == Some(req_id);
            let has_saved_responses = req_node
                .response
                .as_ref()
                .map(|examples| !examples.is_empty())
                .unwrap_or(false);
            let is_responses_collapsed = app.sidebar.collapsed_saved_responses.contains(&req_id);

            let request_title: Element<'_, Message> = if is_editing_request {
                row![
                    text_input("Request Name...", &req_node.name)
                        .on_input(move |txt| Message::RequestNameChanged {
                            collection_id,
                            request_id: req_id,
                            new_name: txt,
                        })
                        .on_submit(Message::SaveRequestNamePressed {
                            collection_id,
                            request_id: req_id,
                        })
                        .style(input_style)
                        .width(Length::Fixed(120.0))
                        .padding(2)
                        .size(TEXT_BASE),
                    button(text("Save").size(10))
                        .on_press(Message::SaveRequestNamePressed {
                            collection_id,
                            request_id: req_id,
                        })
                        .style(button::text)
                ]
                .spacing(5)
                .align_y(Alignment::Center)
                .padding(Padding {
                    top: 2.0,
                    right: 0.0,
                    bottom: 2.0,
                    left: 15.0,
                })
                .into()
            } else {
                let mut label_row = row![].spacing(SPACE_XS).align_y(Alignment::Center);
                if has_saved_responses {
                    label_row = label_row.push(
                        button(
                            text(if is_responses_collapsed { "▶" } else { "▼" }).size(TEXT_CAPTION),
                        )
                        .on_press(Message::ToggleSavedResponsesCollapsed(req_id))
                        .style(button::text)
                        .padding(1),
                    );
                }
                label_row = label_row.push(crate::ui::badge::method_badge(
                    &req_node.request.method,
                    Some(36.0),
                ));
                label_row = label_row.push(text(&req_node.name).size(TEXT_BASE));
                if request_is_unsaved(app, req_node) {
                    label_row = label_row.push(unsaved_dot());
                }

                let request_key = SidebarItemKey::Request {
                    collection_id,
                    parent_path: current_path.clone(),
                    request_id: req_id,
                };
                let is_row_selected = app.sidebar.selected_sidebar_items.contains(&request_key);
                let is_row_hovered =
                    app.sidebar.hovered_sidebar_item.as_ref() == Some(&request_key);

                mouse_area(
                    container(label_row)
                        .padding(Padding {
                            top: 3.0,
                            right: 6.0,
                            bottom: 3.0,
                            left: if has_saved_responses { 8.0 } else { 15.0 },
                        })
                        .style(list_row(is_row_selected, is_row_hovered)),
                )
                .on_enter(Message::SidebarItemHovered(Some(request_key.clone())))
                .on_exit(Message::SidebarItemHovered(None))
                .on_press(sidebar_click_message(
                    app,
                    request_key,
                    Message::SidebarRequestClicked {
                        req_node: req_clone,
                        collection_id,
                        parent_path: path_for_drag,
                    },
                ))
                .on_right_press(Message::ShowRequestContextMenu {
                    collection_id,
                    folder_path: path_for_right_click,
                    request_id: req_id,
                })
                .on_release(Message::SidebarDropped(SidebarDropTarget::Request {
                    collection_id,
                    parent_path: path_for_drop,
                    request_id: req_id,
                }))
                .into()
            };

            let mut req_layout = column![request_title].spacing(2);

            if !is_responses_collapsed {
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
    let is_editing = app.sidebar.editing_saved_response == Some((collection_id, request_id, index));

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
                .style(input_style)
                .width(Length::Fixed(120.0))
                .padding(2)
                .size(TEXT_BODY),
            button(text("Save").size(10))
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
            text("•")
                .size(TEXT_SMALL)
                .color(crate::theme::colors().text_muted),
            text(example.name.clone())
                .size(TEXT_BODY)
                .color(crate::theme::colors().text_muted),
        ]
        .spacing(SPACE_XS)
        .align_y(Alignment::Center);

        mouse_area(container(title_row).padding(Padding {
            top: 3.0,
            right: 6.0,
            bottom: 3.0,
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
