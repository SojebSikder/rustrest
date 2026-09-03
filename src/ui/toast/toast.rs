use crate::message::Message;
use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Border, Color, Length};
use std::time::{Duration, Instant};

pub const TOAST_DURATION: Duration = Duration::from_secs(4);

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
        self.toasts.retain(|toast| toast.expires_at > now);
    }

    pub fn show(
        &mut self,
        message: impl Into<String>,
        status: ToastStatus,
        duration: Duration,
    ) -> (usize, Duration) {
        self.show_internal(message, status, duration, None)
    }

    pub fn show_with_action(
        &mut self,
        message: impl Into<String>,
        status: ToastStatus,
        duration: Duration,
        action_label: impl Into<String>,
    ) -> (usize, Duration) {
        self.show_internal(message, status, duration, Some(action_label.into()))
    }

    fn show_internal(
        &mut self,
        message: impl Into<String>,
        status: ToastStatus,
        mut duration: Duration,
        action_label: Option<String>,
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
        });
        (id, duration)
    }

    pub fn dismiss(&mut self, id: usize) {
        self.toasts.retain(|toast| toast.id != id);
    }

    pub fn view<'a, Message>(
        &'a self,
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

            let mut content = row![text(&toast.message).width(Length::Fill)]
                .spacing(10)
                .align_y(Alignment::Center);

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
