use super::super::Tab;
use super::super::messages::TabMessage;
use super::super::types::{ResponseSubTab, ResponseView};
use iced::widget::{
    Space, button, column, container, pick_list, row, scrollable, text, text_editor,
};
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
        .color(iced::Color::from_rgb(0.4, 0.4, 0.4))
        .into(),

        Some(Ok(resp)) => {
            let status_color = if (200..300).contains(&resp.status) {
                iced::Color::from_rgb(0.12, 0.64, 0.35) // Elegant Emerald Green
            } else {
                iced::Color::from_rgb(0.87, 0.22, 0.22) // Coral/Red
            };

            let metadata_row = row![
                text(format!("Status: {}", resp.status))
                    .color(status_color)
                    .size(13),
                text(format!("Time: {} ms", resp.elapsed.as_millis()))
                    .color(iced::Color::from_rgb(0.5, 0.5, 0.5))
                    .size(13),
            ]
            .spacing(15)
            .align_y(Alignment::Center);

            let response_tabs = [
                ResponseSubTab::Body,
                ResponseSubTab::Cookies,
                ResponseSubTab::Headers,
            ];
            let mut resp_tab_bar = row![].spacing(4).align_y(Alignment::Center);

            for variant in response_tabs.iter() {
                let is_resp_active = tab.active_response_tab == *variant;
                let tab_label = match variant {
                    ResponseSubTab::Body => "Body",
                    ResponseSubTab::Cookies => "Cookies",
                    ResponseSubTab::Headers => "Headers",
                };

                let mut resp_btn = button(text(tab_label).size(13)).padding([6, 12]);

                if is_resp_active {
                    resp_btn = resp_btn.style(button::primary);
                } else {
                    let variant_clone = *variant;
                    resp_btn = resp_btn
                        .style(button::text)
                        .on_press(wrap_msg(TabMessage::ResponseSubTabSelected(variant_clone)));
                }
                resp_tab_bar = resp_tab_bar.push(resp_btn);
            }

            let postman_header = row![resp_tab_bar, Space::new().width(Length::Fill), metadata_row]
                .width(Length::Fill)
                .align_y(Alignment::Center);

            let dynamic_pane: Element<Message> = match tab.active_response_tab {
                ResponseSubTab::Body => {
                    let view_dropdown =
                        pick_list(&ResponseView::ALL[..], Some(tab.response_view), move |v| {
                            wrap_msg(TabMessage::ResponseViewChanged(v))
                        })
                        .padding([4, 8]);

                    let view_toggle_bar = row![view_dropdown].spacing(8).align_y(Alignment::Center);

                    column![
                        view_toggle_bar,
                        container(
                            text_editor(&tab.response_body_editor)
                                .font(Font::MONOSPACE)
                                .size(13)
                                .height(Length::Fill)
                                .on_action(move |act| wrap_msg(
                                    TabMessage::ResponseBodyEditorAction(act)
                                ))
                        )
                        .style(container::bordered_box)
                        .width(Length::Fill)
                        .height(Length::Fill)
                    ]
                    .spacing(8)
                    .height(Length::Fill)
                    .into()
                }

                ResponseSubTab::Cookies => {
                    let mut cookie_table = column![].spacing(1);
                    cookie_table = cookie_table.push(
                        container(
                            row![
                                text("Name")
                                    .width(Length::FillPortion(2))
                                    .size(12)
                                    .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
                                text("Value")
                                    .width(Length::FillPortion(4))
                                    .size(12)
                                    .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
                            ]
                            .padding(8)
                            .align_y(Alignment::Center),
                        )
                        .style(container::bordered_box),
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
                                        .align_y(Alignment::Center),
                                    )
                                    .style(if index % 2 == 0 {
                                        container::bordered_box
                                    } else {
                                        container::transparent
                                    }),
                                );
                            }
                        }
                    } else {
                        cookie_table = cookie_table.push(
                            container(
                                text("No cookies returned in response headers.")
                                    .size(13)
                                    .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
                            )
                            .padding(10),
                        );
                    }

                    scrollable(container(cookie_table).width(Length::Fill))
                        .height(Length::Fill)
                        .into()
                }

                ResponseSubTab::Headers => {
                    let mut headers_table = column![].spacing(1);
                    headers_table = headers_table.push(
                        container(
                            row![
                                text("Header Key")
                                    .width(Length::FillPortion(1))
                                    .size(12)
                                    .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
                                text("Value")
                                    .width(Length::FillPortion(2))
                                    .size(12)
                                    .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
                            ]
                            .padding(8)
                            .align_y(Alignment::Center),
                        )
                        .style(container::bordered_box),
                    );

                    if resp.headers.is_empty() {
                        headers_table = headers_table.push(
                            container(
                                text("No headers returned.")
                                    .size(13)
                                    .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
                            )
                            .padding(10),
                        );
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
                                    .align_y(Alignment::Center),
                                )
                                .style(if index % 2 == 0 {
                                    container::bordered_box
                                } else {
                                    container::transparent
                                }),
                            );
                        }
                    }

                    scrollable(container(headers_table).width(Length::Fill))
                        .height(Length::Fill)
                        .into()
                }
            };

            column![postman_header, dynamic_pane].spacing(12).into()
        }

        Some(Err(err_msg)) => column![
            text("Transaction Failure")
                .color(iced::Color::from_rgb(0.9, 0.0, 0.0))
                .size(14),
            scrollable(
                text(err_msg)
                    .font(Font::MONOSPACE)
                    .size(13)
                    .color(iced::Color::from_rgb(0.7, 0.2, 0.2))
            )
            .height(Length::Fixed(150.0))
        ]
        .spacing(10)
        .into(),
    }
}
