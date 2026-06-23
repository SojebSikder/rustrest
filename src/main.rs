mod http_client;

use http_client::{HttpMethod, send_request};
use iced::widget::{Space, button, column, pick_list, row, text, text_input};
use iced::{Alignment, Application, Command, Element, Length, Settings, Theme};

pub fn main() -> iced::Result {
    Rustrest::run(Settings::default())
}

struct Rustrest {
    url: String,
    method: HttpMethod,
    response_body: String,
    is_loading: bool,
}

#[derive(Debug, Clone)]
enum Message {
    UrlChanged(String),
    MethodChanged(HttpMethod),
    SendPressed,
    ResponseReceived(Result<String, String>),
}

impl Application for Rustrest {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Message>) {
        (
            Self {
                url: String::from("https://httpbin.org/get"),
                method: HttpMethod::GET,
                response_body: String::from("Response will appear here..."),
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
            Message::SendPressed => {
                if self.is_loading {
                    return Command::none();
                }
                self.is_loading = true;
                self.response_body = String::from("Sending request...");

                // Execute the asynchronous request off the main thread
                Command::perform(
                    send_request(self.url.clone(), self.method),
                    Message::ResponseReceived,
                )
            }
            Message::ResponseReceived(res) => {
                self.is_loading = false;
                match res {
                    Ok(body) => self.response_body = body,
                    Err(err) => self.response_body = err,
                }
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        // HTTP Method Dropdown
        let method_picker = pick_list(
            &HttpMethod::ALL[..],
            Some(self.method),
            Message::MethodChanged,
        )
        .padding(10);

        // URL Input field
        let url_input = text_input("Enter Request URL", &self.url)
            .on_input(Message::UrlChanged)
            .padding(10);

        // Send Button
        let send_btn = button(if self.is_loading {
            "Sending..."
        } else {
            "Send"
        })
        .on_press_maybe(if self.is_loading {
            None
        } else {
            Some(Message::SendPressed)
        })
        .padding(10);

        // Top Request Bar
        let request_bar = row![method_picker, url_input, send_btn]
            .spacing(10)
            .align_items(Alignment::Center);

        // Response Display Panel
        let response_display = text(&self.response_body).size(14);

        column![
            text("Rustrest API Client").size(24),
            Space::with_height(Length::Fixed(10.0)),
            request_bar,
            Space::with_height(Length::Fixed(20.0)),
            text("Response:").size(18),
            Space::with_height(Length::Fixed(5.0)),
            response_display
        ]
        .padding(20)
        .into()
    }
}
