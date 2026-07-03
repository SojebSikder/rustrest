use crate::collection::{PostmanCollection, PostmanRequestNode, create_tab_from_request};
use crate::env::Environment;
use crate::http_client::send_request;
use crate::message::Message;
use crate::tab::{Tab, TabMessage};
use crate::utils::{contains_request_node_by_id, format_json_or_fallback};
use iced::Task;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq)]
pub enum CollectionSubTab {
    Variables,
    Documentation,
}

#[derive(Debug, Clone)]
pub enum WorkspaceContent {
    HttpRequest,
    CollectionRoot {
        collection_id: usize,
        collection_name: String,
        active_sub_tab: CollectionSubTab,
    },
}

pub struct TabState {
    pub tab: Tab,
    pub content: WorkspaceContent,
    pub is_editing_name: bool,
}

pub enum ContextMenu {
    Collection(usize),
    Folder { col_id: usize, path: Vec<String> },
    Request { col_id: usize, req_id: usize },
}

pub struct Rustrest {
    pub collections: Vec<PostmanCollection>,
    pub environments: Vec<Environment>,
    pub active_env_index: Option<usize>,
    pub tabs: Vec<TabState>,
    pub active_tab_index: usize,
    pub next_tab_id: usize,
    pub next_request_id: usize,

    // Rename state management tracks
    pub editing_collection_id: Option<usize>,
    pub editing_folder_collection_id: Option<usize>,
    pub editing_folder_path: Vec<String>,
    pub active_context_menu: Option<ContextMenu>,
}

pub fn init() -> (Rustrest, Task<Message>) {
    let mut demo_env = Environment::new("Default");
    if !demo_env.variables.is_empty() {
        demo_env.variables[0].is_active = true;
    }

    (
        Rustrest {
            collections: Vec::new(),
            environments: vec![demo_env],
            active_env_index: None,
            tabs: vec![TabState {
                tab: Tab::new(1),
                content: WorkspaceContent::HttpRequest,
                is_editing_name: false,
            }],
            active_tab_index: 0,
            next_tab_id: 2,
            next_request_id: 1,
            editing_collection_id: None,
            editing_folder_collection_id: None,
            editing_folder_path: Vec::new(),
            active_context_menu: None,
        },
        Task::none(),
    )
}

pub fn update(app: &mut Rustrest, message: Message) -> Task<Message> {
    match message {
        Message::None => Task::none(),
        Message::ImportCollectionPressed => {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Postman Collection", &["json"])
                .pick_file()
            {
                if let Ok(file_content) = std::fs::read_to_string(path) {
                    if let Ok(mut collection) =
                        serde_json::from_str::<PostmanCollection>(&file_content)
                    {
                        collection.id = app.next_tab_id;
                        app.next_tab_id += 1;

                        collection.assign_request_ids(&mut app.next_request_id);

                        app.collections.push(collection);
                    }
                }
            }
            Task::none()
        }

        Message::SidebarCollectionRootClicked(col_id) => {
            let existing_tab_idx = app.tabs.iter().position(|t| {
                if let WorkspaceContent::CollectionRoot { collection_id, .. } = t.content {
                    collection_id == col_id
                } else {
                    false
                }
            });

            if let Some(idx) = existing_tab_idx {
                app.active_tab_index = idx;
            } else if let Some(col) = app.collections.iter().find(|c| c.id == col_id) {
                let mut root_tab = Tab::new(app.next_tab_id);
                root_tab.name = col.info.name.clone();

                app.tabs.push(TabState {
                    tab: root_tab,
                    content: WorkspaceContent::CollectionRoot {
                        collection_id: col_id,
                        collection_name: col.info.name.clone(),
                        active_sub_tab: CollectionSubTab::Variables,
                    },
                    is_editing_name: false,
                });
                app.next_tab_id += 1;
                app.active_tab_index = app.tabs.len() - 1;
            }
            Task::none()
        }

        Message::SidebarRequestClicked(req_node) => {
            let existing_tab_idx = app.tabs.iter().position(|t| {
                t.tab.request_id == Some(req_node.id)
                    && matches!(t.content, WorkspaceContent::HttpRequest)
            });

            if let Some(idx) = existing_tab_idx {
                app.active_tab_index = idx;
            } else {
                let associated_collection_id = app
                    .collections
                    .iter()
                    .find(|c| contains_request_node_by_id(&c.item, req_node.id))
                    .map(|c| c.id);

                let new_tab =
                    create_tab_from_request(app.next_tab_id, &req_node, associated_collection_id);

                app.tabs.push(TabState {
                    tab: new_tab,
                    content: WorkspaceContent::HttpRequest,
                    is_editing_name: false,
                });
                app.next_tab_id += 1;
                app.active_tab_index = app.tabs.len() - 1;
            }
            Task::none()
        }

        Message::TabSelected(index) => {
            if index < app.tabs.len() {
                app.active_tab_index = index;
            }
            Task::none()
        }

        Message::NewTabPressed => {
            app.tabs.push(TabState {
                tab: Tab::new(app.next_tab_id),
                content: WorkspaceContent::HttpRequest,
                is_editing_name: false,
            });
            app.active_tab_index = app.tabs.len() - 1;
            app.next_tab_id += 1;
            Task::none()
        }

        Message::CloseTabPressed(index) => {
            if app.tabs.len() > 1 {
                if let Some(tab_state) = app.tabs.get(index) {
                    if tab_state.tab.is_loading {
                        tab_state.tab.cancel_token.cancel();
                    }
                }
                app.tabs.remove(index);
                if app.active_tab_index >= app.tabs.len() {
                    app.active_tab_index = app.tabs.len() - 1;
                }
            }
            Task::none()
        }

        Message::ActiveTabMessage(tab_msg) => {
            if let Some(tab_state) = app.tabs.get_mut(app.active_tab_index) {
                if let TabMessage::ResponseViewChanged(view) = tab_msg {
                    tab_state.tab.response_view = view;
                    if let Some(Ok(resp)) = &tab_state.tab.response {
                        let body_text = match view {
                            crate::tab::types::ResponseView::Json => {
                                format_json_or_fallback(&resp.body)
                            }
                            crate::tab::types::ResponseView::Raw => resp.body.clone(),
                        };
                        tab_state.tab.response_body_editor =
                            iced::widget::text_editor::Content::with_text(&body_text);
                    }
                } else {
                    tab_state.tab.update(tab_msg);
                }
            }
            Task::none()
        }

        Message::SendPressed => {
            if let Some(tab_state) = app.tabs.get_mut(app.active_tab_index) {
                if let WorkspaceContent::CollectionRoot { .. } = tab_state.content {
                    return Task::none();
                }

                let tab = &mut tab_state.tab;
                if tab.is_loading || tab.url.is_empty() {
                    return Task::none();
                }

                let tab_id = tab.id;
                tab.cancel_token = CancellationToken::new();
                tab.is_loading = true;
                tab.response = None;

                let active_env = app
                    .active_env_index
                    .and_then(|idx| app.environments.get(idx))
                    .cloned();

                let collection_vars = tab
                    .collection_id
                    .and_then(|c_id| app.collections.iter().find(|c| c.id == c_id))
                    .map(|c| c.get_native_variables());

                let (
                    final_url,
                    compiled_body,
                    compiled_form_data,
                    filtered_headers,
                    filtered_cookies,
                    compiled_auth,
                ) = tab.compile_request_fields(&active_env, collection_vars.as_deref());

                return Task::perform(
                    send_request(
                        final_url,
                        tab.method.clone(),
                        tab.body_type,
                        compiled_body,
                        compiled_form_data,
                        tab.binary_file_path.clone(),
                        filtered_headers,
                        filtered_cookies,
                        compiled_auth,
                        tab.cancel_token.clone(),
                    ),
                    move |res| Message::ResponseReceived(tab_id, res),
                );
            }
            Task::none()
        }

        Message::ResponseReceived(tab_id, res) => {
            if let Some(tab_state) = app.tabs.iter_mut().find(|t| t.tab.id == tab_id) {
                let tab = &mut tab_state.tab;
                tab.is_loading = false;
                match &res {
                    Ok(resp) => {
                        let initial_body = match tab.response_view {
                            crate::tab::types::ResponseView::Json => {
                                format_json_or_fallback(&resp.body)
                            }
                            crate::tab::types::ResponseView::Raw => resp.body.clone(),
                        };
                        tab.response_body_editor =
                            iced::widget::text_editor::Content::with_text(&initial_body);
                    }
                    Err(err_msg) => {
                        tab.response_body_editor =
                            iced::widget::text_editor::Content::with_text(err_msg);
                    }
                }
                tab.response = Some(res);
            }
            Task::none()
        }

        Message::TabNameDoubleClick(idx) => {
            if let Some(tab_state) = app.tabs.get_mut(idx) {
                tab_state.is_editing_name = true;
            }
            Task::none()
        }

        Message::TabNameChanged(idx, new_name) => {
            if let Some(tab_state) = app.tabs.get_mut(idx) {
                tab_state.tab.name = new_name;
            }
            Task::none()
        }

        Message::TabNameSave(idx) => {
            if let Some(tab_state) = app.tabs.get_mut(idx) {
                tab_state.is_editing_name = false;
                if tab_state.tab.name.trim().is_empty() {
                    tab_state.tab.name = match &tab_state.content {
                        WorkspaceContent::HttpRequest => "Untitled Request".to_string(),
                        WorkspaceContent::CollectionRoot {
                            collection_name, ..
                        } => collection_name.clone(),
                    };
                }
            }
            Task::none()
        }

        Message::EnvSelected(selected_name) => {
            if let Some(name) = selected_name {
                app.active_env_index = app.environments.iter().position(|e| e.name == name);
            } else {
                app.active_env_index = None;
            }
            Task::none()
        }

        Message::CollectionSubTabSelected(sub_tab) => {
            if let Some(tab_state) = app.tabs.get_mut(app.active_tab_index) {
                if let WorkspaceContent::CollectionRoot {
                    ref mut active_sub_tab,
                    ..
                } = tab_state.content
                {
                    *active_sub_tab = sub_tab;
                }
            }
            Task::none()
        }

        Message::CollectionVariableChanged {
            collection_id,
            index,
            key,
            value,
        } => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                let vars = col.variable.get_or_insert_with(Vec::new);
                if let Some(var) = vars.get_mut(index) {
                    var.key = key;
                    var.value = Some(serde_json::Value::String(value));
                }
            }
            Task::none()
        }

        Message::CollectionVariableToggled {
            collection_id,
            index,
            is_active,
        } => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                if let Some(ref mut vars) = col.variable {
                    if let Some(var) = vars.get_mut(index) {
                        var.r#type = Some(if is_active {
                            "string".to_string()
                        } else {
                            "disabled".to_string()
                        });
                    }
                }
            }
            Task::none()
        }

        Message::AddCollectionVariablePressed(collection_id) => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                let vars = col.variable.get_or_insert_with(Vec::new);
                vars.push(crate::collection::PostmanVariable {
                    key: String::new(),
                    value: Some(serde_json::Value::String(String::new())),
                    r#type: Some("string".to_string()),
                });
            }
            Task::none()
        }

        Message::DeleteCollectionVariablePressed(collection_id, index) => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                if let Some(ref mut vars) = col.variable {
                    if index < vars.len() {
                        vars.remove(index);
                    }
                }
            }
            Task::none()
        }

        Message::CreateNewCollectionPressed => {
            let col_id = app.next_tab_id;
            app.next_tab_id += 1;

            let new_col = crate::collection::PostmanCollection {
                id: col_id,
                info: crate::collection::CollectionInfo {
                    name: format!("New Collection {}", col_id),
                    postman_id: None,
                    schema: "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
                        .to_string(),
                },
                item: Vec::new(),
                variable: Some(Vec::new()),
            };
            app.collections.push(new_col);
            Task::none()
        }

        Message::DeleteCollectionPressed(col_id) => {
            app.collections.retain(|c| c.id != col_id);
            app.tabs.retain(|t| {
                if let WorkspaceContent::CollectionRoot { collection_id, .. } = t.content {
                    collection_id != col_id
                } else {
                    true
                }
            });
            if app.active_tab_index >= app.tabs.len() && !app.tabs.is_empty() {
                app.active_tab_index = app.tabs.len() - 1;
            }
            Task::none()
        }

        Message::RenameCollectionPressed(col_id) => {
            app.editing_collection_id = Some(col_id);
            Task::none()
        }

        Message::CollectionNameChanged(col_id, new_name) => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == col_id) {
                col.info.name = new_name.clone();

                // update associated workspace tabs showing this collection's root
                for t in &mut app.tabs {
                    if let WorkspaceContent::CollectionRoot {
                        collection_id,
                        ref mut collection_name,
                        ..
                    } = t.content
                    {
                        if collection_id == col_id {
                            *collection_name = new_name.clone();
                            t.tab.name = new_name.clone();
                        }
                    }
                }
            }
            Task::none()
        }

        Message::SaveCollectionNamePressed(_col_id) => {
            app.editing_collection_id = None;
            Task::none()
        }

        Message::RenameFolderPressed {
            collection_id,
            folder_path,
        } => {
            app.editing_folder_collection_id = Some(collection_id);
            app.editing_folder_path = folder_path;
            Task::none()
        }

        Message::FolderNameChanged {
            collection_id,
            folder_path,
            new_name,
        } => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                fn rename_nested_folder(
                    items: &mut Vec<crate::collection::CollectionItem>,
                    path: &[String],
                    new_val: &str,
                ) -> bool {
                    if path.is_empty() {
                        return false;
                    }
                    for item in items.iter_mut() {
                        if let crate::collection::CollectionItem::Folder {
                            name,
                            item: sub_items,
                        } = item
                        {
                            if name == &path[0] {
                                if path.len() == 1 {
                                    *name = new_val.to_string();
                                    return true;
                                } else {
                                    return rename_nested_folder(sub_items, &path[1..], new_val);
                                }
                            }
                        }
                    }
                    false
                }

                if rename_nested_folder(&mut col.item, &folder_path, &new_name) {
                    // update our navigation path to track the new name dynamically
                    if let Some(last) = app.editing_folder_path.last_mut() {
                        *last = new_name;
                    }
                }
            }
            Task::none()
        }

        Message::SaveFolderNamePressed { .. } => {
            app.editing_folder_collection_id = None;
            app.editing_folder_path.clear();
            Task::none()
        }

        Message::AddFolderPressed {
            collection_id,
            parent_folder_path,
        } => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                fn insert_nested(
                    items: &mut Vec<crate::collection::CollectionItem>,
                    path: &[String],
                ) {
                    if path.is_empty() {
                        items.push(crate::collection::CollectionItem::Folder {
                            name: "New Folder".to_string(),
                            item: Vec::new(),
                        });
                        return;
                    }
                    for item in items.iter_mut() {
                        if let crate::collection::CollectionItem::Folder {
                            name,
                            item: sub_items,
                        } = item
                        {
                            if name == &path[0] {
                                insert_nested(sub_items, &path[1..]);
                                return;
                            }
                        }
                    }
                }
                insert_nested(&mut col.item, &parent_folder_path);
            }
            Task::none()
        }

        Message::DeleteFolderPressed {
            collection_id,
            folder_path,
        } => {
            if !folder_path.is_empty() {
                if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                    fn remove_nested(
                        items: &mut Vec<crate::collection::CollectionItem>,
                        path: &[String],
                    ) {
                        if path.is_empty() {
                            return;
                        }

                        if path.len() == 1 {
                            items.retain(|item| {
                                if let crate::collection::CollectionItem::Folder { name, .. } = item
                                {
                                    name != &path[0]
                                } else {
                                    true
                                }
                            });
                            return;
                        }

                        for item in items.iter_mut() {
                            if let crate::collection::CollectionItem::Folder {
                                name,
                                item: sub_items,
                            } = item
                            {
                                if name == &path[0] {
                                    remove_nested(sub_items, &path[1..]);
                                    return;
                                }
                            }
                        }
                    }
                    remove_nested(&mut col.item, &folder_path);
                }
            }
            Task::none()
        }

        Message::AddRequestPressed {
            collection_id,
            parent_folder_path,
        } => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                let req_id = app.next_request_id;
                app.next_request_id += 1;

                let new_request_node = PostmanRequestNode {
                    id: req_id,
                    name: "Untitled Request".to_string(),
                    request: crate::collection::PostmanRequestDetails {
                        method: "GET".to_string(),
                        url: crate::collection::PostmanUrl::String(String::new()),
                        header: None,
                        body: None,
                    },
                };

                fn insert_nested_request(
                    items: &mut Vec<crate::collection::CollectionItem>,
                    path: &[String],
                    new_req: crate::collection::CollectionItem,
                ) {
                    if path.is_empty() {
                        items.push(new_req);
                        return;
                    }
                    for item in items.iter_mut() {
                        if let crate::collection::CollectionItem::Folder {
                            name,
                            item: sub_items,
                        } = item
                        {
                            if name == &path[0] {
                                insert_nested_request(sub_items, &path[1..], new_req);
                                return;
                            }
                        }
                    }
                }

                insert_nested_request(
                    &mut col.item,
                    &parent_folder_path,
                    crate::collection::CollectionItem::Request(new_request_node),
                );
            }
            Task::none()
        }

        Message::DeleteRequestPressed {
            collection_id,
            parent_folder_path,
            request_id,
        } => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                fn remove_nested_request(
                    items: &mut Vec<crate::collection::CollectionItem>,
                    path: &[String],
                    req_id: usize,
                ) {
                    if path.is_empty() {
                        items.retain(|item| {
                            if let crate::collection::CollectionItem::Request(req) = item {
                                req.id != req_id
                            } else {
                                true
                            }
                        });
                        return;
                    }
                    for item in items.iter_mut() {
                        if let crate::collection::CollectionItem::Folder {
                            name,
                            item: sub_items,
                        } = item
                        {
                            if name == &path[0] {
                                remove_nested_request(sub_items, &path[1..], req_id);
                                return;
                            }
                        }
                    }
                }
                remove_nested_request(&mut col.item, &parent_folder_path, request_id);

                app.tabs.retain(|t| t.tab.request_id != Some(request_id));
                if app.active_tab_index >= app.tabs.len() && !app.tabs.is_empty() {
                    app.active_tab_index = app.tabs.len() - 1;
                }
            }
            Task::none()
        }

        // context menu
        Message::ShowCollectionContextMenu(col_id) => {
            app.active_context_menu = Some(ContextMenu::Collection(col_id));
            Task::none()
        }

        Message::ShowFolderContextMenu {
            collection_id,
            folder_path,
        } => {
            app.active_context_menu = Some(ContextMenu::Folder {
                col_id: collection_id,
                path: folder_path,
            });
            Task::none()
        }

        Message::ShowRequestContextMenu {
            collection_id,
            request_id,
        } => {
            app.active_context_menu = Some(ContextMenu::Request {
                col_id: collection_id,
                req_id: request_id,
            });
            Task::none()
        }

        Message::CloseContextMenu => {
            app.active_context_menu = None;
            Task::none()
        }
    }
}
