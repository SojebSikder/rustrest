mod http_client;

use http_client::{HttpMethod, HttpResponse, send_request};
use iced::widget::{button, column, container, pick_list, row, scrollable, text, text_input};
use iced::{Alignment, Application, Command, Element, Font, Length, Settings, Size, Theme};

pub fn main() -> iced::Result {
    let mut settings = Settings::default();
    settings.window.size = Size::new(900.0, 700.0);
    Rustrest::run(settings)
}

struct Rustrest {
    url: String,
    method: HttpMethod,
    request_body: String,
    request_headers: String,
    response: Option<Result<HttpResponse, String>>,
    is_loading: bool,
}

#[derive(Debug, Clone)]
enum Message {
    UrlChanged(String),
    MethodChanged(HttpMethod),
    RequestBodyChanged(String),
    RequestHeadersChanged(String),
    SendPressed,
    ResponseReceived(Result<HttpResponse, String>),
}

impl Application for Rustrest {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Message>) {
        (
            Self {
                url: String::from("https://httpbin.org/json"),
                method: HttpMethod::GET,
                request_body: String::from("{\n  \"key\": \"value\"\n}"),
                request_headers: String::from(
                    "Content-Type: application/json\nAccept: application/json",
                ),
                response: None,
                is_loading: false,
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Rustrest - API Client")
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::UrlChanged(url) => {
                self.url = url;
                Command::none()
            }
            Message::MethodChanged(method) => {
                self.method = method;
                Command::none()
            }
            Message::RequestBodyChanged(body) => {
                self.request_body = body;
                Command::none()
            }
            Message::RequestHeadersChanged(headers) => {
                self.request_headers = headers;
                Command::none()
            }
            Message::SendPressed => {
                if self.is_loading || self.url.is_empty() {
                    return Command::none();
                }
                self.is_loading = true;
                self.response = None;

                Command::perform(
                    send_request(
                        self.url.clone(),
                        self.method,
                        self.request_body.clone(),
                        self.request_headers.clone(),
                    ),
                    Message::ResponseReceived,
                )
            }
            Message::ResponseReceived(res) => {
                self.is_loading = false;
                self.response = Some(res);
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let method_picker: Element<Message> = pick_list(
            &HttpMethod::ALL[..],
            Some(self.method),
            Message::MethodChanged,
        )
        .padding(10)
        .into();

        let url_input: Element<Message> =
            text_input("https://api.example.com/v1/resource", &self.url)
                .on_input(Message::UrlChanged)
                .padding(12)
                .into();

        let send_btn: Element<Message> = button(if self.is_loading {
            "Sending..."
        } else {
            "Send"
        })
        .on_press_maybe(if self.is_loading {
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
            &self.request_headers,
        )
        .on_input(Message::RequestHeadersChanged)
        .padding(10)
        .into();

        let body_input: Element<Message> =
            text_input("JSON Request Body Payload...", &self.request_body)
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

        let response_content: Element<Message> = match &self.response {
            None => {
                if self.is_loading {
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
                    iced::Color::from_rgb(0.0, 0.6, 0.1) // Green
                } else {
                    iced::Color::from_rgb(0.8, 0.1, 0.1) // Red
                };

                let metadata_row: iced::widget::Row<'_, Message, Theme> = row![
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
                    .height(Length::Fixed(320.0))
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

        container(
            column![
                text("Rustrest API Client Studio").size(26),
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
