use crate::app::{Rustrest, WorkspaceContent};
use crate::http_client::HttpMethod;
use crate::message::{Message, ResizeKind};
use crate::ui::context_menu::{FieldTarget, with_context_menu};
use crate::ui::unsaved::{tab_is_unsaved, unsaved_dot};
use iced::widget::{
    Id, Space, button, column, container, mouse_area, row, scrollable, text, text_input,
};
use iced::{Alignment, Element, Length};

/// id of the horizontally-scrolling tab strip, used to snap it to the newest tab whenever one is added
pub fn tab_bar_scroll_id() -> Id {
    Id::new("tab-bar-scroll")
}

pub fn render_workbench(app: &Rustrest) -> Element<'_, Message> {
    let env_selector = super::sidebar::render_env_selector(app);

    if app.tabs.is_empty() {
        let empty_state = column![
            iced::widget::text("No requests open").size(20),
            iced::widget::text("Create a new request or collection to get started.").size(13),
            row![
                button("New Request")
                    .on_press(Message::NewTabPressed)
                    .style(button::success)
                    .padding([8, 16]),
                button("New Collection")
                    .on_press(Message::CreateNewCollectionPressed)
                    .padding([8, 16]),
            ]
            .spacing(10),
        ]
        .spacing(12)
        .align_x(Alignment::Center);

        return column![
            row![Space::new().width(Length::Fill), env_selector].align_y(Alignment::Center),
            container(empty_state)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        ]
        .spacing(15)
        .height(Length::Fill)
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
            mouse_area(with_context_menu(
                text_input("", &tab.name)
                    .on_input(move |txt| Message::TabNameChanged(idx, txt))
                    .on_submit(Message::TabNameSave(idx))
                    .size(13)
                    .width(Length::Fixed(100.0)),
                Message::ShowTextFieldContextMenu(FieldTarget::TabName(idx), tab.name.clone()),
            ))
            .on_enter(Message::TabRenameInputHover(true))
            .on_exit(Message::TabRenameInputHover(false))
            .into()
        } else {
            button(text(&tab.name).size(13))
                .on_press(Message::TabNameDoubleClick(idx))
                .style(button::text)
                .padding(0)
                .into()
        };

        let mut tab_row = row![prefix_badge, tab_content]
            .spacing(6)
            .align_y(Alignment::Center);

        if tab_is_unsaved(app, tab_state) {
            tab_row = tab_row.push(unsaved_dot());
        }

        tab_row = tab_row.push(
            button("×")
                .on_press(Message::CloseTabPressed(idx))
                .padding(2)
                .style(button::text),
        );

        // a plain container so the outer mouse_area's on_press
        // still sees the mouse-down and can arm a drag - buttons only report
        // clicks on release, which is too late to catch the drag motion
        let tab_surface = container(tab_row)
            .padding(6)
            .style(move |theme: &iced::Theme| {
                if is_active {
                    container::Style {
                        background: Some(theme.palette().primary.into()),
                        text_color: Some(theme.palette().background),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                } else {
                    container::Style {
                        background: Some(iced::Color::from_rgba(0.5, 0.5, 0.5, 0.12).into()),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                }
            });

        let tab_element = mouse_area(tab_surface)
            .on_press(Message::TabDragStarted(idx))
            .on_enter(Message::TabDragEntered(idx))
            .interaction(iced::mouse::Interaction::Pointer);

        tab_bar = tab_bar.push(tab_element);
    }

    let add_tab_btn = button("+")
        .on_press(Message::NewTabPressed)
        .padding(6)
        .style(button::success);

    tab_bar = tab_bar.push(add_tab_btn);

    // tabs scroll horizontally within their own lane so a growing tab count
    // never squeezes or overlaps the fixed-width env selector on the right
    let tabs_scroll = scrollable(tab_bar)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(4).scroller_width(4),
        ))
        .width(Length::Fill)
        .id(tab_bar_scroll_id());

    let tab_bar_row = row![tabs_scroll, env_selector]
        .spacing(10)
        .align_y(Alignment::Center);

    let active_tab_state = &app.tabs[app.active_tab_index];

    // collection root UI, routed into the active workspace window
    let tab_view: Element<Message> = match &active_tab_state.content {
        WorkspaceContent::HttpRequest => active_tab_state.tab.view(
            Message::ActiveTabMessage,
            Message::SendPressed,
            app.request_pane_height,
            Message::ResizeDragStarted(ResizeKind::RequestPane),
        ),

        WorkspaceContent::CollectionRoot {
            collection_id,
            collection_name,
            active_sub_tab,
        } => super::collection_viewer::render_collection_root(
            *collection_id,
            collection_name,
            active_sub_tab,
            app,
        ),
    };

    column![tab_bar_row, tab_view].spacing(15).into()
}
