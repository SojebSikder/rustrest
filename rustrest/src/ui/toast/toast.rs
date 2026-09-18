use crate::message::Message;
use crate::ui::spinner::spinner;
use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Border, Color, Length};
use std::time::{Duration, Instant};

pub const TOAST_DURATION: Duration = Duration::from_secs(4);
pub const TOAST_DURATION_LONG: Duration = Duration::from_secs(15);

/// Shows a toast and returns the `Task` that dismisses it once its duration elapses.
pub fn show_and_schedule(
    manager: &mut ToastManager,
    message: impl Into<String>,
    status: ToastStatus,
    duration: Duration,
) -> iced::Task<Message> {
    let (id, duration) = manager.show(message, status, duration);
    iced::Task::perform(
        async move {
            tokio::time::sleep(duration).await;
            id
        },
        Message::DismissToast,
    )
}

/// Same as `show_and_schedule`, but the toast shows an animated spinner
/// glyph next to the message - for toasts announcing a still-running
/// operation (e.g. "Downloading update...") rather than a finished one.
pub fn show_pending_and_schedule(
    manager: &mut ToastManager,
    message: impl Into<String>,
    duration: Duration,
) -> (usize, iced::Task<Message>) {
    let (id, duration) = manager.show_pending(message, duration);
    let task = iced::Task::perform(
        async move {
            tokio::time::sleep(duration).await;
            id
        },
        Message::DismissToast,
    );
    (id, task)
}

/// Shows a toast that persists until explicitly dismissed (no auto-close timer)
pub fn show_sticky(
    manager: &mut ToastManager,
    message: impl Into<String>,
    status: ToastStatus,
) -> iced::Task<Message> {
    manager.show_sticky(message, status);
    iced::Task::none()
}

/// Same as `show_sticky`, but renders the pending spinner/progress-ring
/// styling - for a long-running operation with no fixed timer
pub fn show_sticky_pending(manager: &mut ToastManager, message: impl Into<String>) -> usize {
    manager.show_sticky_pending(message)
}

/// Same as `show_and_schedule`, but with an action button
pub fn show_with_action_and_schedule(
    manager: &mut ToastManager,
    message: impl Into<String>,
    status: ToastStatus,
    duration: Duration,
    action_label: impl Into<String>,
) -> (usize, iced::Task<Message>) {
    let (id, duration) = manager.show_with_action(message, status, duration, action_label);
    let task = iced::Task::perform(
        async move {
            tokio::time::sleep(duration).await;
            id
        },
        Message::DismissToast,
    );
    (id, task)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastStatus {
    Success,
    Error,
    Info,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub id: usize,
    pub message: String,
    pub status: ToastStatus,
    pub expires_at: Instant,
    pub action_label: Option<String>,
    pub pending: bool,
    /// fraction (0.0..=1.0) shown as a circular progress ring
    pub download_progress: Option<f32>,
    pub sticky: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ToastManager {
    toasts: Vec<Toast>,
    next_toast_id: usize,
}

impl ToastManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        self.toasts
            .retain(|toast| toast.sticky || toast.expires_at > now);
    }

    /// whether any currently-shown toast is animating a spinner, so the
    /// spinner-tick subscription knows to keep running.
    pub fn has_pending(&self) -> bool {
        self.toasts.iter().any(|t| t.pending)
    }

    pub fn show(
        &mut self,
        message: impl Into<String>,
        status: ToastStatus,
        duration: Duration,
    ) -> (usize, Duration) {
        self.show_internal(message, status, duration, None, false, false)
    }

    pub fn show_with_action(
        &mut self,
        message: impl Into<String>,
        status: ToastStatus,
        duration: Duration,
        action_label: impl Into<String>,
    ) -> (usize, Duration) {
        self.show_internal(
            message,
            status,
            duration,
            Some(action_label.into()),
            false,
            false,
        )
    }

    /// shows a toast with an animated spinner glyph, for an operation
    /// that's still running rather than one that's already finished.
    pub fn show_pending(
        &mut self,
        message: impl Into<String>,
        duration: Duration,
    ) -> (usize, Duration) {
        self.show_internal(message, ToastStatus::Info, duration, None, true, false)
    }

    /// shows a toast that persists until explicitly dismissed.
    pub fn show_sticky(&mut self, message: impl Into<String>, status: ToastStatus) -> usize {
        self.show_internal(message, status, Duration::ZERO, None, false, true)
            .0
    }

    /// shows a pending (spinner/progress-ring) toast that persists until
    /// explicitly dismissed.
    pub fn show_sticky_pending(&mut self, message: impl Into<String>) -> usize {
        self.show_internal(message, ToastStatus::Info, Duration::ZERO, None, true, true)
            .0
    }

    fn show_internal(
        &mut self,
        message: impl Into<String>,
        status: ToastStatus,
        mut duration: Duration,
        action_label: Option<String>,
        pending: bool,
        sticky: bool,
    ) -> (usize, Duration) {
        let id = self.next_toast_id;
        self.next_toast_id += 1;
        if duration.is_zero() {
            duration = Duration::from_secs(5);
        }
        let expires_at = Instant::now() + duration;
        self.toasts.push(Toast {
            id,
            message: message.into(),
            status,
            expires_at,
            action_label,
            pending,
            download_progress: None,
            sticky,
        });
        (id, duration)
    }

    pub fn dismiss(&mut self, id: usize) {
        self.toasts.retain(|toast| toast.id != id);
    }

    /// updates the message and fill fraction (0.0..=1.0) of a pending
    /// toast's circular progress ring
    pub fn set_download_progress(&mut self, id: usize, message: impl Into<String>, progress: f32) {
        if let Some(toast) = self.toasts.iter_mut().find(|toast| toast.id == id) {
            toast.message = message.into();
            toast.download_progress = Some(progress.clamp(0.0, 1.0));
        }
    }

    pub fn view<'a, Message>(
        &'a self,
        spinner_tick: u64,
        on_dismiss: impl Fn(usize) -> Message + 'a,
        on_action: impl Fn(usize) -> Message + 'a,
    ) -> iced::Element<'a, Message>
    where
        Message: Clone + 'a,
    {
        let mut toast_list = column![].spacing(10).align_x(Alignment::End);
        for toast in &self.toasts {
            let border_color = match toast.status {
                ToastStatus::Success => Color::from_rgb(0.1, 0.7, 0.1),
                ToastStatus::Error => Color::from_rgb(0.8, 0.1, 0.1),
                ToastStatus::Info => Color::from_rgb(0.1, 0.5, 0.8),
            };
            let dismiss_id = toast.id;
            let action_id = toast.id;

            let mut content = row![].spacing(10).align_y(Alignment::Center);
            if let Some(progress) = toast.download_progress {
                content = content.push(crate::ui::progress_ring::progress_ring(progress, 16.0));
            } else if toast.pending {
                content = content.push(spinner(spinner_tick));
            }
            content = content.push(text(&toast.message).width(Length::Fill));

            if let Some(label) = &toast.action_label {
                content = content.push(
                    button(text(label.clone()))
                        .on_press(on_action(action_id))
                        .padding(5),
                );
            }

            content = content.push(button("✕").on_press(on_dismiss(dismiss_id)).padding(5));

            let toast_ui = container(content)
                .width(300)
                .padding(12)
                .style(move |_theme| container::Style {
                    background: Some(Color::from_rgb(0.15, 0.15, 0.15).into()),
                    border: Border {
                        color: border_color,
                        width: 2.0,
                        radius: 4.0.into(),
                    },
                    text_color: Some(Color::WHITE),
                    ..Default::default()
                });
            toast_list = toast_list.push(toast_ui);
        }
        // wrap the list in a full-screen container pinned to the bottom right corner
        container(toast_list)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::End)
            .align_y(Alignment::End)
            .padding(20)
            .into()
    }
}
