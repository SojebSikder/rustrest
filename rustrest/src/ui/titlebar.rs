//! Custom window title bar: draggable area, embedded menu strip,
//! and OS-style window controls (minimize / maximize / close).

use crate::app::Rustrest;
use crate::message::Message;
use iced::widget::{Space, button, column, container, mouse_area, row, stack, text};
use iced::{Alignment, Border, Color, Element, Length, Padding};

/// Height of the title bar in logical pixels - keep in sync with the
/// `padding.top` used in `main.rs`'s `base_layout`.
pub const TITLEBAR_HEIGHT: f32 = 36.0;

/// Renders the full-width custom titlebar.
///
/// * `app`        – app state (provides `is_window_maximized`)
/// * `menu_strip` – the already-rendered menu bar element
pub fn render_titlebar<'a>(
    app: &'a Rustrest,
    menu_strip: Element<'a, Message>,
) -> Element<'a, Message> {
    let colors = crate::theme::colors();

    // ── Right: window controls ─────────────────────────────────────────────────
    let neutral_hover = Color::from_rgba(colors.text.r, colors.text.g, colors.text.b, 0.12);
    let neutral_pressed = Color::from_rgba(colors.text.r, colors.text.g, colors.text.b, 0.22);
    let close_hover = Color::from_rgba(0.86, 0.21, 0.27, 0.90);
    let close_pressed = Color::from_rgba(0.72, 0.15, 0.20, 0.95);

    // Minimize: heavy horizontal bar, properly centered and thick
    let btn_minimize = win_btn(
        "━",
        20.0,
        neutral_hover,
        neutral_pressed,
        Message::TitleBarMinimizePressed,
    );

    // Maximize / Restore: large, clear box
    let maximize_icon = if app.is_window_maximized {
        "❐"
    } else {
        "□"
    };
    let maximize_size = if app.is_window_maximized { 20.0 } else { 22.0 };
    let btn_maximize = win_btn(
        maximize_icon,
        maximize_size,
        neutral_hover,
        neutral_pressed,
        Message::TitleBarMaximizePressed,
    );

    // Close: crisp multiplication cross
    let btn_close = win_btn_close(
        "✕",
        18.0,
        close_hover,
        close_pressed,
        Message::TitleBarClosePressed,
    );

    let controls = row![btn_minimize, btn_maximize, btn_close]
        .spacing(0)
        .align_y(Alignment::Center)
        .height(Length::Fill);

    // Assemble the bar: menu on left, draggable space in middle, notification
    // bell (while signed in to Rustrest Cloud) and controls on right
    let bar_content = row![menu_strip, Space::new().width(Length::Fill)]
        .push(super::notifications::bell(app))
        .push(controls)
        .width(Length::Fill)
        .height(Length::Fixed(TITLEBAR_HEIGHT - 1.0))
        .align_y(Alignment::Center);

    // mouse_area: drag anywhere not captured by a button moves the window;
    // double-click toggles maximize.
    let draggable: Element<'a, Message> = mouse_area(bar_content)
        .on_press(Message::TitleBarDragStarted)
        .on_double_click(Message::TitleBarMaximizePressed)
        .into();

    let bar_body = container(draggable)
        .width(Length::Fill)
        .height(Length::Fixed(TITLEBAR_HEIGHT - 1.0))
        .padding(Padding::ZERO)
        .style(|_theme| {
            let colors = crate::theme::colors();
            container::Style {
                background: Some(colors.title_bar_background.into()),
                ..Default::default()
            }
        });

    let bottom_divider = container(Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(|_theme| {
            let colors = crate::theme::colors();
            container::Style {
                background: Some(colors.border_variant.into()),
                ..Default::default()
            }
        });

    column![bar_body, bottom_divider]
        .width(Length::Fill)
        .height(Length::Fixed(TITLEBAR_HEIGHT))
        .into()
}

/// Window control button (minimize / maximize).
fn win_btn(
    icon: &'static str,
    font_size: f32,
    hover_bg: Color,
    pressed_bg: Color,
    msg: Message,
) -> Element<'static, Message> {
    let colors = crate::theme::colors();
    button(
        container(
            text(icon)
                .size(font_size)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center),
    )
    .padding(0)
    .width(Length::Fixed(46.0))
    .height(Length::Fill)
    .style(move |_theme: &iced::Theme, status| {
        let (bg, text_color) = match status {
            iced::widget::button::Status::Pressed => (Some(pressed_bg.into()), colors.text),
            iced::widget::button::Status::Hovered => (Some(hover_bg.into()), colors.text),
            _ => (None, colors.text_muted),
        };
        button::Style {
            background: bg,
            text_color,
            border: Border {
                radius: 0.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    })
    .on_press(msg)
    .into()
}

/// Close button — turns white-on-red on hover.
fn win_btn_close(
    icon: &'static str,
    font_size: f32,
    hover_bg: Color,
    pressed_bg: Color,
    msg: Message,
) -> Element<'static, Message> {
    let colors = crate::theme::colors();
    button(
        container(
            text(icon)
                .size(font_size)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center),
    )
    .padding(0)
    .width(Length::Fixed(46.0))
    .height(Length::Fill)
    .style(move |_theme: &iced::Theme, status| {
        let (bg, text_color) = match status {
            iced::widget::button::Status::Pressed => (Some(pressed_bg.into()), Color::WHITE),
            iced::widget::button::Status::Hovered => (Some(hover_bg.into()), Color::WHITE),
            _ => (None, colors.text_muted),
        };
        button::Style {
            background: bg,
            text_color,
            border: Border {
                radius: 0.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    })
    .on_press(msg)
    .into()
}

/// Renders a outline around the undecorated window.
pub fn render_window_border<'a>() -> Element<'a, Message> {
    let colors = crate::theme::colors();
    let border_color = colors.border;

    container(Space::new())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// Renders invisible edge and corner handles along the perimeter of the window
/// to allow drag-resizing an undecorated (`decorations: false`) window.
pub fn render_window_resize_handles<'a>() -> Element<'a, Message> {
    const HANDLE_THICKNESS: f32 = 5.0;
    const CORNER_SIZE: f32 = 14.0;

    // Left edge
    let left_edge = container(
        mouse_area(
            container(Space::new())
                .width(Length::Fixed(HANDLE_THICKNESS))
                .height(Length::Fill),
        )
        .interaction(iced::mouse::Interaction::ResizingHorizontally)
        .on_press(Message::WindowResizeDrag(iced::window::Direction::West)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::Start);

    // Right edge
    let right_edge = container(
        mouse_area(
            container(Space::new())
                .width(Length::Fixed(HANDLE_THICKNESS))
                .height(Length::Fill),
        )
        .interaction(iced::mouse::Interaction::ResizingHorizontally)
        .on_press(Message::WindowResizeDrag(iced::window::Direction::East)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::End);

    // Bottom edge
    let bottom_edge = container(
        mouse_area(
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fixed(HANDLE_THICKNESS)),
        )
        .interaction(iced::mouse::Interaction::ResizingVertically)
        .on_press(Message::WindowResizeDrag(iced::window::Direction::South)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_y(Alignment::End);

    // Top edge (excludes the top-right 140px window control button area)
    let top_row = row![
        mouse_area(
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fixed(HANDLE_THICKNESS)),
        )
        .interaction(iced::mouse::Interaction::ResizingVertically)
        .on_press(Message::WindowResizeDrag(iced::window::Direction::North)),
        Space::new().width(Length::Fixed(140.0)),
    ]
    .width(Length::Fill)
    .height(Length::Fixed(HANDLE_THICKNESS));

    let top_edge = container(top_row)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Start);

    // Bottom-Left corner
    let bottom_left_corner = container(
        mouse_area(
            container(Space::new())
                .width(Length::Fixed(CORNER_SIZE))
                .height(Length::Fixed(CORNER_SIZE)),
        )
        .interaction(iced::mouse::Interaction::ResizingHorizontally)
        .on_press(Message::WindowResizeDrag(
            iced::window::Direction::SouthWest,
        )),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::Start)
    .align_y(Alignment::End);

    // Bottom-Right corner
    let bottom_right_corner = container(
        mouse_area(
            container(Space::new())
                .width(Length::Fixed(CORNER_SIZE))
                .height(Length::Fixed(CORNER_SIZE)),
        )
        .interaction(iced::mouse::Interaction::ResizingHorizontally)
        .on_press(Message::WindowResizeDrag(
            iced::window::Direction::SouthEast,
        )),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::End)
    .align_y(Alignment::End);

    // Top-Left corner
    let top_left_corner = container(
        mouse_area(
            container(Space::new())
                .width(Length::Fixed(CORNER_SIZE))
                .height(Length::Fixed(CORNER_SIZE)),
        )
        .interaction(iced::mouse::Interaction::ResizingHorizontally)
        .on_press(Message::WindowResizeDrag(
            iced::window::Direction::NorthWest,
        )),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::Start)
    .align_y(Alignment::Start);

    stack![
        left_edge,
        right_edge,
        bottom_edge,
        top_edge,
        bottom_left_corner,
        bottom_right_corner,
        top_left_corner,
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
