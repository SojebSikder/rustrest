use iced::widget::{Space, button, column, container, opaque, row, text};
use iced::{Border, Color, Element, Length, Padding, Shadow, Vector, alignment};

/// tracks the open/closed state of a reusable dropdown menu.
#[derive(Debug, Clone, Default)]
pub struct DropdownMenuState {
    pub open_index: Option<usize>,
}

impl DropdownMenuState {
    /// Creates a new inactive dropdown menu state.
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
    pub label: String,
    pub action: Option<T>,
    pub shortcut: Option<String>,
    pub is_separator: bool,
}

impl<T> DropdownItem<T> {
    /// Creates a new actionable dropdown menu item.
    pub fn new(label: impl Into<String>, action: T) -> Self {
        Self {
            label: label.into(),
            action: Some(action),
            shortcut: None,
            is_separator: false,
        }
    }

    /// Creates a visual divider line separating menu item groups.
    pub fn separator() -> Self {
        Self {
            label: String::new(),
            action: None,
            shortcut: None,
            is_separator: true,
        }
    }

    /// attaches a keyboard shortcut hint (e.g. "Ctrl+Shift+P") shown next to the label.
    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }
}

/// configuration structure for a top-level menu column (e.g., "File" or "Help").
pub struct MenuGroup<T> {
    pub title: String,
    pub items: Vec<DropdownItem<T>>,
}

impl<T> MenuGroup<T> {
    /// Creates a new top-level menu group with the given title and items.
    pub fn new(title: impl Into<String>, items: Vec<DropdownItem<T>>) -> Self {
        Self {
            title: title.into(),
            items,
        }
    }
}

/// Computes the exact layout width for a menu header button based on its title.
pub fn menu_button_width(title: &str) -> f32 {
    let mut text_width: f32 = 0.0;
    for ch in title.chars() {
        text_width += match ch {
            'i' | 'l' | 't' | 'j' | 'r' | ' ' | '.' | ':' | '-' => 4.5,
            'f' | 'I' => 5.5,
            'm' | 'w' | 'M' | 'W' => 10.5,
            'A'..='Z' => 8.5,
            _ => 7.0,
        };
    }
    // 20.0 padding (10 left + 10 right) + 8.0 breathing room, with minimum width of 44.0
    (text_width + 28.0_f32).max(44.0).ceil()
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
        if item.is_separator {
            items_column = items_column.push(
                container(Space::new().height(1))
                    .width(Length::Fill)
                    .height(Length::Fixed(1.0))
                    .padding(Padding::from([3, 4]))
                    .style(|_theme: &iced::Theme| container::Style {
                        background: Some(crate::theme::colors().border_variant.into()),
                        ..Default::default()
                    }),
            );
            continue;
        }

        let mut item_row = row![text(item.label.clone()).size(13)]
            .width(Length::Fill)
            .spacing(16)
            .align_y(alignment::Vertical::Center);

        if let Some(shortcut) = &item.shortcut {
            item_row = item_row.push(
                text(shortcut.clone())
                    .size(11)
                    .color(crate::theme::colors().text_muted),
            );
        }

        if let Some(action) = &item.action {
            let action_clone = action.clone();
            items_column = items_column.push(
                button(item_row)
                    .width(Length::Fill)
                    .padding([5, 10])
                    .style(|theme: &iced::Theme, status| {
                        let colors = crate::theme::colors();
                        match status {
                            iced::widget::button::Status::Hovered
                            | iced::widget::button::Status::Pressed => button::Style {
                                background: Some(
                                    Color::from_rgba(
                                        colors.text_accent.r,
                                        colors.text_accent.g,
                                        colors.text_accent.b,
                                        0.12,
                                    )
                                    .into(),
                                ),
                                text_color: colors.text_accent,
                                border: Border {
                                    radius: 4.0.into(),
                                    ..Default::default()
                                },
                                ..button::text(theme, status)
                            },
                            _ => button::Style {
                                text_color: colors.text,
                                border: Border {
                                    radius: 4.0.into(),
                                    ..Default::default()
                                },
                                ..button::text(theme, status)
                            },
                        }
                    })
                    .on_press(DropdownMessage::TriggerAction(action_clone)),
            );
        }
    }

    let dropdown_panel =
        container(items_column)
            .width(230)
            .padding(5)
            .style(|_theme: &iced::Theme| {
                let colors = crate::theme::colors();
                container::Style {
                    background: Some(colors.elevated_surface_background.into()),
                    text_color: Some(colors.text),
                    border: Border {
                        color: colors.border_variant,
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    shadow: Shadow {
                        color: Color::from_rgba(0.0, 0.0, 0.0, 0.38),
                        offset: Vector::new(0.0, 6.0),
                        blur_radius: 20.0,
                    },
                    ..Default::default()
                }
            });

    // Compute cumulative horizontal offset matching each header button's position
    let mut horizontal_offset: f32 = 10.0;
    for group in &groups[..open_idx] {
        horizontal_offset += menu_button_width(&group.title) + 2.0;
    }

    let overlay_layer = column![
        container(text("")).height(30),
        row![
            container(text("")).width(horizontal_offset),
            opaque(dropdown_panel)
        ]
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    Some(overlay_layer.into())
}

/// renders the horizontal menu bar strip with active group highlights.
pub fn render_menu_bar<'a, T: 'static + Clone>(
    state: &DropdownMenuState,
    groups: &[MenuGroup<T>],
) -> Element<'a, DropdownMessage<T>> {
    let mut menu_row = row![].spacing(2).align_y(alignment::Vertical::Center);

    for (group_idx, group) in groups.iter().enumerate() {
        let is_open = state.open_index == Some(group_idx);
        let button_w = menu_button_width(&group.title);
        let header_button = button(
            container(text(group.title.clone()).size(13).font(iced::Font {
                weight: if is_open {
                    iced::font::Weight::Bold
                } else {
                    iced::font::Weight::Normal
                },
                ..iced::Font::DEFAULT
            }))
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Center),
        )
        .width(Length::Fixed(button_w))
        .padding([4, 10])
        .style(move |theme: &iced::Theme, status| {
            let colors = crate::theme::colors();
            if is_open {
                button::Style {
                    background: Some(
                        Color::from_rgba(
                            colors.text_accent.r,
                            colors.text_accent.g,
                            colors.text_accent.b,
                            0.15,
                        )
                        .into(),
                    ),
                    text_color: colors.text_accent,
                    border: Border {
                        radius: 4.0.into(),
                        width: 1.0,
                        color: Color::from_rgba(
                            colors.text_accent.r,
                            colors.text_accent.g,
                            colors.text_accent.b,
                            0.35,
                        ),
                    },
                    ..button::text(theme, status)
                }
            } else {
                button::Style {
                    text_color: colors.text,
                    border: Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..button::text(theme, status)
                }
            }
        })
        .on_press(DropdownMessage::Toggle(group_idx));

        menu_row = menu_row.push(header_button);
    }

    container(menu_row)
        .width(Length::Fill)
        .padding([3, 10])
        .style(|_theme| container::Style {
            background: Some(crate::theme::colors().title_bar_background.into()),
            border: Border {
                width: 1.0,
                color: crate::theme::colors().border_variant,
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}
