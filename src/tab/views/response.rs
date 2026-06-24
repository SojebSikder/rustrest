use super::super::Tab;
use super::super::messages::TabMessage;
use super::super::types::{ResponseSubTab, ResponseView};
use iced::widget::{button, column, container, pick_list, row, scrollable, text};
use iced::{Alignment, Element, Font, Length};

pub fn render_response_pane<'a, Message>(
    tab: &'a Tab,
    wrap_msg: impl Fn(TabMessage) -> Message + Copy + 'static,
) -> Element<'a, Message>
where
    Message: Clone + 'static,
{
    match &tab.response {
        None => text(if tab.is_loading {
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
                let is_resp_active = tab.active_response_tab == *variant;
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

            let dynamic_pane: Element<Message> = match tab.active_response_tab {
                ResponseSubTab::Body => {
                    let view_dropdown =
                        pick_list(&ResponseView::ALL[..], Some(tab.response_view), move |v| {
                            wrap_msg(TabMessage::ResponseViewChanged(v))
                        })
                        .padding(5);

                    let view_toggle_bar = row![text("Response Format:").size(14), view_dropdown]
                        .spacing(10)
                        .align_items(Alignment::Center);

                    let processed_body = match tab.response_view {
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
                                    .style(if index % 2 == 0 {
                                        iced::theme::Container::Box
                                    } else {
                                        iced::theme::Container::Transparent
                                    }),
                                );
                            }
                        }
                    } else {
                        cookie_table = cookie_table.push(
                            container(text("No cookies returned in response headers.").size(13))
                                .padding(10),
                        );
                    }

                    scrollable(container(cookie_table).width(Length::Fill))
                        .height(Length::Fixed(220.0))
                        .into()
                }

                ResponseSubTab::Headers => {
                    let mut headers_table = column![].spacing(1);
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
                                            .width(Length::FillPortion(2)),
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

            column![metadata_row, resp_tab_bar, dynamic_pane]
                .spacing(10)
                .into()
        }

        Some(Ok(resp)) => unreachable!(),
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
    }
}
