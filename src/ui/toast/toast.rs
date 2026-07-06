use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Border, Color, Length};
use std::time::{Duration, Instant};

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
        mut duration: Duration,
    ) -> (usize, Duration) {
        let id = self.next_toast_id;
        self.next_toast_id += 1;

        if !duration.is_zero() {
            duration = Duration::from_secs(5);
        }

        let expires_at = Instant::now() + duration;

        self.toasts.push(Toast {
            id,
            message: message.into(),
            status,
            expires_at,
        });

        (id, duration)
    }

    pub fn dismiss(&mut self, id: usize) {
        self.toasts.retain(|toast| toast.id != id);
    }

    pub fn view<'a, Message>(
        &'a self,
        on_dismiss: impl Fn(usize) -> Message + 'a,
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
            let toast_ui = container(
                row![
                    text(&toast.message).width(Length::Fill),
                    button("✕").on_press(on_dismiss(dismiss_id)).padding(5)
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
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
