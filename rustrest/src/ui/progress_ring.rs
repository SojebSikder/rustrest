//! A small circular "donut" progress indicator that fills clockwise from
//! the top as `progress` advances

use iced::widget::canvas::{self, Frame, LineCap, Path, Stroke};
use iced::{Color, Element, Length, Radians, Rectangle, Renderer, Theme, mouse};
use std::f32::consts::PI;

const TRACK_COLOR: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.15);
const FILL_COLOR: Color = Color::from_rgb(0.35, 0.65, 1.0);
const STROKE_WIDTH: f32 = 2.5;

#[derive(Debug, Clone, Copy)]
struct ProgressRing {
    progress: f32,
}

impl<Message> canvas::Program<Message> for ProgressRing {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let center = frame.center();
        let radius = (bounds.width.min(bounds.height) / 2.0) - STROKE_WIDTH / 2.0;

        let track = Path::circle(center, radius);
        frame.stroke(
            &track,
            Stroke::default()
                .with_color(TRACK_COLOR)
                .with_width(STROKE_WIDTH),
        );

        if self.progress > 0.0 {
            let start_angle = Radians(-PI / 2.0);
            let end_angle = Radians(-PI / 2.0 + self.progress * 2.0 * PI);
            let arc = Path::new(|builder| {
                builder.arc(canvas::path::Arc {
                    center,
                    radius,
                    start_angle,
                    end_angle,
                });
            });
            frame.stroke(
                &arc,
                Stroke::default()
                    .with_color(FILL_COLOR)
                    .with_width(STROKE_WIDTH)
                    .with_line_cap(LineCap::Round),
            );
        }

        vec![frame.into_geometry()]
    }
}

/// a fixed-size circular progress indicator, `progress` clamped to 0.0..=1.0.
pub fn progress_ring<'a, Message: 'a>(progress: f32, diameter: f32) -> Element<'a, Message> {
    iced::widget::canvas(ProgressRing {
        progress: progress.clamp(0.0, 1.0),
    })
    .width(Length::Fixed(diameter))
    .height(Length::Fixed(diameter))
    .into()
}
