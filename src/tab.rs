use crate::http_client::{HttpMethod, HttpResponse};
use iced::widget::{button, column, container, pick_list, row, scrollable, text, text_input};
use iced::{Alignment, Element, Font, Length};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestSubTab {
    Params,
    Auth,
    Headers,
    Body,
}

impl RequestSubTab {
    pub const ALL: [Self; 4] = [Self::Params, Self::Auth, Self::Headers, Self::Body];

    fn name(&self) -> &str {
        match self {
            Self::Params => "Params",
            Self::Auth => "Authorization",
            Self::Headers => "Headers",
            Self::Body => "Body",
        }
    }
}

#[derive(Debug, Clone)]
pub enum TabMessage {
    UrlChanged(String),
    MethodChanged(HttpMethod),
    SubTabSelected(RequestSubTab),
    ParamsChanged(String),
    AuthChanged(String),
    HeadersChanged(String),
    BodyChanged(String),
}

pub struct Tab {
    pub id: usize,
    pub name: String,
    pub url: String,
    pub method: HttpMethod,
    pub active_sub_tab: RequestSubTab,
    pub request_params: String,
    pub request_auth: String,
    pub request_headers: String,
    pub request_body: String,
    pub response: Option<Result<HttpResponse, String>>,
    pub is_loading: bool,
}

impl Tab {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            name: format!("Request {}", id),
            url: String::from("https://httpbin.org/json"),
            method: HttpMethod::GET,
            active_sub_tab: RequestSubTab::Params,
            request_params: String::from("key=value&foo=bar"),
            request_auth: String::from("Bearer your_token_here"),
            request_headers: String::from(
                "Content-Type: application/json\nAccept: application/json",
            ),
            request_body: String::from("{\n  \"key\": \"value\"\n}"),
            response: None,
            is_loading: false,
        }
    }

    pub fn update(&mut self, message: TabMessage) {
        match message {
            TabMessage::UrlChanged(url) => self.url = url,
            TabMessage::MethodChanged(method) => self.method = method,
            TabMessage::SubTabSelected(sub_tab) => self.active_sub_tab = sub_tab,
            TabMessage::ParamsChanged(params) => self.request_params = params,
            TabMessage::AuthChanged(auth) => self.request_auth = auth,
            TabMessage::HeadersChanged(headers) => self.request_headers = headers,
            TabMessage::BodyChanged(body) => self.request_body = body,
        }
    }

    pub fn view<Message>(
        &self,
        wrap_msg: impl Fn(TabMessage) -> Message + Copy + 'static,
        on_send: Message,
    ) -> Element<'_, Message>
    where
        Message: Clone + 'static,
    {
        // HTTP Request bar layout elements
        let method_picker = pick_list(&HttpMethod::ALL[..], Some(self.method), move |m| {
            wrap_msg(TabMessage::MethodChanged(m))
        })
        .padding(10);

        let url_input = text_input("https://api.example.com/v1/resource", &self.url)
            .on_input(move |u| wrap_msg(TabMessage::UrlChanged(u)))
            .padding(12);

        let send_btn = button(if self.is_loading {
            "Sending..."
        } else {
            "Send"
        })
        .on_press_maybe(if self.is_loading { None } else { Some(on_send) })
        .padding(12);

        let request_bar = row![method_picker, url_input, send_btn]
            .spacing(10)
            .align_items(Alignment::Center);

        // Sub-tabs rendering loop
        let mut sub_tab_bar = row![].spacing(10);
        for variant in RequestSubTab::ALL.iter() {
            let is_sub_active = self.active_sub_tab == *variant;
            let mut sub_btn = button(text(variant.name()).size(12)).padding(6);

            if is_sub_active {
                sub_btn = sub_btn.style(iced::theme::Button::Primary);
            } else {
                let variant_clone = *variant;
                sub_btn = sub_btn
                    .style(iced::theme::Button::Text)
                    .on_press(wrap_msg(TabMessage::SubTabSelected(variant_clone)));
            }
            sub_tab_bar = sub_tab_bar.push(sub_btn);
        }

        // Sub-tab active panel selection
        let inner_input_field: Element<Message> = match self.active_sub_tab {
            RequestSubTab::Params => text_input("Query Parameters...", &self.request_params)
                .on_input(move |p| wrap_msg(TabMessage::ParamsChanged(p)))
                .padding(10)
                .into(),
            RequestSubTab::Auth => text_input("Authorization Headers...", &self.request_auth)
                .on_input(move |a| wrap_msg(TabMessage::AuthChanged(a)))
                .padding(10)
                .into(),
            RequestSubTab::Headers => text_input("Headers (Key: Value)...", &self.request_headers)
                .on_input(move |h| wrap_msg(TabMessage::HeadersChanged(h)))
                .padding(10)
                .into(),
            RequestSubTab::Body => text_input("JSON Request Body Payload...", &self.request_body)
                .on_input(move |b| wrap_msg(TabMessage::BodyChanged(b)))
                .padding(10)
                .into(),
        };

        let configuration_pane = column![sub_tab_bar, inner_input_field].spacing(10);

        // Response Render Frame Management
        let response_content: Element<Message> = match &self.response {
            None => text(if self.is_loading {
                "Awaiting network response..."
            } else {
                "No transactions dispatched yet."
            })
            .style(iced::theme::Text::Color(iced::Color::from_rgb(
                0.4, 0.4, 0.4,
            )))
            .into(),
            Some(Ok(resp)) => {
                let status_color = if (200..300).contains(&resp.status) {
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
                    .height(Length::Fixed(250.0))
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

        column![
            request_bar,
            configuration_pane,
            container(response_content)
                .width(Length::Fill)
                .padding(15)
                .style(iced::theme::Container::Box)
        ]
        .spacing(18)
        .into()
    }
}
