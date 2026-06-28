use crate::app::Rustrest;
use crate::collection::CollectionItem;
use crate::message::Message;
use iced::widget::{Column, button, column, container, pick_list, row, scrollable, text};
use iced::{Alignment, Element, Font, Length, Padding};

pub fn render_sidebar(app: &Rustrest) -> Element<'_, Message> {
    let env_options: Vec<String> = app.environments.iter().map(|e| e.name.clone()).collect();
    let current_env_selection = app
        .active_env_index
        .and_then(|idx| app.environments.get(idx))
        .map(|e| e.name.clone());

    let env_selector = row![
        pick_list(env_options, current_env_selection, |selected| {
            Message::EnvSelected(Some(selected))
        })
        .placeholder("No Environment")
        .width(Length::Fixed(180.0))
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let mut sidebar_contents = column![
        button("Import Collection")
            .on_press(Message::ImportCollectionPressed)
            .padding(8)
            .width(Length::Fill),
        container(env_selector).padding(Padding {
            top: 5.0,
            right: 0.0,
            bottom: 10.0,
            left: 0.0,
        }),
    ]
    .spacing(10);

    if app.collections.is_empty() {
        sidebar_contents = sidebar_contents.push(
            text("No collections imported yet.")
                .size(11)
                .style(text::secondary),
        );
    } else {
        for col in &app.collections {
            let col_id = col.id;

            let collection_header = button(
                text(format!("📁 {}", col.info.name))
                    .font(Font {
                        weight: iced::font::Weight::Bold,
                        ..Font::DEFAULT
                    })
                    .size(15),
            )
            .on_press(Message::SidebarCollectionRootClicked(col_id))
            .style(button::text)
            .padding([4, 2]);

            let mut col_tree = column![collection_header].spacing(4);

            for item in &col.item {
                col_tree = render_sidebar_item(col_tree, item);
            }
            sidebar_contents = sidebar_contents.push(col_tree);
        }
    }

    container(scrollable(sidebar_contents))
        .width(Length::Fixed(260.0))
        .height(Length::Fill)
        .padding(10)
        .style(container::bordered_box)
        .into()
}

fn render_sidebar_item<'a>(
    layout: Column<'a, Message>,
    item: &'a CollectionItem,
) -> Column<'a, Message> {
    match item {
        CollectionItem::Folder {
            name,
            item: sub_items,
        } => {
            let mut folder_layout = column![text(format!("📁 {}", name)).size(15)]
                .spacing(3)
                .padding(Padding {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 10.0,
                });
            for sub in sub_items {
                folder_layout = render_sidebar_item(folder_layout, sub);
            }
            layout.push(folder_layout)
        }
        CollectionItem::Request(req_node) => {
            let req_clone = req_node.clone();
            let label = format!("{} - {}", req_node.request.method, req_node.name);
            layout.push(
                button(text(label).size(15))
                    .on_press(Message::SidebarRequestClicked(req_clone))
                    .style(button::text)
                    .padding([2, 5]),
            )
        }
    }
}
