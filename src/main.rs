#![windows_subsystem = "windows"]

mod http_client;
mod tab;

use http_client::{HttpMethod, HttpResponse, send_request};
use tab::{Tab, TabMessage};
use tokio_util::sync::CancellationToken;

use iced::widget::{button, column, container, row, text, text_input};
use iced::{Alignment, Application, Command, Element, Length, Settings, Size, Theme};

const APP_NAME: &str = "Rustrest";

pub fn main() -> iced::Result {
    let mut settings = Settings::default();
    settings.window.size = Size::new(1100.0, 800.0);
    Rustrest::run(settings)
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

impl Application for Rustrest {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Message>) {
        (
            Self {
                tabs: vec![TabState {
                    tab: Tab::new(1),
                    is_editing_name: false,
                }],
                active_tab_index: 0,
                next_tab_id: 2,
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        format!("{} - API Testing Platform", APP_NAME)
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::TabSelected(index) => {
                if index < self.tabs.len() {
                    self.active_tab_index = index;
                }
                Command::none()
            }
            Message::NewTabPressed => {
                self.tabs.push(TabState {
                    tab: Tab::new(self.next_tab_id),
                    is_editing_name: false,
                });
                self.active_tab_index = self.tabs.len() - 1;
                self.next_tab_id += 1;
                Command::none()
            }
            Message::CloseTabPressed(index) => {
                if self.tabs.len() > 1 {
                    if let Some(tab_state) = self.tabs.get(index) {
                        if tab_state.tab.is_loading {
                            tab_state.tab.cancel_token.cancel();
                        }
                    }

                    self.tabs.remove(index);
                    if self.active_tab_index >= self.tabs.len() {
                        self.active_tab_index = self.tabs.len() - 1;
                    }
                }
                Command::none()
            }
            Message::ActiveTabMessage(tab_msg) => {
                if let Some(tab_state) = self.tabs.get_mut(self.active_tab_index) {
                    tab_state.tab.update(tab_msg);
                }
                Command::none()
            }
            Message::SendPressed => {
                if let Some(tab_state) = self.tabs.get_mut(self.active_tab_index) {
                    let tab = &mut tab_state.tab;
                    if tab.is_loading || tab.url.is_empty() {
                        return Command::none();
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

                    return Command::perform(
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
                Command::none()
            }
            Message::ResponseReceived(tab_id, res) => {
                if let Some(tab_state) = self.tabs.iter_mut().find(|t| t.tab.id == tab_id) {
                    tab_state.tab.is_loading = false;
                    tab_state.tab.response = Some(res);
                }
                Command::none()
            }
            Message::TabNameDoubleClick(idx) => {
                if let Some(tab_state) = self.tabs.get_mut(idx) {
                    tab_state.is_editing_name = true;
                }
                Command::none()
            }
            Message::TabNameChanged(idx, new_name) => {
                if let Some(tab_state) = self.tabs.get_mut(idx) {
                    tab_state.tab.name = new_name;
                }
                Command::none()
            }
            Message::TabNameSave(idx) => {
                if let Some(tab_state) = self.tabs.get_mut(idx) {
                    tab_state.is_editing_name = false;
                    if tab_state.tab.name.trim().is_empty() {
                        tab_state.tab.name = "Untitled Request".to_string();
                    }
                }
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let mut tab_bar = row![].spacing(5).align_items(Alignment::Center);

        for (idx, tab_state) in self.tabs.iter().enumerate() {
            let is_active = idx == self.active_tab_index;
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
                    .style(iced::theme::Button::Text)
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
                        .style(iced::theme::Button::Text)
                ]
                .spacing(8)
                .align_items(Alignment::Center),
            )
            .padding(8);

            if !is_active {
                tab_button = tab_button
                    .style(iced::theme::Button::Secondary)
                    .on_press(Message::TabSelected(idx));
            }
            tab_bar = tab_bar.push(tab_button);
        }

        let add_tab_btn = button("+")
            .on_press(Message::NewTabPressed)
            .padding(8)
            .style(iced::theme::Button::Positive);
        tab_bar = tab_bar.push(add_tab_btn);

        let current_tab = &self.tabs[self.active_tab_index].tab;
        let tab_view = current_tab.view(Message::ActiveTabMessage, Message::SendPressed);

        container(column![tab_bar, tab_view].spacing(18))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(25)
            .into()
    }
}
