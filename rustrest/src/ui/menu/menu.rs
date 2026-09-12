use iced::widget::{button, column, container, opaque, row, text};
use iced::{Border, Element, Length, alignment};

/// tracks the open/closed state of a reusable dropdown menu.
#[derive(Debug, Clone, Default)]
pub struct DropdownMenuState {
    pub open_index: Option<usize>,
}

impl DropdownMenuState {
    pub fn new() -> Self {
        Self { open_index: None }
    }

    /// Handles the internal state mutation when a menu item or header is clicked.
    pub fn update<T: Clone>(&mut self, message: DropdownMessage<T>) -> Option<T> {
        match message {
            DropdownMessage::Toggle(index) => {
                if self.open_index == Some(index) {
                    self.open_index = None;
                } else {
                    self.open_index = Some(index);
                }
                None
            }
            DropdownMessage::Close => {
                self.open_index = None;
                None
            }
            DropdownMessage::TriggerAction(action) => {
                self.open_index = None; // Automatically close menu on action selection
                Some(action)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum DropdownMessage<T> {
    Toggle(usize),
    Close,
    TriggerAction(T),
}

/// represents an individual actionable item inside a dropdown list.
pub struct DropdownItem<T> {
    label: String,
    action: T,
}

impl<T> DropdownItem<T> {
    pub fn new(label: impl Into<String>, action: T) -> Self {
        Self {
            label: label.into(),
            action,
        }
    }
}

/// configuration structure for a top-level menu column (e.g., "File" or "Help").
pub struct MenuGroup<T> {
    title: String,
    items: Vec<DropdownItem<T>>,
}

impl<T> MenuGroup<T> {
    pub fn new(title: impl Into<String>, items: Vec<DropdownItem<T>>) -> Self {
        Self {
            title: title.into(),
            items,
        }
    }
}

/// renders the floating dropdown overlay panel if one is open.
pub fn render_menu_overlay<'a, T: 'static + Clone>(
    state: &DropdownMenuState,
    groups: &[MenuGroup<T>],
) -> Option<Element<'a, DropdownMessage<T>>> {
    let open_idx = state.open_index?;
    let target_group = groups.get(open_idx)?;

    let mut items_column = column![].spacing(2);

    for item in &target_group.items {
        items_column = items_column.push(
            button(text(item.label.clone()).size(14))
                .width(Length::Fill)
                .padding([6, 12])
                .style(button::text)
                .on_press(DropdownMessage::TriggerAction(item.action.clone())),
        );
    }

    let dropdown_panel = container(items_column)
        .width(140)
        .padding(4)
        .style(|theme| container::Style {
            background: Some(theme.palette().background.into()),
            border: Border {
                color: theme.palette().primary,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        });

    // approximate sizing alignment offset (adjust as needed for your font/padding)
    let horizontal_offset = 8.0 + (open_idx as f32 * 68.0);

    let overlay_layer = column![
        container(text("")).height(32),
        row![
            container(text("")).width(horizontal_offset),
            opaque(dropdown_panel)
        ]
    ]
    .width(Length::Fill)
    .height(Length::Fill); // make it fill the application screen so it doesn't clip!

    Some(overlay_layer.into())
}

/// renders the horizontal menu bar strip.
pub fn render_menu_bar<'a, T: 'static + Clone>(
    groups: &[MenuGroup<T>],
) -> Element<'a, DropdownMessage<T>> {
    let mut menu_row = row![].spacing(8).align_y(alignment::Vertical::Center);

    for (group_idx, group) in groups.iter().enumerate() {
        let header_button = button(text(group.title.clone()).size(14))
            .padding([4, 10])
            .style(button::text)
            .on_press(DropdownMessage::Toggle(group_idx));

        menu_row = menu_row.push(header_button);
    }

    container(menu_row)
        .width(Length::Fill)
        .padding([4, 8])
        .style(|theme| container::Style {
            background: Some(theme.palette().background.into()),
            border: Border {
                width: 1.0,
                color: iced::Color::from_rgba8(0, 0, 0, 0.1),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}
