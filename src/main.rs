#![windows_subsystem = "windows"]

mod http_client;
mod tab;

use http_client::{HttpResponse, send_request};
use tab::{Tab, TabMessage};
use tokio_util::sync::CancellationToken;

use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Application, Command, Element, Length, Settings, Size, Theme};

const APP_NAME: &str = "Rustrest";

pub fn main() -> iced::Result {
    let mut settings = Settings::default();
    settings.window.size = Size::new(1100.0, 800.0);
    Rustrest::run(settings)
}

struct Rustrest {
    tabs: Vec<Tab>,
    active_tab_index: usize,
    next_tab_id: usize,
}

#[derive(Debug, Clone)]
enum Message {
    TabSelected(usize),
    NewTabPressed,
    CloseTabPressed(usize),
    ActiveTabMessage(TabMessage),
    SendPressed,
    ResponseReceived(usize, Result<HttpResponse, String>),
}

impl Application for Rustrest {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Message>) {
        (
            Self {
                tabs: vec![Tab::new(1)],
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
                self.tabs.push(Tab::new(self.next_tab_id));
                self.active_tab_index = self.tabs.len() - 1;
                self.next_tab_id += 1;
                Command::none()
            }
            Message::CloseTabPressed(index) => {
                if self.tabs.len() > 1 {
                    if let Some(tab) = self.tabs.get(index) {
                        if tab.is_loading {
                            tab.cancel_token.cancel();
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
                if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
                    tab.update(tab_msg);
                }
                Command::none()
            }
            Message::SendPressed => {
                let tab_idx = self.active_tab_index;
                if let Some(tab) = self.tabs.get_mut(tab_idx) {
                    if tab.is_loading || tab.url.is_empty() {
                        return Command::none();
                    }

                    // reset and generate a fresh cancellation token for this execution run
                    tab.cancel_token = CancellationToken::new();
                    tab.is_loading = true;
                    tab.response = None;

                    // construct dynamic URL by filtering and appending only selected query parameters
                    let mut final_url = tab.url.clone();
                    let active_params: Vec<String> = tab
                        .request_params
                        .iter()
                        .filter(|kv| kv.is_active && !kv.key.trim().is_empty())
                        .map(|kv| {
                            format!(
                                "{}={}",
                                urlencoding::encode(kv.key.trim()),
                                urlencoding::encode(kv.value.trim())
                            )
                        })
                        .collect();

                    if !active_params.is_empty() {
                        let query_string = active_params.join("&");
                        if final_url.contains('?') {
                            final_url.push('&');
                        } else {
                            final_url.push('?');
                        }
                        final_url.push_str(&query_string);
                    }

                    // filter out and package only selected headers
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

                    let body_string = tab.request_body.text();

                    let token = tab.cancel_token.clone();
                    let method = tab.method;
                    let auth = tab.request_auth.clone();

                    return Command::perform(
                        send_request(
                            final_url,
                            method,
                            body_string,
                            filtered_headers,
                            filtered_cookies,
                            auth,
                            token,
                        ),
                        move |res| Message::ResponseReceived(tab_idx, res),
                    );
                }
                Command::none()
            }
            Message::ResponseReceived(tab_idx, res) => {
                if let Some(tab) = self.tabs.get_mut(tab_idx) {
                    tab.is_loading = false;
                    tab.response = Some(res);
                }
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let mut tab_bar = row![].spacing(5).align_items(Alignment::Center);
        for (idx, tab) in self.tabs.iter().enumerate() {
            let is_active = idx == self.active_tab_index;
            let mut tab_button = button(
                row![
                    text(&tab.name).size(13),
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

        let current_tab = &self.tabs[self.active_tab_index];
        let tab_view = current_tab.view(Message::ActiveTabMessage, Message::SendPressed);

        container(column![tab_bar, tab_view].spacing(18))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(25)
            .into()
    }
}
