use iced::widget::{Space, button, column, pick_list, row, text, text_input};
use iced::{Alignment, Element, Length, Sandbox, Settings};

pub fn main() -> iced::Result {
    Rustrest::run(Settings::default())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                HttpMethod::GET => "GET",
                HttpMethod::POST => "POST",
                HttpMethod::PUT => "PUT",
                HttpMethod::DELETE => "DELETE",
            }
        )
    }
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

impl Sandbox for Rustrest {
    type Message = Message;

    fn new() -> Self {
        Self {
            url: String::from("https://httpbin.org/get"),
            method: HttpMethod::GET,
            response_body: String::from("Response will appear here..."),
            is_loading: false,
        }
    }

    fn title(&self) -> String {
        String::from("Rustrest - API Client")
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::UrlChanged(url) => {
                self.url = url;
            }
            Message::MethodChanged(method) => {
                self.method = method;
            }
            Message::SendPressed => {
                if self.is_loading {
                    return;
                }
                self.is_loading = true;
                self.response_body = String::from("Sending request...");

                let url = self.url.clone();
                let method = self.method;

                let url_clone = url.clone();

                match send_request(url_clone, method) {
                    Ok(res) => self.response_body = res,
                    Err(err) => self.response_body = format!("Error: {}", err),
                }
                self.is_loading = false;
            }
            Message::ResponseReceived(res) => {
                self.is_loading = false;
                match res {
                    Ok(body) => self.response_body = body,
                    Err(err) => self.response_body = err,
                }
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let methods = vec![
            HttpMethod::GET,
            HttpMethod::POST,
            HttpMethod::PUT,
            HttpMethod::DELETE,
        ];

        // HTTP Method Dropdown
        let method_picker =
            pick_list(methods, Some(self.method), Message::MethodChanged).padding(10);

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
        .on_press(Message::SendPressed)
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

// Helper synchronous wrapper for network request
fn send_request(url: String, method: HttpMethod) -> Result<String, reqwest::Error> {
    let client = reqwest::blocking::Client::new();
    let req = match method {
        HttpMethod::GET => client.get(&url),
        HttpMethod::POST => client.post(&url).body("{}"),
        HttpMethod::PUT => client.put(&url).body("{}"),
        HttpMethod::DELETE => client.delete(&url),
    };

    let response = req.send()?;
    response.text()
}
