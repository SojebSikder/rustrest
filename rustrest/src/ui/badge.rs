use iced::widget::{container, text};
use iced::{Alignment, Element, Font, Length, Padding};

/// Returns a modern, distinct accent color for HTTP methods.
pub fn http_method_color(method: &str) -> iced::Color {
    match method.trim().to_uppercase().as_str() {
        "GET" => iced::Color::from_rgb(0.063, 0.725, 0.506), // emerald green (#10b981)
        "POST" => iced::Color::from_rgb(0.961, 0.620, 0.043), // warm amber (#f59e0b)
        "PUT" => iced::Color::from_rgb(0.231, 0.510, 0.965), // vibrant blue (#3b82f6)
        "DELETE" => iced::Color::from_rgb(0.937, 0.267, 0.267), // clean red (#ef4444)
        "PATCH" => iced::Color::from_rgb(0.659, 0.333, 0.965), // purple (#a855f7)
        "HEAD" => iced::Color::from_rgb(0.024, 0.714, 0.831), // cyan (#06b6d4)
        "OPTIONS" => iced::Color::from_rgb(0.450, 0.510, 0.600), // slate (#718096)
        _ => iced::Color::from_rgb(0.550, 0.550, 0.600),     // neutral gray
    }
}

/// Renders a compact, color-coded HTTP method badge with a fixed width for perfect column alignment.
pub fn method_badge<'a, Message: 'a>(
    method: &str,
    fixed_width: Option<f32>,
) -> Element<'a, Message> {
    let color = http_method_color(method);
    let label = match method.trim().to_uppercase().as_str() {
        "" => "GET".to_string(),
        m => m.to_string(),
    };

    let badge_text = text(label)
        .size(10)
        .font(Font {
            weight: iced::font::Weight::Bold,
            ..Font::DEFAULT
        })
        .color(color);

    let mut badge_container = container(badge_text)
        .padding(Padding::from([2, 5]))
        .align_x(Alignment::Center)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Color::from_rgba(color.r, color.g, color.b, 0.12).into()),
            border: iced::Border {
                radius: 4.0.into(),
                width: 1.0,
                color: iced::Color::from_rgba(color.r, color.g, color.b, 0.28),
            },
            ..Default::default()
        });

    if let Some(w) = fixed_width {
        badge_container = badge_container.width(Length::Fixed(w));
    }

    badge_container.into()
}

/// Returns the standard canonical HTTP status text for a status code.
pub fn status_code_reason(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        101 => "Switching Protocols",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    }
}

/// Formats a byte size nicely (B, KB, MB).
pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Formats a duration nicely (ms, s).
pub fn format_duration(ms: u128) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.2} s", ms as f64 / 1000.0)
    }
}
