use super::super::Tab;
use super::super::components::kv_editor_pane;
use super::super::messages::TabMessage;
use super::super::types::{BodyType, RawType, RequestSubTab};
use crate::http_client::HttpMethod;
use iced::widget::{
    button, column, container, pick_list, radio, row, text, text_editor, text_input,
};
use iced::{Alignment, Element, Length};

pub fn render_request_bar<'a, Message>(
    tab: &'a Tab,
    wrap_msg: impl Fn(TabMessage) -> Message + Copy + 'static,
    on_send: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'static,
{
    let method_picker = pick_list(&HttpMethod::ALL[..], Some(tab.method), move |m| {
        wrap_msg(TabMessage::MethodChanged(m))
    })
    .padding(10);

    let url_input = text_input("https://api.example.com/v1/resource", &tab.url)
        .on_input(move |u| wrap_msg(TabMessage::UrlChanged(u)))
        .padding(12);

    let send_btn = if tab.is_loading {
        button("Cancel")
            .on_press(wrap_msg(TabMessage::CancelRequest))
            .style(iced::theme::Button::Destructive)
            .padding(12)
    } else {
        button("Send")
            .on_press(on_send)
            .style(iced::theme::Button::Primary)
            .padding(12)
    };

    row![method_picker, url_input, send_btn]
        .spacing(10)
        .align_items(Alignment::Center)
        .into()
}

pub fn render_configuration_pane<'a, Message>(
    tab: &'a Tab,
    wrap_msg: impl Fn(TabMessage) -> Message + Copy + 'static,
) -> Element<'a, Message>
where
    Message: Clone + 'static,
{
    let mut sub_tab_bar = row![].spacing(10);
    for variant in RequestSubTab::ALL.iter() {
        let is_sub_active = tab.active_sub_tab == *variant;
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

    let inner_input_field: Element<Message> = match tab.active_sub_tab {
        RequestSubTab::Params => kv_editor_pane(
            &tab.request_params,
            "Add Param",
            move |i, kv| wrap_msg(TabMessage::ParamRowChanged(i, kv)),
            wrap_msg(TabMessage::AddParamRow),
            move |i| wrap_msg(TabMessage::RemoveParamRow(i)),
        ),
        RequestSubTab::Headers => kv_editor_pane(
            &tab.request_headers,
            "Add Header",
            move |i, kv| wrap_msg(TabMessage::HeaderRowChanged(i, kv)),
            wrap_msg(TabMessage::AddHeaderRow),
            move |i| wrap_msg(TabMessage::RemoveHeaderRow(i)),
        ),
        RequestSubTab::Cookies => kv_editor_pane(
            &tab.request_cookies,
            "Add Cookie",
            move |i, kv| wrap_msg(TabMessage::CookieRowChanged(i, kv)),
            wrap_msg(TabMessage::AddCookieRow),
            move |i| wrap_msg(TabMessage::RemoveCookieRow(i)),
        ),
        RequestSubTab::Auth => text_input("Authorization Headers...", &tab.request_auth)
            .on_input(move |a| wrap_msg(TabMessage::AuthChanged(a)))
            .padding(10)
            .into(),

        RequestSubTab::Body => {
            let mut radio_bar = row![].spacing(15).align_items(Alignment::Center);
            for variant in BodyType::ALL.iter() {
                let radio_btn = radio(variant.label(), *variant, Some(tab.body_type), move |b| {
                    wrap_msg(TabMessage::BodyTypeChanged(b))
                });
                radio_bar = radio_bar.push(radio_btn);
            }

            let body_input: Element<Message> = match tab.body_type {
                BodyType::None => text("This request does not have a body payload.")
                    .style(iced::theme::Text::Color(iced::Color::from_rgb(
                        0.5, 0.5, 0.5,
                    )))
                    .into(),

                BodyType::FormData => kv_editor_pane(
                    &tab.body_form_data,
                    "Add Form Field",
                    move |i, kv| wrap_msg(TabMessage::FormDataRowChanged(i, kv)),
                    wrap_msg(TabMessage::AddFormDataRow),
                    move |i| wrap_msg(TabMessage::RemoveFormDataRow(i)),
                ),

                BodyType::XWwwFormUrlencoded => kv_editor_pane(
                    &tab.body_urlencoded,
                    "Add URL Encoded Pair",
                    move |i, kv| wrap_msg(TabMessage::UrlencodedRowChanged(i, kv)),
                    wrap_msg(TabMessage::AddUrlencodedRow),
                    move |i| wrap_msg(TabMessage::RemoveUrlencodedRow(i)),
                ),

                BodyType::Raw => {
                    let raw_dropdown = pick_list(&RawType::ALL[..], Some(tab.raw_type), move |t| {
                        wrap_msg(TabMessage::RawTypeChanged(t))
                    })
                    .padding(5);

                    let editor = text_editor(&tab.request_body)
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
                    let editor = text_editor(&tab.request_body)
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

    column![sub_tab_bar, inner_input_field].spacing(10).into()
}
