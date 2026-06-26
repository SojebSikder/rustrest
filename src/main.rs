#![windows_subsystem = "windows"]

mod http_client;
mod tab;

use http_client::{HttpMethod, HttpResponse, send_request};
use tab::{Tab, TabMessage};
use tokio_util::sync::CancellationToken;

use iced::widget::{button, column, container, row, text, text_editor, text_input};
use iced::{Alignment, Element, Length, Size, Task};

const APP_NAME: &str = "Rustrest";
const APP_VERSION: &str = "0.1.0";

pub fn main() -> iced::Result {
    iced::application(init, update, view)
        .title(|app: &Rustrest| format!("{} - API Testing Platform", APP_NAME))
        .window(iced::window::Settings {
            size: Size::new(1100.0, 800.0),
            ..Default::default()
        })
        .run()
}

struct Rustrest {
    tabs: Vec<TabState>,
    active_tab_index: usize,
    next_tab_id: usize,
}

struct TabState {
    tab: Tab,
    is_editing_name: bool,
}

#[derive(Debug, Clone)]
enum Message {
    TabSelected(usize),
    NewTabPressed,
    CloseTabPressed(usize),
    ActiveTabMessage(TabMessage),
    SendPressed,
    ResponseReceived(usize, Result<HttpResponse, String>),
    TabNameDoubleClick(usize),
    TabNameChanged(usize, String),
    TabNameSave(usize),
}

fn init() -> (Rustrest, Task<Message>) {
    (
        Rustrest {
            tabs: vec![TabState {
                tab: Tab::new(1),
                is_editing_name: false,
            }],
            active_tab_index: 0,
            next_tab_id: 2,
        },
        Task::none(),
    )
}

fn update(app: &mut Rustrest, message: Message) -> Task<Message> {
    match message {
        Message::TabSelected(index) => {
            if index < app.tabs.len() {
                app.active_tab_index = index;
            }
            Task::none()
        }
        Message::NewTabPressed => {
            app.tabs.push(TabState {
                tab: Tab::new(app.next_tab_id),
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
                            tab::types::ResponseView::Json => format_json_or_fallback(&resp.body),
                            tab::types::ResponseView::Raw => resp.body.clone(),
                        };
                        tab_state.tab.response_body_editor =
                            text_editor::Content::with_text(&body_text);
                    }
                } else {
                    tab_state.tab.update(tab_msg);
                }
            }
            Task::none()
        }
        Message::SendPressed => {
            if let Some(tab_state) = app.tabs.get_mut(app.active_tab_index) {
                let tab = &mut tab_state.tab;
                if tab.is_loading || tab.url.is_empty() {
                    return Task::none();
                }

                let tab_id = tab.id;

                tab.cancel_token = CancellationToken::new();
                tab.is_loading = true;
                tab.response = None;

                let final_url = tab.url.clone();

                let filtered_headers: Vec<(String, String)> = tab
                    .request_headers
                    .iter()
                    .filter(|kv| kv.is_active && !kv.key.trim().is_empty())
                    .map(|kv| (kv.key.trim().to_string(), kv.value.trim().to_string()))
                    .collect();

                let filtered_cookies: Vec<(String, String)> = tab
                    .request_cookies
                    .iter()
                    .filter(|kv| kv.is_active && !kv.key.trim().is_empty())
                    .map(|kv| (kv.key.trim().to_string(), kv.value.trim().to_string()))
                    .collect();

                let body_type = tab.body_type;
                let body_string = tab.request_body.text();
                let form_data = tab.body_form_data.clone();
                let binary_path = tab.binary_file_path.clone();

                let token = tab.cancel_token.clone();
                let method = tab.method.clone();
                let auth = tab.request_auth.clone();

                return Task::perform(
                    send_request(
                        final_url,
                        method,
                        body_type,
                        body_string,
                        form_data,
                        binary_path,
                        filtered_headers,
                        filtered_cookies,
                        auth,
                        token,
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
                            tab::types::ResponseView::Json => format_json_or_fallback(&resp.body),
                            tab::types::ResponseView::Raw => resp.body.clone(),
                        };
                        tab.response_body_editor = text_editor::Content::with_text(&initial_body);
                    }
                    Err(err_msg) => {
                        tab.response_body_editor = text_editor::Content::with_text(err_msg);
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
                    tab_state.tab.name = "Untitled Request".to_string();
                }
            }
            Task::none()
        }
    }
}

fn view(app: &Rustrest) -> Element<Message> {
    let mut tab_bar = row![].spacing(5).align_y(Alignment::Center);

    for (idx, tab_state) in app.tabs.iter().enumerate() {
        let is_active = idx == app.active_tab_index;
        let tab = &tab_state.tab;

        let method_str = match &tab.method {
            HttpMethod::Custom(custom) if custom.trim().is_empty() => "CUSTOM".to_string(),
            HttpMethod::Custom(custom) => custom.to_uppercase(),
            other => format!("{}", other),
        };
        let method_badge = text(format!("[{}]", method_str)).size(11);

        let tab_content: Element<Message> = if tab_state.is_editing_name {
            text_input("", &tab.name)
                .on_input(move |txt| Message::TabNameChanged(idx, txt))
                .on_submit(Message::TabNameSave(idx))
                .size(13)
                .width(Length::Fixed(120.0))
                .into()
        } else {
            button(text(&tab.name).size(13))
                .on_press(Message::TabNameDoubleClick(idx))
                .style(button::text)
                .padding(0)
                .into()
        };

        let mut tab_button = button(
            row![
                method_badge,
                tab_content,
                button("×")
                    .on_press(Message::CloseTabPressed(idx))
                    .padding(2)
                    .style(button::text)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding(8);

        if !is_active {
            tab_button = tab_button
                .style(button::secondary)
                .on_press(Message::TabSelected(idx));
        }
        tab_bar = tab_bar.push(tab_button);
    }

    let add_tab_btn = button("+")
        .on_press(Message::NewTabPressed)
        .padding(8)
        .style(button::success);

    tab_bar = tab_bar.push(add_tab_btn);

    let current_tab = &app.tabs[app.active_tab_index].tab;
    let tab_view = current_tab.view(Message::ActiveTabMessage, Message::SendPressed);

    container(column![tab_bar, tab_view].spacing(18))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(25)
        .into()
}

fn format_json_or_fallback(raw_body: &str) -> String {
    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(raw_body) {
        serde_json::to_string_pretty(&json_value).unwrap_or_else(|_| raw_body.to_string())
    } else {
        format!(
            "// Invalid JSON (Showing Raw Payload instead):\n\n{}",
            raw_body
        )
    }
}
