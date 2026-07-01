use crate::app::{Rustrest, WorkspaceContent};
use crate::http_client::HttpMethod;
use crate::message::Message;
use iced::widget::{button, column, row, text, text_input};
use iced::{Alignment, Element, Length};

pub fn render_workbench(app: &Rustrest) -> Element<'_, Message> {
    if app.tabs.is_empty() {
        return iced::widget::text("No active requests open. Click a sidebar item or hit '+'.")
            .into();
    }

    let mut tab_bar = row![].spacing(5).align_y(Alignment::Center);

    for (idx, tab_state) in app.tabs.iter().enumerate() {
        let is_active = idx == app.active_tab_index;
        let tab = &tab_state.tab;

        let prefix_badge: Element<'_, Message> = match &tab_state.content {
            WorkspaceContent::HttpRequest => {
                let method_str = match &tab.method {
                    HttpMethod::Custom(c) if c.trim().is_empty() => "CUSTOM".to_string(),
                    HttpMethod::Custom(c) => c.to_uppercase(),
                    other => format!("{}", other),
                };
                text(format!("[{}]", method_str)).size(11).into()
            }
            WorkspaceContent::CollectionRoot { .. } => text("").size(11).into(),
        };

        let tab_content: Element<Message> = if tab_state.is_editing_name {
            text_input("", &tab.name)
                .on_input(move |txt| Message::TabNameChanged(idx, txt))
                .on_submit(Message::TabNameSave(idx))
                .size(13)
                .width(Length::Fixed(100.0))
                .into()
        } else {
            button(text(&tab.name).size(13))
                .on_press(Message::TabNameDoubleClick(idx))
                .style(button::text)
                .padding(0)
                .into()
        };

        let mut tab_button = button(
            row![
                prefix_badge,
                tab_content,
                button("×")
                    .on_press(Message::CloseTabPressed(idx))
                    .padding(2)
                    .style(button::text)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding(6);

        if !is_active {
            tab_button = tab_button
                .style(button::secondary)
                .on_press(Message::TabSelected(idx));
        }
        tab_bar = tab_bar.push(tab_button);
    }

    let add_tab_btn = button("+")
        .on_press(Message::NewTabPressed)
        .padding(6)
        .style(button::success);

    tab_bar = tab_bar.push(add_tab_btn);

    let active_tab_state = &app.tabs[app.active_tab_index];

    // collection root UI, routed into the active workspace window
    let tab_view: Element<Message> = match &active_tab_state.content {
        WorkspaceContent::HttpRequest => active_tab_state
            .tab
            .view(Message::ActiveTabMessage, Message::SendPressed),

        WorkspaceContent::CollectionRoot {
            collection_id,
            collection_name,
            active_sub_tab,
        } => super::collection_viewer::render_collection_root(
            *collection_id,
            collection_name,
            active_sub_tab,
            &app.collections,
        ),
    };

    column![tab_bar, tab_view].spacing(15).into()
}
