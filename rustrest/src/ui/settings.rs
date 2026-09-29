use crate::app::Rustrest;
use crate::message::Message;
use crate::theme::{Appearance, ThemeMode, ThemeSelection};
use crate::ui::modal::{card, muted_text_color};
use iced::widget::{button, checkbox, column, container, pick_list, row, text};
use iced::{Alignment, Font, Length, Theme};

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
                .style(if app.settings.settings_tab == tab {
                    button::primary
                } else {
                    button::text
                }),
        );
    }

    let tab_content = match app.settings.settings_tab {
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

    card(body, 560.0)
}

fn section_label(label: &str) -> iced::widget::Text<'_> {
    text(label).size(12).style(|theme: &Theme| text::Style {
        color: Some(muted_text_color(theme)),
    })
}

fn view_theme_tab(app: &Rustrest) -> iced::Element<'_, Message> {
    let registry = &app.theme.registry;
    let selection = &app.theme.selection;

    let mut modes = row![].spacing(6);
    for mode in ThemeMode::ALL {
        modes = modes.push(
            button(text(mode.label()).size(13))
                .on_press(Message::ThemeModeSelected(mode))
                .padding([6, 12])
                .style(if selection.mode() == Some(mode) {
                    button::primary
                } else {
                    button::secondary
                }),
        );
    }

    let picker = |label: &'static str,
                  names: Vec<String>,
                  selected: &str,
                  on_select: fn(String) -> Message| {
        row![
            text(label).size(13).width(Length::Fixed(90.0)),
            pick_list(names, Some(selected.to_string()), on_select)
                .text_size(13)
                .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
    };

    let pickers: iced::Element<'_, Message> = match selection {
        ThemeSelection::Dynamic { light, dark, .. } => column![
            picker(
                "Light theme",
                registry.names(Some(Appearance::Light)),
                light,
                |name| Message::ThemeSelectedFor(Appearance::Light, name),
            ),
            picker(
                "Dark theme",
                registry.names(Some(Appearance::Dark)),
                dark,
                |name| Message::ThemeSelectedFor(Appearance::Dark, name),
            ),
        ]
        .spacing(8)
        .into(),
        ThemeSelection::Static(name) => {
            picker("Theme", registry.names(None), name, Message::ThemeSelected).into()
        }
    };

    let actions = column![
        row![
            button(text("Browse Themes...").size(13))
                .on_press(Message::ToggleThemeSelector)
                .padding([6, 12])
                .style(button::secondary),
            text("Ctrl+K Ctrl+T")
                .size(11)
                .style(|theme: &Theme| text::Style {
                    color: Some(muted_text_color(theme)),
                }),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        row![
            button(text("Import Theme...").size(13))
                .on_press(Message::ImportThemePressed)
                .padding([6, 12])
                .style(button::secondary),
            button(text("Open Themes Folder").size(13))
                .on_press(Message::OpenThemesFolderPressed)
                .padding([6, 12])
                .style(button::secondary),
            button(text("Reload").size(13))
                .on_press(Message::ReloadThemesPressed)
                .padding([6, 12])
                .style(button::secondary),
        ]
        .spacing(6),
    ]
    .spacing(8);

    let mut content = column![
        section_label("Appearance"),
        modes,
        section_label("Theme"),
        pickers,
        actions,
    ]
    .spacing(10);

    for error in &registry.errors {
        content = content.push(
            text(error.clone())
                .size(11)
                .style(|theme: &Theme| text::Style {
                    color: Some(crate::ui::modal::danger_text_color(theme)),
                }),
        );
    }

    content.into()
}

fn view_general_tab(app: &Rustrest) -> iced::Element<'_, Message> {
    column![
        text("General").size(12).style(|theme: &Theme| text::Style {
            color: Some(muted_text_color(theme)),
        }),
        checkbox(app.settings.close_on_outside_click)
            .label("Close windows by clicking outside")
            .on_toggle(Message::CloseOnOutsideClickToggled),
    ]
    .spacing(10)
    .into()
}
