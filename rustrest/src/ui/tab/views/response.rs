use super::super::Tab;
use super::super::messages::TabMessage;
use super::super::types::{ResponseSubTab, ResponseView, SavedResponse};
use crate::ui::context_menu::{TabFieldTarget, with_context_menu};
use iced::widget::{
    Space, button, column, container, pick_list, row, scrollable, text, text_editor,
};
use iced::{Alignment, Element, Font, Length};
use std::collections::HashMap;

pub fn render_response_pane<'a, Message>(
    tab: &'a Tab,
    wrap_msg: impl Fn(TabMessage) -> Message + Copy + 'static,
) -> Element<'a, Message>
where
    Message: Clone + 'static,
{
    let content = match tab
        .viewing_saved_response
        .and_then(|idx| tab.saved_responses.get(idx).map(|saved| (idx, saved)))
    {
        Some((idx, saved)) => render_saved_response(tab, idx, saved, wrap_msg),
        None => render_live_response(tab, wrap_msg),
    };

    // requests that belong to a collection get their saved responses
    if tab.collection_id.is_some() || tab.saved_responses.is_empty() {
        return content;
    }

    column![render_saved_responses_bar(tab, wrap_msg), content]
        .spacing(10)
        .height(Length::Fill)
        .into()
}

/// standalone-request fallback
fn render_saved_responses_bar<'a, Message>(
    tab: &'a Tab,
    wrap_msg: impl Fn(TabMessage) -> Message + Copy + 'static,
) -> Element<'a, Message>
where
    Message: Clone + 'static,
{
    let live_is_active = tab.viewing_saved_response.is_none();
    let mut live_btn = button(text("Live").size(12)).padding([4, 10]);
    live_btn = if live_is_active {
        live_btn.style(button::primary)
    } else {
        live_btn
            .style(button::secondary)
            .on_press(wrap_msg(TabMessage::ViewSavedResponse(None)))
    };
    let mut bar = row![live_btn].spacing(6).align_y(Alignment::Center);

    for (index, saved) in tab.saved_responses.iter().enumerate() {
        let is_active = tab.viewing_saved_response == Some(index);
        let mut select_btn = button(text(&saved.name).size(12)).padding([4, 10]);
        select_btn = if is_active {
            select_btn.style(button::primary)
        } else {
            select_btn
                .style(button::secondary)
                .on_press(wrap_msg(TabMessage::ViewSavedResponse(Some(index))))
        };

        let delete_btn = button(text("x").size(12))
            .padding([4, 8])
            .style(button::text)
            .on_press(wrap_msg(TabMessage::DeleteSavedResponse(index)));

        bar = bar.push(
            row![select_btn, delete_btn]
                .spacing(2)
                .align_y(Alignment::Center),
        );
    }

    bar.into()
}

fn response_tab_bar<Message>(
    active: ResponseSubTab,
    wrap_msg: impl Fn(TabMessage) -> Message + Copy + 'static,
) -> Element<'static, Message>
where
    Message: Clone + 'static,
{
    let mut resp_tab_bar = row![].spacing(4).align_y(Alignment::Center);

    for variant in ResponseSubTab::ALL.iter() {
        let is_resp_active = active == *variant;
        let mut resp_btn = button(text(variant.label()).size(13)).padding([6, 12]);

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

    resp_tab_bar.into()
}

fn status_color(status: u16) -> iced::Color {
    if (200..300).contains(&status) {
        iced::Color::from_rgb(0.12, 0.64, 0.35) // Elegant Emerald Green
    } else {
        iced::Color::from_rgb(0.87, 0.22, 0.22) // Coral/Red
    }
}

fn build_cookie_table<'a, Message>(headers: &HashMap<String, String>) -> Element<'a, Message>
where
    Message: Clone + 'static,
{
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

    if let Some(cookie_header) = headers
        .get("set-cookie")
        .or_else(|| headers.get("Set-Cookie"))
    {
        let cookies: Vec<&str> = cookie_header.split(';').collect();

        for (index, cookie_kv) in cookies.iter().enumerate() {
            let parts: Vec<&str> = cookie_kv.splitn(2, '=').collect();
            let key = parts.first().unwrap_or(&"").trim();
            let val = parts.get(1).unwrap_or(&"").trim();

            if !key.is_empty() {
                cookie_table = cookie_table.push(
                    container(
                        row![
                            text(key.to_string())
                                .font(Font::MONOSPACE)
                                .size(13)
                                .width(Length::FillPortion(2)),
                            text(val.to_string())
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

fn build_headers_table<'a, Message>(headers: &HashMap<String, String>) -> Element<'a, Message>
where
    Message: Clone + 'static,
{
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

    if headers.is_empty() {
        headers_table = headers_table.push(
            container(
                text("No headers returned.")
                    .size(13)
                    .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
            )
            .padding(10),
        );
    } else {
        let mut sorted_headers: Vec<(&String, &String)> = headers.iter().collect();
        sorted_headers.sort_by(|a, b| a.0.cmp(b.0));

        for (index, (key, val)) in sorted_headers.into_iter().enumerate() {
            headers_table = headers_table.push(
                container(
                    row![
                        text(key.to_string())
                            .font(Font {
                                weight: iced::font::Weight::Bold,
                                ..Font::DEFAULT
                            })
                            .size(13)
                            .width(Length::FillPortion(1)),
                        text(val.to_string())
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

fn render_live_response<'a, Message>(
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
            let metadata_row = row![
                text(format!("Status: {}", resp.status))
                    .color(status_color(resp.status))
                    .size(13),
                text(format!("Time: {} ms", resp.elapsed.as_millis()))
                    .color(iced::Color::from_rgb(0.5, 0.5, 0.5))
                    .size(13),
                button(text("Save Response").size(12))
                    .padding([3, 8])
                    .style(button::secondary)
                    .on_press(wrap_msg(TabMessage::SaveResponse)),
            ]
            .spacing(15)
            .align_y(Alignment::Center);

            let postman_header = row![
                response_tab_bar(tab.active_response_tab, wrap_msg),
                Space::new().width(Length::Fill),
                metadata_row
            ]
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

                    let response_editor = with_context_menu(
                        text_editor(&tab.response_body_editor)
                            .font(Font::MONOSPACE)
                            .size(13)
                            .on_action(move |act| {
                                wrap_msg(TabMessage::ResponseBodyEditorAction(act))
                            }),
                        wrap_msg(TabMessage::ShowFieldContextMenu(
                            TabFieldTarget::ResponseBodyEditor,
                            tab.response_body_editor
                                .selection()
                                .unwrap_or_else(|| tab.response_body_editor.text()),
                        )),
                    );

                    column![
                        view_toggle_bar,
                        container(scrollable(response_editor).height(Length::Fill))
                            .style(container::bordered_box)
                            .width(Length::Fill)
                            .height(Length::Fill)
                    ]
                    .spacing(8)
                    .height(Length::Fill)
                    .into()
                }

                ResponseSubTab::Cookies => build_cookie_table(&resp.headers),
                ResponseSubTab::Headers => build_headers_table(&resp.headers),

                ResponseSubTab::TestResults => {
                    let mut test_list = column![].spacing(8);

                    if resp.test_results.is_empty() {
                        test_list = test_list.push(
                            container(
                                text("No test results. Add assertions in the Post-request script tab (e.g. pm.test(...)) to view test outcomes here.")
                                    .size(13)
                                    .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
                            )
                            .padding(10),
                        );
                    } else {
                        for test in &resp.test_results {
                            let (status_text, status_color) = if test.passed {
                                ("PASS", iced::Color::from_rgb(0.12, 0.64, 0.35))
                            } else {
                                ("FAIL", iced::Color::from_rgb(0.87, 0.22, 0.22))
                            };

                            let test_row = container(
                                row![
                                    container(text(status_text).size(11).color(iced::Color::WHITE))
                                        .padding([2, 6])
                                        .style(move |_| container::Style {
                                            background: Some(iced::Background::Color(status_color)),
                                            border: iced::Border {
                                                radius: 4.0.into(),
                                                ..Default::default()
                                            },
                                            ..Default::default()
                                        }),
                                    text(&test.name).size(13),
                                ]
                                .spacing(10)
                                .align_y(Alignment::Center),
                            )
                            .padding(8)
                            .width(Length::Fill)
                            .style(container::bordered_box);

                            test_list = test_list.push(test_row);
                        }
                    }

                    scrollable(container(test_list).width(Length::Fill))
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

/// renders a previously-saved response snapshot, read-only
fn render_saved_response<'a, Message>(
    tab: &'a Tab,
    _index: usize,
    saved: &'a SavedResponse,
    wrap_msg: impl Fn(TabMessage) -> Message + Copy + 'static,
) -> Element<'a, Message>
where
    Message: Clone + 'static,
{
    let metadata_row = row![
        text(format!("Status: {}", saved.status))
            .color(status_color(saved.status))
            .size(13),
        text(format!("Time: {} ms", saved.elapsed_ms))
            .color(iced::Color::from_rgb(0.5, 0.5, 0.5))
            .size(13),
        text(format!("Saved: {}", saved.name))
            .color(iced::Color::from_rgb(0.5, 0.5, 0.5))
            .size(12),
        button(text("Back to Live").size(12))
            .padding([3, 8])
            .style(button::text)
            .on_press(wrap_msg(TabMessage::ViewSavedResponse(None))),
    ]
    .spacing(15)
    .align_y(Alignment::Center);

    let postman_header = row![
        response_tab_bar(tab.active_response_tab, wrap_msg),
        Space::new().width(Length::Fill),
        metadata_row
    ]
    .width(Length::Fill)
    .align_y(Alignment::Center);

    let dynamic_pane: Element<Message> = match tab.active_response_tab {
        ResponseSubTab::Body => container(
            scrollable(text(saved.body.clone()).font(Font::MONOSPACE).size(13))
                .height(Length::Fill),
        )
        .style(container::bordered_box)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(10)
        .into(),

        ResponseSubTab::Cookies => build_cookie_table(&saved.headers),
        ResponseSubTab::Headers => build_headers_table(&saved.headers),

        ResponseSubTab::TestResults => container(
            text("Test results aren't captured in saved responses.")
                .size(13)
                .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
        )
        .padding(10)
        .into(),
    };

    column![postman_header, dynamic_pane].spacing(12).into()
}
