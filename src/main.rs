#![windows_subsystem = "windows"]

mod http_client;

use http_client::{HttpMethod, HttpResponse, send_request};
use iced::widget::{button, column, container, pick_list, row, scrollable, text, text_input};
use iced::{Alignment, Application, Command, Element, Font, Length, Settings, Size, Theme};

// define app name
const APP_NAME: &str = "Rustrest";

pub fn main() -> iced::Result {
    let mut settings = Settings::default();
    settings.window.size = Size::new(1000.0, 750.0);
    Rustrest::run(settings)
}

/// Dynamic reusable state holding structural data for a singular Postman-style Tab
struct Tab {
    id: usize,
    name: String,
    url: String,
    method: HttpMethod,
    request_body: String,
    request_headers: String,
    response: Option<Result<HttpResponse, String>>,
    is_loading: bool,
}

impl Tab {
    fn new(id: usize) -> Self {
        Self {
            id,
            name: format!("Request {}", id),
            url: String::from("https://httpbin.org/json"),
            method: HttpMethod::GET,
            request_body: String::from("{\n  \"key\": \"value\"\n}"),
            request_headers: String::from(
                "Content-Type: application/json\nAccept: application/json",
            ),
            response: None,
            is_loading: false,
        }
    }
}

struct Rustrest {
    tabs: Vec<Tab>,
    active_tab_index: usize,
    next_tab_id: usize,
}

#[derive(Debug, Clone)]
enum Message {
    // Tab Management
    TabSelected(usize),
    NewTabPressed,
    CloseTabPressed(usize),

    // Core Client Actions
    UrlChanged(String),
    MethodChanged(HttpMethod),
    RequestBodyChanged(String),
    RequestHeadersChanged(String),
    SendPressed,
    ResponseReceived(usize, Result<HttpResponse, String>), // Carries tab target index context
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
        String::from(APP_NAME.to_string() + " - API Testing Platform")
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
                    self.tabs.remove(index);
                    // Prevent index bound crashes when deleting surrounding components
                    if self.active_tab_index >= self.tabs.len() {
                        self.active_tab_index = self.tabs.len() - 1;
                    }
                }
                Command::none()
            }
            Message::UrlChanged(url) => {
                if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
                    tab.url = url;
                }
                Command::none()
            }
            Message::MethodChanged(method) => {
                if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
                    tab.method = method;
                }
                Command::none()
            }
            Message::RequestBodyChanged(body) => {
                if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
                    tab.request_body = body;
                }
                Command::none()
            }
            Message::RequestHeadersChanged(headers) => {
                if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
                    tab.request_headers = headers;
                }
                Command::none()
            }
            Message::SendPressed => {
                let tab_idx = self.active_tab_index;
                if let Some(tab) = self.tabs.get_mut(tab_idx) {
                    if tab.is_loading || tab.url.is_empty() {
                        return Command::none();
                    }
                    tab.is_loading = true;
                    tab.response = None;

                    return Command::perform(
                        send_request(
                            tab.url.clone(),
                            tab.method,
                            tab.request_body.clone(),
                            tab.request_headers.clone(),
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
        // tab selector bar
        let mut tab_bar = row![].spacing(5).align_items(Alignment::Center);

        for (idx, tab) in self.tabs.iter().enumerate() {
            let is_active = idx == self.active_tab_index;

            // Build tab item containing title and individual close action
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
                // Dim non-focused context variants
                tab_button = tab_button
                    .style(iced::theme::Button::Secondary)
                    .on_press(Message::TabSelected(idx));
            }

            tab_bar = tab_bar.push(tab_button);
        }

        // action selector element addition button
        let add_tab_btn = button("+")
            .on_press(Message::NewTabPressed)
            .padding(8)
            .style(iced::theme::Button::Positive);

        tab_bar = tab_bar.push(add_tab_btn);

        // render the content panes for the selected tab
        let current_tab = &self.tabs[self.active_tab_index];

        let method_picker: Element<Message> = pick_list(
            &HttpMethod::ALL[..],
            Some(current_tab.method),
            Message::MethodChanged,
        )
        .padding(10)
        .into();

        let url_input: Element<Message> =
            text_input("https://api.example.com/v1/resource", &current_tab.url)
                .on_input(Message::UrlChanged)
                .padding(12)
                .into();

        let send_btn: Element<Message> = button(if current_tab.is_loading {
            "Sending..."
        } else {
            "Send"
        })
        .on_press_maybe(if current_tab.is_loading {
            None
        } else {
            Some(Message::SendPressed)
        })
        .padding(12)
        .into();

        let request_bar = row![method_picker, url_input, send_btn]
            .spacing(10)
            .align_items(Alignment::Center);

        let headers_input: Element<Message> = text_input(
            "Headers (Format -> Key: Value, one per line)",
            &current_tab.request_headers,
        )
        .on_input(Message::RequestHeadersChanged)
        .padding(10)
        .into();

        let body_input: Element<Message> =
            text_input("JSON Request Body Payload...", &current_tab.request_body)
                .on_input(Message::RequestBodyChanged)
                .padding(10)
                .into();

        let configuration_pane = column![
            text("Headers").size(14),
            headers_input,
            text("JSON Body Raw Payload").size(14),
            body_input,
        ]
        .spacing(8);

        let response_content: Element<Message> = match &current_tab.response {
            None => {
                if current_tab.is_loading {
                    text("Awaiting network target buffer transmission...")
                        .style(iced::theme::Text::Color(iced::Color::from_rgb(
                            0.5, 0.5, 0.5,
                        )))
                        .into()
                } else {
                    text("No transactions dispatched yet.")
                        .style(iced::theme::Text::Color(iced::Color::from_rgb(
                            0.4, 0.4, 0.4,
                        )))
                        .into()
                }
            }
            Some(Ok(resp)) => {
                let status_color = if resp.status >= 200 && resp.status < 300 {
                    iced::Color::from_rgb(0.0, 0.6, 0.1)
                } else {
                    iced::Color::from_rgb(0.8, 0.1, 0.1)
                };

                let metadata_row = row![
                    text(format!("Status: {}", resp.status))
                        .style(iced::theme::Text::Color(status_color)),
                    text(format!(" | Latency: {}ms", resp.elapsed.as_millis())).size(14),
                ]
                .spacing(10);

                column![
                    metadata_row,
                    text("Response Payload:").size(14),
                    scrollable(
                        container(text(&resp.body).font(Font::MONOSPACE).size(13))
                            .padding(10)
                            .style(iced::theme::Container::Box)
                    )
                    .height(Length::Fixed(280.0))
                ]
                .spacing(10)
                .into()
            }
            Some(Err(err_msg)) => column![
                text("Transaction Failure").style(iced::theme::Text::Color(iced::Color::from_rgb(
                    0.9, 0.0, 0.0
                ))),
                scrollable(text(err_msg).font(Font::MONOSPACE).size(13).style(
                    iced::theme::Text::Color(iced::Color::from_rgb(0.7, 0.2, 0.2))
                ))
                .height(Length::Fixed(150.0))
            ]
            .spacing(10)
            .into(),
        };

        // main application grid layout
        container(
            column![
                // text("Rustrest API Testing Platform").size(24),
                tab_bar,
                request_bar,
                configuration_pane,
                container(response_content)
                    .width(Length::Fill)
                    .padding(15)
                    .style(iced::theme::Container::Box)
            ]
            .spacing(18),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(25)
        .into()
    }
}
