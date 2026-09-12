use crate::app::Rustrest;
use crate::http_client::HttpMethod;
use crate::message::Message;
use crate::ui::tab::messages::{TabMessage, ValueField};
use crate::ui::tab::types::{FormDataRow, KeyValuePair};
use iced::widget::text_editor::{Action, Edit};
use iced::widget::{button, column, container, mouse_area, opaque, row, text};
use iced::{Element, Length};
use std::sync::Arc;

/// identifies an addressable field that belongs to the currently active tab.
#[derive(Debug, Clone, PartialEq)]
pub enum TabFieldTarget {
    Url,
    CustomMethod,
    Auth,
    HeaderKey(usize),
    HeaderValue(usize),
    ParamKey(usize),
    ParamValue(usize),
    CookieKey(usize),
    CookieValue(usize),
    UrlencodedKey(usize),
    UrlencodedValue(usize),
    FormDataKey(usize),
    FormDataValue(usize),
    BodyEditor,
    PreRequestScriptEditor,
    PostResponseScriptEditor,
    /// read-only; only "Copy" is offered for this target.
    ResponseBodyEditor,
}

/// identifies any addressable text field in the app.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldTarget {
    Tab(TabFieldTarget),
    CollectionName(usize),
    FolderName {
        collection_id: usize,
        folder_path: Vec<String>,
    },
    WorkspaceName(usize),
    TabName(usize),
    EnvName(usize),
    EnvVarKey {
        env_idx: usize,
        var_idx: usize,
    },
    EnvVarValue {
        env_idx: usize,
        var_idx: usize,
    },
    CollectionVarKey {
        collection_id: usize,
        index: usize,
    },
    CollectionVarValue {
        collection_id: usize,
        index: usize,
    },
    SaveRequestName,
    CommitMessage,
}

pub enum ContextMenu {
    Collection(usize),
    Folder {
        col_id: usize,
        path: Vec<String>,
    },
    Request {
        col_id: usize,
        folder_path: Vec<String>,
        req_id: usize,
    },
    SavedResponse {
        col_id: usize,
        req_id: usize,
        index: usize,
    },
    /// a plain text field/editor; `current_value` is captured at the moment
    /// the menu was opened so "Copy" doesn't need to re-look up the field.
    TextField {
        target: FieldTarget,
        current_value: String,
    },
}

/// wraps any widget with a right-click handler that opens the shared context menu.
///
/// this is the single reusable primitive every input field/text editor uses to
/// gain a Copy/Paste context menu
pub fn with_context_menu<'a, Message>(
    content: impl Into<Element<'a, Message>>,
    on_right_click: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    mouse_area(content).on_right_press(on_right_click).into()
}

/// renders the floating Copy/Paste (or CRUD, for sidebar items) dropdown, positioned
/// at `app.context_menu_position`, if a context menu is currently open.
pub fn render_context_menu_overlay<'a>(app: &Rustrest) -> Option<Element<'a, Message>> {
    let context_menu = app.active_context_menu.as_ref()?;

    // suppress the panel while the targeted item is mid-rename, since its
    // row is showing a text input instead of the label the menu anchors to
    let is_editing = match context_menu {
        ContextMenu::Collection(id) => app.editing_collection_id == Some(*id),
        ContextMenu::Folder { col_id, path } => {
            app.editing_folder_collection_id == Some(*col_id) && app.editing_folder_path == *path
        }
        ContextMenu::Request { col_id, req_id, .. } => {
            app.editing_request_collection_id == Some(*col_id)
                && app.editing_request_id == Some(*req_id)
        }
        ContextMenu::SavedResponse {
            col_id,
            req_id,
            index,
        } => app.editing_saved_response == Some((*col_id, *req_id, *index)),
        ContextMenu::TextField { .. } => false,
    };
    if is_editing {
        return None;
    }

    let options: Vec<(&'a str, Message)> = match context_menu {
        ContextMenu::Collection(id) => {
            let col_id = *id;
            let is_git_backed = app
                .collections
                .iter()
                .find(|c| c.id == col_id)
                .map(|c| c.storage_dir.is_some())
                .unwrap_or(false);

            let mut opts = vec![
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
            ];
            if is_git_backed {
                opts.push(("Commit changes...", Message::CommitChangesPressed(col_id)));
            }
            opts.push(("Export As...", Message::ExportCollectionPressed(col_id)));
            opts.push(("Delete", Message::DeleteCollectionPressed(col_id)));
            opts
        }
        ContextMenu::Folder { col_id, path } => {
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
        ContextMenu::Request {
            col_id,
            folder_path,
            req_id,
        } => vec![
            (
                "Rename",
                Message::RenameRequestPressed {
                    collection_id: *col_id,
                    request_id: *req_id,
                },
            ),
            (
                "Delete",
                Message::DeleteRequestPressed {
                    collection_id: *col_id,
                    parent_folder_path: folder_path.clone(),
                    request_id: *req_id,
                },
            ),
        ],
        ContextMenu::SavedResponse {
            col_id,
            req_id,
            index,
        } => vec![
            (
                "Rename",
                Message::RenameSavedResponsePressed {
                    collection_id: *col_id,
                    request_id: *req_id,
                    index: *index,
                },
            ),
            (
                "Delete",
                Message::DeleteSavedResponsePressed {
                    collection_id: *col_id,
                    request_id: *req_id,
                    index: *index,
                },
            ),
        ],
        ContextMenu::TextField {
            target,
            current_value,
        } => {
            let mut opts = vec![("Copy", Message::CopyToClipboard(current_value.clone()))];
            if !matches!(target, FieldTarget::Tab(TabFieldTarget::ResponseBodyEditor)) {
                opts.push(("Paste", Message::PasteIntoField(target.clone())));
            }
            opts
        }
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

/// writes `text` into the field addressed by `target`, once a "Paste" action's
/// async clipboard read resolves. reuses the exact same update paths the
/// field's own `on_input` handler would take, so side effects (e.g. syncing
/// query params back into the URL) stay in one place.
pub fn apply_field_paste(app: &mut Rustrest, target: FieldTarget, text: String) {
    match target {
        FieldTarget::Tab(tab_target) => {
            let Some(tab_state) = app.tabs.get_mut(app.active_tab_index) else {
                return;
            };
            let tab = &mut tab_state.tab;

            match tab_target {
                TabFieldTarget::Url => tab.update(TabMessage::UrlChanged(text)),
                TabFieldTarget::CustomMethod => {
                    tab.update(TabMessage::MethodChanged(HttpMethod::Custom(text)))
                }
                TabFieldTarget::Auth => tab.update(TabMessage::AuthChanged(Action::Edit(
                    Edit::Paste(Arc::new(text)),
                ))),

                TabFieldTarget::HeaderKey(idx) => {
                    if let Some(updated) = kv_key_paste(&tab.request_headers, idx, text) {
                        tab.update(TabMessage::HeaderRowChanged(idx, updated));
                    }
                }
                TabFieldTarget::HeaderValue(idx) => tab.update(TabMessage::ValueEditorAction(
                    ValueField::Header,
                    idx,
                    Action::Edit(Edit::Paste(Arc::new(text))),
                )),
                TabFieldTarget::ParamKey(idx) => {
                    if let Some(updated) = kv_key_paste(&tab.request_params, idx, text) {
                        tab.update(TabMessage::ParamRowChanged(idx, updated));
                    }
                }
                TabFieldTarget::ParamValue(idx) => tab.update(TabMessage::ValueEditorAction(
                    ValueField::Param,
                    idx,
                    Action::Edit(Edit::Paste(Arc::new(text))),
                )),
                TabFieldTarget::CookieKey(idx) => {
                    if let Some(updated) = kv_key_paste(&tab.request_cookies, idx, text) {
                        tab.update(TabMessage::CookieRowChanged(idx, updated));
                    }
                }
                TabFieldTarget::CookieValue(idx) => tab.update(TabMessage::ValueEditorAction(
                    ValueField::Cookie,
                    idx,
                    Action::Edit(Edit::Paste(Arc::new(text))),
                )),
                TabFieldTarget::UrlencodedKey(idx) => {
                    if let Some(updated) = kv_key_paste(&tab.body_urlencoded, idx, text) {
                        tab.update(TabMessage::UrlencodedRowChanged(idx, updated));
                    }
                }
                TabFieldTarget::UrlencodedValue(idx) => tab.update(TabMessage::ValueEditorAction(
                    ValueField::Urlencoded,
                    idx,
                    Action::Edit(Edit::Paste(Arc::new(text))),
                )),
                TabFieldTarget::FormDataKey(idx) => {
                    if let Some(row) = tab.body_form_data.get(idx) {
                        let updated = FormDataRow {
                            is_active: row.is_active,
                            key: text,
                            value: row.value.clone(),
                            field_type: row.field_type,
                        };
                        tab.update(TabMessage::FormDataRowChanged(idx, updated));
                    }
                }
                TabFieldTarget::FormDataValue(idx) => tab.update(TabMessage::ValueEditorAction(
                    ValueField::FormData,
                    idx,
                    Action::Edit(Edit::Paste(Arc::new(text))),
                )),

                TabFieldTarget::BodyEditor => tab.update(TabMessage::BodyChanged(Action::Edit(
                    Edit::Paste(Arc::new(text)),
                ))),
                TabFieldTarget::PreRequestScriptEditor => tab.update(
                    TabMessage::PreRequestScriptChanged(Action::Edit(Edit::Paste(Arc::new(text)))),
                ),
                TabFieldTarget::PostResponseScriptEditor => {
                    tab.update(TabMessage::PostResponseScriptChanged(Action::Edit(
                        Edit::Paste(Arc::new(text)),
                    )))
                }
                TabFieldTarget::ResponseBodyEditor => {
                    // read-only; the menu never offers "Paste" for this target
                }
            }
        }

        FieldTarget::CollectionName(col_id) => {
            let _ = crate::app::update(app, Message::CollectionNameChanged(col_id, text));
        }
        FieldTarget::FolderName {
            collection_id,
            folder_path,
        } => {
            let _ = crate::app::update(
                app,
                Message::FolderNameChanged {
                    collection_id,
                    folder_path,
                    new_name: text,
                },
            );
        }
        FieldTarget::WorkspaceName(id) => {
            let _ = crate::app::update(app, Message::WorkspaceNameChanged(id, text));
        }
        FieldTarget::TabName(idx) => {
            let _ = crate::app::update(app, Message::TabNameChanged(idx, text));
        }
        FieldTarget::SaveRequestName => {
            let _ = crate::app::update(app, Message::SaveRequestNameChanged(text));
        }
        FieldTarget::CommitMessage => {
            let _ = crate::app::update(
                app,
                Message::CommitMessageChanged(Action::Edit(Edit::Paste(Arc::new(text)))),
            );
        }
        FieldTarget::EnvName(idx) => {
            let _ = crate::app::update(app, Message::EnvNameChanged(idx, text));
        }
        FieldTarget::EnvVarKey { env_idx, var_idx } => {
            let _ = crate::app::update(
                app,
                Message::EnvVariableKeyChanged {
                    env_idx,
                    var_idx,
                    key: text,
                },
            );
        }
        FieldTarget::EnvVarValue { env_idx, var_idx } => {
            let _ = crate::app::update(
                app,
                Message::EnvVariableValueChanged {
                    env_idx,
                    var_idx,
                    value: text,
                },
            );
        }
        FieldTarget::CollectionVarKey {
            collection_id,
            index,
        } => {
            let current_value = collection_var_value(app, collection_id, index);
            let _ = crate::app::update(
                app,
                Message::CollectionVariableChanged {
                    collection_id,
                    index,
                    key: text,
                    value: current_value,
                },
            );
        }
        FieldTarget::CollectionVarValue {
            collection_id,
            index,
        } => {
            let current_key = collection_var_key(app, collection_id, index);
            let _ = crate::app::update(
                app,
                Message::CollectionVariableChanged {
                    collection_id,
                    index,
                    key: current_key,
                    value: text,
                },
            );
        }
    }
}

fn collection_var_value(app: &Rustrest, collection_id: usize, index: usize) -> String {
    app.collections
        .iter()
        .find(|c| c.id == collection_id)
        .and_then(|c| c.variable.as_ref())
        .and_then(|vars| vars.get(index))
        .map(|var| match &var.value {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(other) => other.to_string().trim_matches('"').to_string(),
            None => String::new(),
        })
        .unwrap_or_default()
}

fn collection_var_key(app: &Rustrest, collection_id: usize, index: usize) -> String {
    app.collections
        .iter()
        .find(|c| c.id == collection_id)
        .and_then(|c| c.variable.as_ref())
        .and_then(|vars| vars.get(index))
        .map(|var| var.key.clone())
        .unwrap_or_default()
}

fn kv_key_paste(pairs: &[KeyValuePair], idx: usize, text: String) -> Option<KeyValuePair> {
    pairs.get(idx).map(|row| KeyValuePair {
        is_active: row.is_active,
        key: text,
        value: row.value.clone(),
    })
}
