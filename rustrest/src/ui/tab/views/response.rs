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

fn build_cookie_table<'a, Message>(
    headers: &HashMap<String, String>,
    wrap_msg: impl Fn(TabMessage) -> Message + Copy + 'static,
) -> Element<'a, Message>
where
    Message: Clone + 'static,
{
    let cookie_header = headers
        .get("set-cookie")
        .or_else(|| headers.get("Set-Cookie"));

    let rows: Vec<(String, String)> = cookie_header
        .map(|cookie_header| {
            cookie_header
                .split(';')
                .filter_map(|cookie_kv| {
                    let parts: Vec<&str> = cookie_kv.splitn(2, '=').collect();
                    let key = parts.first().unwrap_or(&"").trim().to_string();
                    let val = parts.get(1).unwrap_or(&"").trim().to_string();
                    if key.is_empty() {
                        None
                    } else {
                        Some((key, val))
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    build_kv_table(
        ("Name", "Value"),
        (2, 4),
        "No cookies returned in response headers.",
        rows,
        wrap_msg,
    )
}

fn build_headers_table<'a, Message>(
    headers: &HashMap<String, String>,
    wrap_msg: impl Fn(TabMessage) -> Message + Copy + 'static,
) -> Element<'a, Message>
where
    Message: Clone + 'static,
{
    let mut rows: Vec<(String, String)> = headers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));

    build_kv_table(
        ("Header Key", "Value"),
        (1, 2),
        "No headers returned.",
        rows,
        wrap_msg,
    )
}

/// builds a two-column key/value table (used for response headers and
/// cookies) where every cell offers a right-click "Copy" and every row has a
/// visible "Copy" button, plus a "Copy All" button that copies the whole
/// table as `key: value` lines.
fn build_kv_table<'a, Message>(
    column_labels: (&'static str, &'static str),
    column_portions: (u16, u16),
    empty_message: &'static str,
    rows: Vec<(String, String)>,
    wrap_msg: impl Fn(TabMessage) -> Message + Copy + 'static,
) -> Element<'a, Message>
where
    Message: Clone + 'static,
{
    let mut title_row = row![
        text(column_labels.0)
            .width(Length::FillPortion(column_portions.0))
            .size(12)
            .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
        text(column_labels.1)
            .width(Length::FillPortion(column_portions.1))
            .size(12)
            .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
    ]
    .padding(8)
    .align_y(Alignment::Center);

    if !rows.is_empty() {
        let all_text = rows
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join("\n");
        title_row = title_row.push(
            button(text("Copy All").size(11))
                .padding([2, 8])
                .style(button::text)
                .on_press(wrap_msg(TabMessage::CopyToClipboard(all_text))),
        );
    }

    let mut table = column![]
        .spacing(1)
        .push(container(title_row).style(container::bordered_box));

    if rows.is_empty() {
        table = table.push(
            container(
                text(empty_message)
                    .size(13)
                    .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
            )
            .padding(10),
        );
    } else {
        for (index, (key, val)) in rows.into_iter().enumerate() {
            let row_text = format!("{key}: {val}");

            let key_field = with_context_menu(
                text(key.clone())
                    .font(Font {
                        weight: iced::font::Weight::Bold,
                        ..Font::DEFAULT
                    })
                    .size(13)
                    .width(Length::FillPortion(column_portions.0)),
                wrap_msg(TabMessage::ShowFieldContextMenu(
                    TabFieldTarget::ResponseField,
                    key,
                )),
            );

            let value_field = with_context_menu(
                text(val.clone())
                    .font(Font::MONOSPACE)
                    .size(13)
                    .width(Length::FillPortion(column_portions.1)),
                wrap_msg(TabMessage::ShowFieldContextMenu(
                    TabFieldTarget::ResponseField,
                    val,
                )),
            );

            let copy_btn = button(text("Copy").size(11))
                .padding([2, 6])
                .style(button::text)
                .on_press(wrap_msg(TabMessage::CopyToClipboard(row_text)));

            table = table.push(
                container(
                    row![key_field, value_field, copy_btn]
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

    scrollable(container(table).width(Length::Fill))
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
                button(text(format!("Time: {} ms", resp.elapsed.as_millis())).size(13))
                    .padding(0)
                    .style(button::text)
                    .on_press(wrap_msg(TabMessage::ShowResponseTimingModal(None))),
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
                ResponseSubTab::Body if tab.sse_active => {
                    container(crate::ui::tab::protocol_common::message_log(
                        &tab.sse_log,
                        crate::ui::tab::protocol_common::sse_log_id(tab.id),
                    ))
                    .style(container::bordered_box)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
                }

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

                ResponseSubTab::Cookies => build_cookie_table(&resp.headers, wrap_msg),
                ResponseSubTab::Headers => build_headers_table(&resp.headers, wrap_msg),

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
    index: usize,
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
        button(text(format!("Time: {} ms", saved.elapsed_ms)).size(13))
            .padding(0)
            .style(button::text)
            .on_press(wrap_msg(TabMessage::ShowResponseTimingModal(Some(index)))),
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
        ResponseSubTab::Body => {
            let body_text = with_context_menu(
                text(saved.body.clone()).font(Font::MONOSPACE).size(13),
                wrap_msg(TabMessage::ShowFieldContextMenu(
                    TabFieldTarget::ResponseBodyEditor,
                    saved.body.clone(),
                )),
            );

            container(scrollable(body_text).height(Length::Fill))
                .style(container::bordered_box)
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(10)
                .into()
        }

        ResponseSubTab::Cookies => build_cookie_table(&saved.headers, wrap_msg),
        ResponseSubTab::Headers => build_headers_table(&saved.headers, wrap_msg),

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
