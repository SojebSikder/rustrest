//! Window layout/panel-sizing state: the sidebar/request-pane/console/right-panel
//! resize drag, and the bottom console panel's own logs/collapsed state.

use super::Rustrest;
use crate::message::{Message, ResizeKind};
use iced::Task;

pub const SIDEBAR_WIDTH_RANGE: (f32, f32) = (180.0, 520.0);
pub const REQUEST_PANE_HEIGHT_RANGE: (f32, f32) = (120.0, 700.0);
pub const CONSOLE_PANEL_HEIGHT_RANGE: (f32, f32) = (120.0, 500.0);
pub const RIGHT_PANEL_WIDTH_RANGE: (f32, f32) = (260.0, 560.0);

pub struct ResizeDrag {
    pub kind: ResizeKind,
    pub start_cursor: iced::Point,
    pub start_size: f32,
}

#[derive(Default)]
pub struct LayoutState {
    pub sidebar_width: f32,
    pub request_pane_height: f32,
    pub resize_drag: Option<ResizeDrag>,
    pub console_logs: Vec<String>,
    pub console_collapsed: bool,
    pub console_panel_height: f32,
}

pub fn cursor_moved(app: &mut Rustrest, position: iced::Point) -> Task<Message> {
    if let Some(drag) = &app.layout.resize_drag {
        match drag.kind {
            ResizeKind::Sidebar => {
                let delta = position.x - drag.start_cursor.x;
                app.layout.sidebar_width =
                    (drag.start_size + delta).clamp(SIDEBAR_WIDTH_RANGE.0, SIDEBAR_WIDTH_RANGE.1);
            }
            ResizeKind::RequestPane => {
                let delta = position.y - drag.start_cursor.y;
                app.layout.request_pane_height = (drag.start_size + delta)
                    .clamp(REQUEST_PANE_HEIGHT_RANGE.0, REQUEST_PANE_HEIGHT_RANGE.1);
            }
            ResizeKind::ConsolePanel => {
                let delta = drag.start_cursor.y - position.y;
                app.layout.console_panel_height = (drag.start_size + delta)
                    .clamp(CONSOLE_PANEL_HEIGHT_RANGE.0, CONSOLE_PANEL_HEIGHT_RANGE.1);
            }
            ResizeKind::RightPanel => {
                let delta = drag.start_cursor.x - position.x;
                app.plugins.right_panel_width = (drag.start_size + delta)
                    .clamp(RIGHT_PANEL_WIDTH_RANGE.0, RIGHT_PANEL_WIDTH_RANGE.1);
            }
        }
    }
    app.cursor_position = position;
    Task::none()
}

pub fn resize_drag_started(app: &mut Rustrest, kind: ResizeKind) -> Task<Message> {
    let start_size = match kind {
        ResizeKind::Sidebar => app.layout.sidebar_width,
        ResizeKind::RequestPane => app.layout.request_pane_height,
        ResizeKind::ConsolePanel => app.layout.console_panel_height,
        ResizeKind::RightPanel => app.plugins.right_panel_width,
    };
    app.layout.resize_drag = Some(ResizeDrag {
        kind,
        start_cursor: app.cursor_position,
        start_size,
    });
    Task::none()
}

pub fn resize_drag_ended(app: &mut Rustrest) -> Task<Message> {
    app.layout.resize_drag = None;
    Task::none()
}

pub fn toggle_console_panel(app: &mut Rustrest) -> Task<Message> {
    app.layout.console_collapsed = !app.layout.console_collapsed;
    Task::none()
}

pub fn clear_console_logs(app: &mut Rustrest) -> Task<Message> {
    app.layout.console_logs.clear();
    Task::none()
}
