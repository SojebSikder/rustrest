use crate::message::Message;
use crate::ui::modal::{card, muted_text_color};
use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Element, Font, Length, Theme};
use rustrest_core::http::PhaseTimings;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ResponseTimingModalState {
    pub status: u16,
    pub timings: PhaseTimings,
    pub request_size: u64,
    pub response_size: u64,
}

fn fmt_ms(d: Duration) -> String {
    let micros = d.as_micros();
    if micros < 1000 {
        format!("{} \u{b5}s", micros)
    } else {
        format!("{:.2} ms", d.as_secs_f64() * 1000.0)
    }
}

fn fmt_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{} B", bytes)
    } else if b < MB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{:.2} MB", b / MB)
    }
}

fn phase_row<'a>(label: &'a str, duration: Duration) -> Element<'a, Message> {
    row![
        text(label).size(13).width(Length::FillPortion(2)),
        text(fmt_ms(duration))
            .size(13)
            .font(Font::MONOSPACE)
            .width(Length::FillPortion(1)),
    ]
    .align_y(Alignment::Center)
    .into()
}

fn size_row<'a>(label: &'a str, bytes: u64) -> Element<'a, Message> {
    row![
        text(label).size(13).width(Length::FillPortion(2)),
        text(fmt_bytes(bytes))
            .size(13)
            .font(Font::MONOSPACE)
            .width(Length::FillPortion(1)),
    ]
    .align_y(Alignment::Center)
    .into()
}

pub fn view_response_timing_modal(state: &ResponseTimingModalState) -> Element<'_, Message> {
    let title = text("Response Time").size(18).font(Font {
        weight: iced::font::Weight::Bold,
        ..Font::DEFAULT
    });

    let total = state.timings.total();
    let subtitle = text(format!(
        "Status {} \u{2022} Total {}",
        state.status,
        fmt_ms(total)
    ))
    .size(12)
    .style(|theme: &Theme| text::Style {
        color: Some(muted_text_color(theme)),
    });

    let timings_section = column![
        phase_row("Prepare", state.timings.prepare),
        phase_row("Socket Initialization", state.timings.socket_initialization),
        phase_row("DNS Lookup", state.timings.dns_lookup),
        phase_row("TCP Handshake", state.timings.tcp_handshake),
        phase_row("SSL Handshake", state.timings.ssl_handshake),
        phase_row("Waiting (TTFB)", state.timings.waiting),
        phase_row("Download", state.timings.download),
        phase_row("Process", state.timings.process),
    ]
    .spacing(6);

    let sizes_section = column![
        size_row("Request Size", state.request_size),
        size_row("Response Size", state.response_size),
    ]
    .spacing(6);

    let close_btn = button(text("Close").size(14))
        .on_press(Message::CloseResponseTimingModal)
        .padding([8, 16])
        .style(button::secondary);

    let body = column![
        title,
        subtitle,
        container(timings_section)
            .padding(12)
            .width(Length::Fill)
            .style(container::bordered_box),
        container(sizes_section)
            .padding(12)
            .width(Length::Fill)
            .style(container::bordered_box),
        container(close_btn)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right),
    ]
    .spacing(16)
    .padding(24);

    card(body, 360.0)
}
