use crate::ui::tokens::{RADIUS_MD, RADIUS_SM, tint};
use iced::widget::{Space, button, column, container, mouse_area, opaque, row, text};
use iced::{Border, Color, Element, Length, Padding, Shadow, Vector, alignment};

/// tracks the open/closed state of a reusable dropdown menu.
#[derive(Debug, Clone, Default)]
pub struct DropdownMenuState {
    pub open_index: Option<usize>,
    /// index (within the open group) of the item whose submenu is expanded.
    pub open_submenu: Option<usize>,
}

impl DropdownMenuState {
    /// Creates a new inactive dropdown menu state.
    pub fn new() -> Self {
        Self {
            open_index: None,
            open_submenu: None,
        }
    }

    /// Handles the internal state mutation when a menu item or header is clicked.
    pub fn update<T: Clone>(&mut self, message: DropdownMessage<T>) -> Option<T> {
        match message {
            DropdownMessage::Toggle(index) => {
                self.open_submenu = None;
                if self.open_index == Some(index) {
                    self.open_index = None;
                } else {
                    self.open_index = Some(index);
                }
                None
            }
            DropdownMessage::Close => {
                self.open_index = None;
                self.open_submenu = None;
                None
            }
            DropdownMessage::TriggerAction(action) => {
                self.open_index = None; // Automatically close menu on action selection
                self.open_submenu = None;
                Some(action)
            }
            DropdownMessage::HoverSubmenu(index) => {
                self.open_submenu = index;
                None
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum DropdownMessage<T> {
    Toggle(usize),
    Close,
    TriggerAction(T),
    /// the pointer entered a row: `Some` for a submenu parent, `None` otherwise.
    HoverSubmenu(Option<usize>),
}

/// represents an individual actionable item inside a dropdown list.
pub struct DropdownItem<T> {
    pub label: String,
    pub action: Option<T>,
    pub shortcut: Option<String>,
    pub is_separator: bool,
    /// nested items shown in a flyout panel when this item is hovered.
    pub children: Vec<DropdownItem<T>>,
}

impl<T> DropdownItem<T> {
    /// Creates a new actionable dropdown menu item.
    pub fn new(label: impl Into<String>, action: T) -> Self {
        Self {
            label: label.into(),
            action: Some(action),
            shortcut: None,
            is_separator: false,
            children: Vec::new(),
        }
    }

    /// Creates an item that opens a flyout of nested items on hover.
    pub fn submenu(label: impl Into<String>, children: Vec<DropdownItem<T>>) -> Self {
        Self {
            label: label.into(),
            action: None,
            shortcut: None,
            is_separator: false,
            children,
        }
    }

    /// Creates a visual divider line separating menu item groups.
    pub fn separator() -> Self {
        Self {
            label: String::new(),
            action: None,
            shortcut: None,
            is_separator: true,
            children: Vec::new(),
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

/// Estimates the rendered width of a size-13 label.
fn label_width(label: &str) -> f32 {
    label
        .chars()
        .map(|ch| match ch {
            'i' | 'l' | 't' | 'j' | 'r' | ' ' | '.' | ':' | '-' => 4.5,
            'f' | 'I' => 5.5,
            'm' | 'w' | 'M' | 'W' => 10.5,
            'A'..='Z' => 8.5,
            _ => 7.0,
        })
        .sum()
}

/// Computes the exact layout width for a menu header button based on its title.
pub fn menu_button_width(title: &str) -> f32 {
    // 20.0 padding (10 left + 10 right) + 8.0 breathing room, with minimum width of 44.0
    (label_width(title) + 28.0_f32).max(44.0).ceil()
}

/// Width of a dropdown panel: wide enough for its longest row, never below
/// `PANEL_MIN_WIDTH`.
fn panel_width<T>(items: &[DropdownItem<T>]) -> f32 {
    items
        .iter()
        .filter(|item| !item.is_separator)
        .map(|item| {
            let trailing = if !item.children.is_empty() {
                16.0
            } else if let Some(shortcut) = &item.shortcut {
                // shortcuts render at size 11 instead of 13
                label_width(shortcut) * 11.0 / 13.0 + 8.0
            } else {
                0.0
            };
            // 20.0 row padding + 12.0 panel padding + 16.0 breathing room
            label_width(&item.label) + trailing + 48.0
        })
        .fold(PANEL_MIN_WIDTH, f32::max)
        .ceil()
}

/// Style for the floating panel that hosts a menu or context dropdown: elevated
/// surface, hairline border, rounded corners and a soft drop shadow.
pub fn panel_style(_theme: &iced::Theme) -> container::Style {
    let colors = crate::theme::colors();
    container::Style {
        background: Some(colors.elevated_surface_background.into()),
        text_color: Some(colors.text),
        border: Border {
            color: colors.border_variant,
            width: 1.0,
            radius: RADIUS_MD.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.38),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 20.0,
        },
        ..Default::default()
    }
}

/// Style for a dropdown row: the same rounded shape in every state, with a soft
/// accent wash on hover/press so the targeted row reads clearly.
pub fn item_style(theme: &iced::Theme, status: button::Status) -> button::Style {
    let colors = crate::theme::colors();
    let highlighted = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: if highlighted {
            Some(tint(colors.text_accent, 0.12).into())
        } else {
            None
        },
        text_color: if highlighted {
            colors.text_accent
        } else {
            colors.text
        },
        border: Border {
            radius: RADIUS_SM.into(),
            ..Default::default()
        },
        ..button::text(theme, status)
    }
}

/// Style for a top-level menu-bar button: the open group is filled with an
/// accent wash, any other group gets a subtle neutral wash on hover.
fn header_style(is_open: bool, theme: &iced::Theme, status: button::Status) -> button::Style {
    let colors = crate::theme::colors();
    if is_open {
        button::Style {
            background: Some(tint(colors.text_accent, 0.15).into()),
            text_color: colors.text_accent,
            border: Border {
                radius: RADIUS_SM.into(),
                width: 1.0,
                color: tint(colors.text_accent, 0.35),
            },
            ..button::text(theme, status)
        }
    } else {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: if hovered {
                Some(colors.element_hover.into())
            } else {
                None
            },
            text_color: colors.text,
            border: Border {
                radius: RADIUS_SM.into(),
                ..Default::default()
            },
            ..button::text(theme, status)
        }
    }
}

// fixed row metrics so a submenu flyout can be aligned with its parent row
const ITEM_HEIGHT: f32 = 29.0;
const SEPARATOR_HEIGHT: f32 = 9.0;
const ITEM_SPACING: f32 = 2.0;
const PANEL_MIN_WIDTH: f32 = 230.0;

/// builds one dropdown panel. `hover_tracking` makes each row report hover so
/// the panel can open/close submenu flyouts.
fn render_panel<'a, T: 'static + Clone>(
    items: &[DropdownItem<T>],
    open_submenu: Option<usize>,
    hover_tracking: bool,
) -> Element<'a, DropdownMessage<T>> {
    let mut items_column = column![].spacing(ITEM_SPACING);

    for (idx, item) in items.iter().enumerate() {
        if item.is_separator {
            items_column = items_column.push(
                container(
                    container(Space::new())
                        .width(Length::Fill)
                        .height(Length::Fixed(1.0))
                        .style(|_theme: &iced::Theme| container::Style {
                            background: Some(crate::theme::colors().border_variant.into()),
                            ..Default::default()
                        }),
                )
                .width(Length::Fill)
                .height(Length::Fixed(SEPARATOR_HEIGHT))
                .padding(Padding::from([4, 6])),
            );
            continue;
        }

        let has_children = !item.children.is_empty();
        let message = if has_children {
            DropdownMessage::HoverSubmenu(Some(idx))
        } else if let Some(action) = item.action.clone() {
            DropdownMessage::TriggerAction(action)
        } else {
            continue;
        };

        // rows have a fixed height, so a wrapped label would spill out of its row
        let label = text(item.label.clone())
            .size(13)
            .wrapping(text::Wrapping::None);
        let mut content = row![label]
            .width(Length::Fill)
            .spacing(8)
            .align_y(alignment::Vertical::Center);

        // a filler keeps every shortcut / submenu arrow pinned to the same right edge
        let trailing = if has_children {
            Some("▸".to_string())
        } else {
            item.shortcut.clone()
        };
        if let Some(trailing) = trailing {
            content = content.push(Space::new().width(Length::Fill));
            content = content.push(
                text(trailing)
                    .size(11)
                    .color(crate::theme::colors().text_muted),
            );
        }

        // keep a submenu parent highlighted while its flyout is open
        let is_expanded = has_children && open_submenu == Some(idx);
        let row_button = button(content)
            .width(Length::Fill)
            .height(Length::Fixed(ITEM_HEIGHT))
            .padding([6, 10])
            .style(move |theme, status| {
                let status = if is_expanded {
                    button::Status::Hovered
                } else {
                    status
                };
                item_style(theme, status)
            })
            .on_press(message);

        if hover_tracking {
            let hover = DropdownMessage::HoverSubmenu(has_children.then_some(idx));
            items_column = items_column.push(mouse_area(row_button).on_enter(hover));
        } else {
            items_column = items_column.push(row_button);
        }
    }

    container(items_column)
        .width(panel_width(items))
        .padding(6)
        .style(panel_style)
        .into()
}

/// renders the floating dropdown overlay panel if one is open.
pub fn render_menu_overlay<'a, T: 'static + Clone>(
    state: &DropdownMenuState,
    groups: &[MenuGroup<T>],
) -> Option<Element<'a, DropdownMessage<T>>> {
    let open_idx = state.open_index?;
    let target_group = groups.get(open_idx)?;

    let dropdown_panel = render_panel(&target_group.items, state.open_submenu, true);

    // Compute cumulative horizontal offset matching each header button's position
    let mut horizontal_offset: f32 = 8.0;
    for group in &groups[..open_idx] {
        horizontal_offset += menu_button_width(&group.title) + 2.0;
    }

    let mut panels = row![opaque(dropdown_panel)];

    if let Some((sub_idx, parent)) = state
        .open_submenu
        .and_then(|idx| Some((idx, target_group.items.get(idx)?)))
        .filter(|(_, item)| !item.children.is_empty())
    {
        // shift the flyout down so its first row lines up with the parent row
        let vertical_offset: f32 = target_group.items[..sub_idx]
            .iter()
            .map(|item| {
                let height = if item.is_separator {
                    SEPARATOR_HEIGHT
                } else {
                    ITEM_HEIGHT
                };
                height + ITEM_SPACING
            })
            .sum();
        panels = panels.push(column![
            Space::new().height(vertical_offset),
            opaque(render_panel(&parent.children, None, false))
        ]);
    }

    let overlay_layer = column![
        container(text("")).height(crate::ui::titlebar::TITLEBAR_HEIGHT),
        row![container(text("")).width(horizontal_offset), panels]
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
        let header_label = container(text(group.title.clone()).size(13))
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Center);

        let header_button = button(header_label)
            .width(Length::Fixed(button_w))
            .padding([5, 10])
            .style(move |theme, status| header_style(is_open, theme, status))
            .on_press(DropdownMessage::Toggle(group_idx));

        menu_row = menu_row.push(header_button);
    }

    container(menu_row)
        .width(Length::Shrink)
        .padding([0, 8])
        .style(|_theme| container::Style {
            background: Some(Color::TRANSPARENT.into()),
            ..Default::default()
        })
        .into()
}
