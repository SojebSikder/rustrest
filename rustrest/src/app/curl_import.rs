//! The "Import cURL" modal: parses a pasted cURL command and opens it as a new, unsaved request tab.

use super::{Rustrest, TabState, WorkspaceContent};
use crate::collection::collection::PostmanRequestNode;
use crate::collection_adapter::create_tab_from_request;
use crate::message::Message;
use crate::ui::curl_import_modal::{CurlImportMessage, CurlImportModalState, editor_id};
use crate::ui::tab::types::RawType;
use crate::ui::toast::toast::ToastStatus;
use iced::Task;
use rustrest_core::curl_import;

pub fn open(app: &mut Rustrest) -> Task<Message> {
    app.overlays.curl_import_modal = Some(CurlImportModalState::default());
    iced::widget::operation::focus(editor_id())
}

pub fn update(app: &mut Rustrest, msg: CurlImportMessage) -> Task<Message> {
    let Some(modal) = app.overlays.curl_import_modal.as_mut() else {
        return Task::none();
    };

    match msg {
        CurlImportMessage::EditorAction(action) => {
            if action.is_edit() {
                modal.error = None;
            }
            modal.command.perform(action);
            Task::none()
        }
        CurlImportMessage::Close => {
            app.overlays.curl_import_modal = None;
            Task::none()
        }
        CurlImportMessage::Import => match curl_import::parse(&modal.command.text()) {
            Ok(request) => {
                app.overlays.curl_import_modal = None;
                open_tab(app, request)
            }
            Err(e) => {
                modal.error = Some(e);
                Task::none()
            }
        },
    }
}

fn open_tab(
    app: &mut Rustrest,
    request: crate::collection::collection::PostmanRequestDetails,
) -> Task<Message> {
    let content_type = request
        .header
        .iter()
        .flatten()
        .find(|h| h.key.eq_ignore_ascii_case("Content-Type"))
        .map(|h| h.value.to_ascii_lowercase());
    let has_body = request.body.is_some();

    let node = PostmanRequestNode {
        id: 0,
        uid: None,
        name: curl_import::suggested_name(&request),
        event: None,
        request,
        unsaved: true,
        response: None,
        protocol_request: None,
    };
    let mut tab = create_tab_from_request(app.next_tab_id, &node, None);
    // a standalone tab, not tied to any saved request
    tab.request_id = None;
    if !has_body {
        tab.request_body = iced::widget::text_editor::Content::new();
    }
    tab.raw_type = match content_type.as_deref() {
        Some(ct) if ct.contains("json") => RawType::Json,
        Some(ct) if ct.contains("xml") => RawType::Xml,
        Some(ct) if ct.contains("html") => RawType::Html,
        Some(ct) if ct.contains("javascript") => RawType::JavaScript,
        _ if has_body => RawType::Text,
        _ => tab.raw_type,
    };

    app.tabs.push(TabState {
        tab,
        content: WorkspaceContent::HttpRequest,
        is_editing_name: false,
    });
    app.active_tab_index = app.tabs.len() - 1;
    app.next_tab_id += 1;

    Task::batch([
        iced::widget::operation::snap_to_end(crate::ui::workspace::tab_bar_scroll_id()),
        Task::done(Message::ShowToast(
            "Imported cURL command".to_string(),
            ToastStatus::Success,
        )),
    ])
}
