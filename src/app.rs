use crate::collection::{PostmanCollection, create_tab_from_request};
use crate::env::Environment;
use crate::http_client::send_request;
use crate::message::Message;
use crate::tab::{Tab, TabMessage};
use crate::utils::{contains_request_node, format_json_or_fallback};
use iced::Task;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub enum WorkspaceContent {
    HttpRequest,
    CollectionRoot {
        collection_id: usize,
        collection_name: String,
    },
}

pub struct TabState {
    pub tab: Tab,
    pub content: WorkspaceContent,
    pub is_editing_name: bool,
}

pub struct Rustrest {
    pub collections: Vec<PostmanCollection>,
    pub environments: Vec<Environment>,
    pub active_env_index: Option<usize>,
    pub tabs: Vec<TabState>,
    pub active_tab_index: usize,
    pub next_tab_id: usize,
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
        },
        Task::none(),
    )
}

pub fn update(app: &mut Rustrest, message: Message) -> Task<Message> {
    match message {
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
                    },
                    is_editing_name: false,
                });
                app.next_tab_id += 1;
                app.active_tab_index = app.tabs.len() - 1;
            }
            Task::none()
        }

        Message::SidebarRequestClicked(req_node) => {
            let associated_collection_id = app
                .collections
                .iter()
                .find(|c| contains_request_node(&c.item, &req_node.name))
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
    }
}
