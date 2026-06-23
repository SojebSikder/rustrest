use crate::http_client::{HttpMethod, HttpResponse};
use iced::widget::{
    button, checkbox, column, container, pick_list, row, scrollable, text, text_input,
};
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

// represents a key-value row with active selection status
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyValuePair {
    pub is_active: bool,
    pub key: String,
    pub value: String,
}

impl KeyValuePair {
    pub fn new(key: &str, value: &str) -> Self {
        Self {
            is_active: true,
            key: String::from(key),
            value: String::from(value),
        }
    }
}

#[derive(Debug, Clone)]
pub enum TabMessage {
    UrlChanged(String),
    MethodChanged(HttpMethod),
    SubTabSelected(RequestSubTab),
    // Auth and Body remain strings, while Params and Headers are now list-based
    AuthChanged(String),
    BodyChanged(String),
    // Multi-item action messages
    ParamRowChanged(usize, KeyValuePair),
    HeaderRowChanged(usize, KeyValuePair),
    AddParamRow,
    AddHeaderRow,
    RemoveParamRow(usize),
    RemoveHeaderRow(usize),
}

pub struct Tab {
    pub id: usize,
    pub name: String,
    pub url: String,
    pub method: HttpMethod,
    pub active_sub_tab: RequestSubTab,
    pub request_params: Vec<KeyValuePair>,
    pub request_headers: Vec<KeyValuePair>,
    pub request_auth: String,
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
            request_params: vec![
                KeyValuePair::new("key", "value"),
                KeyValuePair::new("foo", "bar"),
            ],
            request_headers: vec![
                KeyValuePair::new("Content-Type", "application/json"),
                KeyValuePair::new("Accept", "application/json"),
            ],
            request_auth: String::from("Bearer your_token_here"),
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
            TabMessage::AuthChanged(auth) => self.request_auth = auth,
            TabMessage::BodyChanged(body) => self.request_body = body,

            // Params and Headers mutations
            TabMessage::ParamRowChanged(index, kv) => {
                if let Some(row) = self.request_params.get_mut(index) {
                    *row = kv;
                }
            }
            TabMessage::HeaderRowChanged(index, kv) => {
                if let Some(row) = self.request_headers.get_mut(index) {
                    *row = kv;
                }
            }
            TabMessage::AddParamRow => {
                self.request_params.push(KeyValuePair::new("", ""));
            }
            TabMessage::AddHeaderRow => {
                self.request_headers.push(KeyValuePair::new("", ""));
            }
            TabMessage::RemoveParamRow(index) => {
                if index < self.request_params.len() {
                    self.request_params.remove(index);
                }
            }
            TabMessage::RemoveHeaderRow(index) => {
                if index < self.request_headers.len() {
                    self.request_headers.remove(index);
                }
            }
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
            RequestSubTab::Params => {
                let mut content = column![].spacing(5);
                for (idx, item) in self.request_params.iter().enumerate() {
                    let item_clone = item.clone();
                    let key_clone = item.key.clone();
                    let val_clone = item.value.clone();

                    let row_element = row![
                        checkbox("", item.is_active).on_toggle(move |checked| {
                            wrap_msg(TabMessage::ParamRowChanged(
                                idx,
                                KeyValuePair {
                                    is_active: checked,
                                    key: key_clone.clone(),
                                    value: val_clone.clone(),
                                },
                            ))
                        }),
                        text_input("Key", &item.key)
                            .on_input(move |k| {
                                wrap_msg(TabMessage::ParamRowChanged(
                                    idx,
                                    KeyValuePair {
                                        is_active: item_clone.is_active,
                                        key: k,
                                        value: item_clone.value.clone(),
                                    },
                                ))
                            })
                            .padding(8),
                        text_input("Value", &item.value)
                            .on_input(move |v| {
                                wrap_msg(TabMessage::ParamRowChanged(
                                    idx,
                                    KeyValuePair {
                                        is_active: item_clone.is_active,
                                        key: item_clone.key.clone(),
                                        value: v,
                                    },
                                ))
                            })
                            .padding(8),
                        button("Delete")
                            .on_press(wrap_msg(TabMessage::RemoveParamRow(idx)))
                            .padding(8)
                            .style(iced::theme::Button::Destructive)
                    ]
                    .spacing(8)
                    .align_items(Alignment::Center);

                    content = content.push(row_element);
                }

                column![
                    scrollable(content).height(Length::Fixed(150.0)),
                    button("Add Param")
                        .on_press(wrap_msg(TabMessage::AddParamRow))
                        .padding(8)
                ]
                .spacing(10)
                .into()
            }
            RequestSubTab::Headers => {
                let mut content = column![].spacing(5);
                for (idx, item) in self.request_headers.iter().enumerate() {
                    let item_clone = item.clone();
                    let key_clone = item.key.clone();
                    let val_clone = item.value.clone();

                    let row_element = row![
                        checkbox("", item.is_active).on_toggle(move |checked| {
                            wrap_msg(TabMessage::HeaderRowChanged(
                                idx,
                                KeyValuePair {
                                    is_active: checked,
                                    key: key_clone.clone(),
                                    value: val_clone.clone(),
                                },
                            ))
                        }),
                        text_input("Key", &item.key)
                            .on_input(move |k| {
                                wrap_msg(TabMessage::HeaderRowChanged(
                                    idx,
                                    KeyValuePair {
                                        is_active: item_clone.is_active,
                                        key: k,
                                        value: item_clone.value.clone(),
                                    },
                                ))
                            })
                            .padding(8),
                        text_input("Value", &item.value)
                            .on_input(move |v| {
                                wrap_msg(TabMessage::HeaderRowChanged(
                                    idx,
                                    KeyValuePair {
                                        is_active: item_clone.is_active,
                                        key: item_clone.key.clone(),
                                        value: v,
                                    },
                                ))
                            })
                            .padding(8),
                        button("Delete")
                            .on_press(wrap_msg(TabMessage::RemoveHeaderRow(idx)))
                            .padding(8)
                            .style(iced::theme::Button::Destructive)
                    ]
                    .spacing(8)
                    .align_items(Alignment::Center);

                    content = content.push(row_element);
                }

                column![
                    scrollable(content).height(Length::Fixed(150.0)),
                    button("Add Header")
                        .on_press(wrap_msg(TabMessage::AddHeaderRow))
                        .padding(8)
                ]
                .spacing(10)
                .into()
            }
            RequestSubTab::Auth => text_input("Authorization Headers...", &self.request_auth)
                .on_input(move |a| wrap_msg(TabMessage::AuthChanged(a)))
                .padding(10)
                .into(),
            RequestSubTab::Body => scrollable(
                text_input("JSON Request Body Payload...", &self.request_body)
                    .on_input(move |b| wrap_msg(TabMessage::BodyChanged(b)))
                    .padding(10),
            )
            .height(Length::Fixed(150.0))
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
