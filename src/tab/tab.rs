use super::components::kv_editor_pane;
use super::messages::TabMessage;
use super::types::{BodyType, KeyValuePair, RawType, RequestSubTab, ResponseSubTab, ResponseView}; // Added ResponseSubTab
use crate::http_client::{HttpMethod, HttpResponse};

use iced::widget::{
    button, column, container, pick_list, radio, row, scrollable, text, text_editor, text_input,
};
use iced::{Alignment, Element, Font, Length};

pub struct Tab {
    pub id: usize,
    pub name: String,
    pub url: String,
    pub method: HttpMethod,
    pub active_sub_tab: RequestSubTab,
    pub active_response_tab: ResponseSubTab,
    pub body_type: BodyType,
    pub raw_type: RawType,
    pub response_view: ResponseView,
    pub request_params: Vec<KeyValuePair>,
    pub request_headers: Vec<KeyValuePair>,
    pub request_auth: String,
    pub request_body: text_editor::Content,
    pub body_form_data: Vec<KeyValuePair>,
    pub body_urlencoded: Vec<KeyValuePair>,
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
            active_response_tab: ResponseSubTab::Body,
            body_type: BodyType::Raw,
            raw_type: RawType::Json,
            response_view: ResponseView::Json,
            request_params: vec![
                KeyValuePair::new("key", "value"),
                KeyValuePair::new("foo", "bar"),
            ],
            request_headers: vec![
                KeyValuePair::new("Content-Type", "application/json"),
                KeyValuePair::new("Accept", "application/json"),
            ],
            request_auth: String::from("Bearer your_token_here"),
            request_body: text_editor::Content::with_text("{\n  \"key\": \"value\"\n}"),
            body_form_data: vec![KeyValuePair::new("form_field", "value")],
            body_urlencoded: vec![KeyValuePair::new("form_key", "form_value")],
            response: None,
            is_loading: false,
        }
    }

    pub fn update(&mut self, message: TabMessage) {
        match message {
            TabMessage::UrlChanged(url) => self.url = url,
            TabMessage::MethodChanged(method) => self.method = method,
            TabMessage::SubTabSelected(sub_tab) => self.active_sub_tab = sub_tab,
            TabMessage::ResponseSubTabSelected(resp_tab) => self.active_response_tab = resp_tab, // Handle state switch
            TabMessage::AuthChanged(auth) => self.request_auth = auth,
            TabMessage::BodyTypeChanged(body_type) => self.body_type = body_type,
            TabMessage::RawTypeChanged(raw_type) => self.raw_type = raw_type,

            TabMessage::ResponseViewChanged(view) => self.response_view = view,

            TabMessage::BodyChanged(action) => self.request_body.perform(action),

            TabMessage::ParamRowChanged(index, kv) => {
                if let Some(row) = self.request_params.get_mut(index) {
                    *row = kv;
                }
            }
            TabMessage::AddParamRow => {
                self.request_params.push(KeyValuePair::new("", ""));
            }
            TabMessage::RemoveParamRow(index) => {
                if index < self.request_params.len() {
                    self.request_params.remove(index);
                }
            }

            TabMessage::HeaderRowChanged(index, kv) => {
                if let Some(row) = self.request_headers.get_mut(index) {
                    *row = kv;
                }
            }
            TabMessage::AddHeaderRow => {
                self.request_headers.push(KeyValuePair::new("", ""));
            }
            TabMessage::RemoveHeaderRow(index) => {
                if index < self.request_headers.len() {
                    self.request_headers.remove(index);
                }
            }

            TabMessage::FormDataRowChanged(index, kv) => {
                if let Some(row) = self.body_form_data.get_mut(index) {
                    *row = kv;
                }
            }
            TabMessage::AddFormDataRow => {
                self.body_form_data.push(KeyValuePair::new("", ""));
            }
            TabMessage::RemoveFormDataRow(index) => {
                if index < self.body_form_data.len() {
                    self.body_form_data.remove(index);
                }
            }

            TabMessage::UrlencodedRowChanged(index, kv) => {
                if let Some(row) = self.body_urlencoded.get_mut(index) {
                    *row = kv;
                }
            }
            TabMessage::AddUrlencodedRow => {
                self.body_urlencoded.push(KeyValuePair::new("", ""));
            }
            TabMessage::RemoveUrlencodedRow(index) => {
                if index < self.body_urlencoded.len() {
                    self.body_urlencoded.remove(index);
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

        let inner_input_field: Element<Message> = match self.active_sub_tab {
            RequestSubTab::Params => kv_editor_pane(
                &self.request_params,
                "Add Param",
                move |i, kv| wrap_msg(TabMessage::ParamRowChanged(i, kv)),
                wrap_msg(TabMessage::AddParamRow),
                move |i| wrap_msg(TabMessage::RemoveParamRow(i)),
            ),
            RequestSubTab::Headers => kv_editor_pane(
                &self.request_headers,
                "Add Header",
                move |i, kv| wrap_msg(TabMessage::HeaderRowChanged(i, kv)),
                wrap_msg(TabMessage::AddHeaderRow),
                move |i| wrap_msg(TabMessage::RemoveHeaderRow(i)),
            ),
            RequestSubTab::Auth => text_input("Authorization Headers...", &self.request_auth)
                .on_input(move |a| wrap_msg(TabMessage::AuthChanged(a)))
                .padding(10)
                .into(),

            RequestSubTab::Body => {
                let mut radio_bar = row![].spacing(15).align_items(Alignment::Center);
                for variant in BodyType::ALL.iter() {
                    let radio_btn =
                        radio(variant.label(), *variant, Some(self.body_type), move |b| {
                            wrap_msg(TabMessage::BodyTypeChanged(b))
                        });
                    radio_bar = radio_bar.push(radio_btn);
                }

                let body_input: Element<Message> = match self.body_type {
                    BodyType::None => text("This request does not have a body payload.")
                        .style(iced::theme::Text::Color(iced::Color::from_rgb(
                            0.5, 0.5, 0.5,
                        )))
                        .into(),

                    BodyType::FormData => kv_editor_pane(
                        &self.body_form_data,
                        "Add Form Field",
                        move |i, kv| wrap_msg(TabMessage::FormDataRowChanged(i, kv)),
                        wrap_msg(TabMessage::AddFormDataRow),
                        move |i| wrap_msg(TabMessage::RemoveFormDataRow(i)),
                    ),

                    BodyType::XWwwFormUrlencoded => kv_editor_pane(
                        &self.body_urlencoded,
                        "Add URL Encoded Pair",
                        move |i, kv| wrap_msg(TabMessage::UrlencodedRowChanged(i, kv)),
                        wrap_msg(TabMessage::AddUrlencodedRow),
                        move |i| wrap_msg(TabMessage::RemoveUrlencodedRow(i)),
                    ),

                    BodyType::Raw => {
                        let raw_dropdown =
                            pick_list(&RawType::ALL[..], Some(self.raw_type), move |t| {
                                wrap_msg(TabMessage::RawTypeChanged(t))
                            })
                            .padding(5);

                        let editor = text_editor(&self.request_body)
                            .on_action(move |action| wrap_msg(TabMessage::BodyChanged(action)))
                            .height(Length::Fixed(300.0))
                            .padding(10);

                        column![
                            raw_dropdown,
                            container(editor)
                                .height(Length::Fixed(150.0))
                                .style(iced::theme::Container::Box)
                        ]
                        .spacing(10)
                        .into()
                    }

                    _ => {
                        let editor = text_editor(&self.request_body)
                            .on_action(move |action| wrap_msg(TabMessage::BodyChanged(action)))
                            .height(Length::Fixed(300.0))
                            .padding(10);

                        container(editor)
                            .height(Length::Fixed(150.0))
                            .style(iced::theme::Container::Box)
                            .into()
                    }
                };

                column![radio_bar, body_input].spacing(10).into()
            }
        };

        let configuration_pane = column![sub_tab_bar, inner_input_field].spacing(10);

        let response_content: Element<Message> = match &self.response {
            None => text(if self.is_loading {
                "Awaiting network response..."
            } else {
                "Enter a request and click 'Send' to see the response."
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

                let response_tabs = [
                    ResponseSubTab::Body,
                    ResponseSubTab::Cookies,
                    ResponseSubTab::Headers,
                ];
                let mut resp_tab_bar = row![].spacing(10);

                for variant in response_tabs.iter() {
                    let is_resp_active = self.active_response_tab == *variant;
                    let tab_label = match variant {
                        ResponseSubTab::Body => "Body",
                        ResponseSubTab::Cookies => "Cookies",
                        ResponseSubTab::Headers => "Headers",
                    };
                    let mut resp_btn = button(text(tab_label).size(12)).padding(6);

                    if is_resp_active {
                        resp_btn = resp_btn.style(iced::theme::Button::Primary);
                    } else {
                        let variant_clone = *variant;
                        resp_btn = resp_btn
                            .style(iced::theme::Button::Text)
                            .on_press(wrap_msg(TabMessage::ResponseSubTabSelected(variant_clone)));
                    }
                    resp_tab_bar = resp_tab_bar.push(resp_btn);
                }

                let dynamic_response_pane: Element<Message> = match self.active_response_tab {
                    ResponseSubTab::Body => {
                        let view_dropdown =
                            pick_list(&ResponseView::ALL[..], Some(self.response_view), move |v| {
                                wrap_msg(TabMessage::ResponseViewChanged(v))
                            })
                            .padding(5);

                        let view_toggle_bar =
                            row![text("Response Format:").size(14), view_dropdown]
                                .spacing(10)
                                .align_items(Alignment::Center);

                        let processed_body = match self.response_view {
                            ResponseView::Json => {
                                if let Ok(json_value) =
                                    serde_json::from_str::<serde_json::Value>(&resp.body)
                                {
                                    serde_json::to_string_pretty(&json_value)
                                        .unwrap_or_else(|_| resp.body.clone())
                                } else {
                                    format!(
                                        "// Invalid JSON (Showing Raw Payload instead):\n\n{}",
                                        resp.body
                                    )
                                }
                            }
                            ResponseView::Raw => resp.body.clone(),
                        };

                        column![
                            view_toggle_bar,
                            scrollable(
                                container(text(processed_body).font(Font::MONOSPACE).size(13))
                                    .padding(10)
                                    .style(iced::theme::Container::Box)
                                    .width(Length::Fill)
                            )
                            .height(Length::Fixed(220.0))
                        ]
                        .spacing(10)
                        .into()
                    }

                    ResponseSubTab::Cookies => {
                        let mut cookie_table = column![].spacing(1);

                        // table headers
                        cookie_table = cookie_table.push(
                            container(
                                row![
                                    text("Name").width(Length::FillPortion(2)).size(12),
                                    text("Value").width(Length::FillPortion(4)).size(12),
                                ]
                                .padding(8)
                                .align_items(Alignment::Center),
                            )
                            .style(iced::theme::Container::Box),
                        );

                        if let Some(cookie_header) = resp
                            .headers
                            .get("set-cookie")
                            .or_else(|| resp.headers.get("Set-Cookie"))
                        {
                            // basic parsing split by semicolon for presentation
                            let cookies: Vec<&str> = cookie_header.split(';').collect();

                            for (index, cookie_kv) in cookies.iter().enumerate() {
                                let parts: Vec<&str> = cookie_kv.splitn(2, '=').collect();
                                let key = parts.get(0).unwrap_or(&"").trim();
                                let val = parts.get(1).unwrap_or(&"").trim();

                                if !key.is_empty() {
                                    cookie_table = cookie_table.push(
                                        container(
                                            row![
                                                text(key)
                                                    .font(Font::MONOSPACE)
                                                    .size(13)
                                                    .width(Length::FillPortion(2)),
                                                text(val)
                                                    .font(Font::MONOSPACE)
                                                    .size(13)
                                                    .width(Length::FillPortion(4)),
                                            ]
                                            .padding(8)
                                            .align_items(Alignment::Center),
                                        )
                                        .style(
                                            if index % 2 == 0 {
                                                iced::theme::Container::Box // Alternating rows
                                            } else {
                                                iced::theme::Container::Transparent
                                            },
                                        ),
                                    );
                                }
                            }
                        } else {
                            cookie_table = cookie_table.push(
                                container(
                                    text("No cookies returned in response headers.").size(13),
                                )
                                .padding(10), //  Moved padding to the container
                            );
                        }

                        scrollable(container(cookie_table).width(Length::Fill))
                            .height(Length::Fixed(220.0))
                            .into()
                    }
                    ResponseSubTab::Headers => {
                        let mut headers_table = column![].spacing(1);

                        // Table Headers
                        headers_table = headers_table.push(
                            container(
                                row![
                                    text("Header Key").width(Length::FillPortion(1)).size(12),
                                    text("Value").width(Length::FillPortion(2)).size(12),
                                ]
                                .padding(8)
                                .align_items(Alignment::Center),
                            )
                            .style(iced::theme::Container::Box),
                        );

                        if resp.headers.is_empty() {
                            headers_table = headers_table
                                .push(container(text("No headers returned.").size(13)).padding(10));
                        } else {
                            let mut sorted_headers: Vec<(&String, &String)> =
                                resp.headers.iter().collect();
                            sorted_headers.sort_by(|a, b| a.0.cmp(b.0));

                            for (index, (key, val)) in sorted_headers.into_iter().enumerate() {
                                headers_table = headers_table.push(
                                    container(
                                        row![
                                            text(key)
                                                .font(Font {
                                                    weight: iced::font::Weight::Bold,
                                                    ..Font::DEFAULT
                                                })
                                                .size(13)
                                                .width(Length::FillPortion(1)),
                                            text(val)
                                                .font(Font::MONOSPACE)
                                                .size(13)
                                                .width(Length::FillPortion(2))
                                        ]
                                        .padding(8)
                                        .align_items(Alignment::Center),
                                    )
                                    .style(if index % 2 == 0 {
                                        iced::theme::Container::Box
                                    } else {
                                        iced::theme::Container::Transparent
                                    }),
                                );
                            }
                        }

                        scrollable(container(headers_table).width(Length::Fill))
                            .height(Length::Fixed(220.0))
                            .into()
                    }
                };

                column![metadata_row, resp_tab_bar, dynamic_response_pane]
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
