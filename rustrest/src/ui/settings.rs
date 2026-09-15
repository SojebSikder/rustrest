use crate::app::Rustrest;
use crate::message::Message;
use crate::ui::modal::{card, muted_text_color};
use iced::widget::{button, checkbox, column, container, row, text};
use iced::{Font, Length, Theme};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppTheme {
    Light,
    Dark,
}

impl AppTheme {
    pub const ALL: [AppTheme; 2] = [AppTheme::Light, AppTheme::Dark];

    pub fn label(&self) -> &'static str {
        match self {
            AppTheme::Light => "Light Theme",
            AppTheme::Dark => "Dark Theme",
        }
    }

    pub fn to_iced(self) -> iced::Theme {
        match self {
            AppTheme::Light => iced::Theme::Light,
            AppTheme::Dark => iced::Theme::Dark,
        }
    }
}

impl Default for AppTheme {
    fn default() -> Self {
        AppTheme::Dark
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Theme,
    General,
}

impl SettingsTab {
    pub const ALL: [SettingsTab; 2] = [SettingsTab::Theme, SettingsTab::General];

    pub fn label(&self) -> &'static str {
        match self {
            SettingsTab::Theme => "Theme",
            SettingsTab::General => "General",
        }
    }
}

impl Default for SettingsTab {
    fn default() -> Self {
        SettingsTab::Theme
    }
}

pub fn view_settings_modal(app: &Rustrest) -> iced::Element<'_, Message> {
    let title = text("Settings").size(18).font(Font {
        weight: iced::font::Weight::Bold,
        ..Font::DEFAULT
    });

    let mut nav = column![].spacing(4).width(Length::Fixed(140.0));
    for tab in SettingsTab::ALL {
        nav = nav.push(
            button(text(tab.label()).size(14))
                .on_press(Message::SettingsTabSelected(tab))
                .width(Length::Fill)
                .padding([8, 12])
                .style(if app.settings_tab == tab {
                    button::primary
                } else {
                    button::text
                }),
        );
    }

    let tab_content = match app.settings_tab {
        SettingsTab::Theme => view_theme_tab(app),
        SettingsTab::General => view_general_tab(app),
    };

    let close_btn = button(text("Close").size(14))
        .on_press(Message::CloseSettingsPressed)
        .padding([8, 16])
        .style(button::secondary);

    let body = column![
        title,
        row![nav, tab_content].spacing(20),
        container(close_btn)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right),
    ]
    .spacing(18)
    .padding(24);

    card(body, 460.0)
}

fn view_theme_tab(app: &Rustrest) -> iced::Element<'_, Message> {
    let mut list = column![].spacing(8).width(Length::Fill);
    for theme in AppTheme::ALL {
        list = list.push(
            button(text(theme.label()).size(14))
                .on_press(Message::ThemeSelected(theme))
                .width(Length::Fill)
                .padding([8, 12])
                .style(if app.theme == theme {
                    button::primary
                } else {
                    button::secondary
                }),
        );
    }

    column![
        text("Theme").size(12).style(|theme: &Theme| text::Style {
            color: Some(muted_text_color(theme)),
        }),
        list,
    ]
    .spacing(10)
    .into()
}

fn view_general_tab(app: &Rustrest) -> iced::Element<'_, Message> {
    column![
        text("General").size(12).style(|theme: &Theme| text::Style {
            color: Some(muted_text_color(theme)),
        }),
        checkbox(app.close_on_outside_click)
            .label("Close windows by clicking outside")
            .on_toggle(Message::CloseOnOutsideClickToggled),
    ]
    .spacing(10)
    .into()
}
