use crate::app::Rustrest;
use crate::message::Message;
use crate::theme::{Appearance, ThemeMode, ThemeSelection};
use crate::ui::modal::{card, muted_text_color};
use crate::ui::tokens::RADIUS_MD;
use iced::widget::{button, checkbox, column, container, pick_list, row, text, text_input};
use iced::{Alignment, Font, Length, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Theme,
    General,
    Proxy,
}

impl SettingsTab {
    pub const ALL: [SettingsTab; 3] =
        [SettingsTab::Theme, SettingsTab::General, SettingsTab::Proxy];

    pub fn label(&self) -> &'static str {
        match self {
            SettingsTab::Theme => "Theme",
            SettingsTab::General => "General",
            SettingsTab::Proxy => "Proxy",
        }
    }
}

/// one field of the Proxy settings tab changing
#[derive(Debug, Clone)]
pub enum ProxyEdit {
    Enabled(bool),
    Protocol(crate::http_client::ProxyProtocol),
    Host(String),
    Port(String),
    AuthEnabled(bool),
    Username(String),
    Password(String),
    Bypass(String),
    UseSystem(bool),
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

    let mut nav = column![].spacing(6).width(Length::Fixed(140.0));
    for tab in SettingsTab::ALL {
        let is_active = app.settings.settings_tab == tab;
        nav = nav.push(
            button(text(tab.label()).size(14))
                .on_press(Message::SettingsTabSelected(tab))
                .width(Length::Fill)
                .padding([8, 14])
                .style(move |theme: &Theme, status| {
                    let palette = theme.extended_palette();
                    if is_active {
                        let mut base = palette.primary.base.color;
                        base.a = 0.14;
                        button::Style {
                            background: Some(iced::Background::Color(base)),
                            text_color: palette.primary.base.color,
                            border: iced::Border {
                                color: palette.primary.base.color,
                                width: 1.0,
                                radius: RADIUS_MD.into(),
                            },
                            ..Default::default()
                        }
                    } else {
                        match status {
                            button::Status::Hovered => button::Style {
                                background: Some(iced::Background::Color(
                                    palette.background.weak.color,
                                )),
                                text_color: palette.background.weak.text,
                                border: iced::Border {
                                    radius: RADIUS_MD.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                            _ => button::Style {
                                background: None,
                                text_color: palette.background.base.text,
                                ..Default::default()
                            },
                        }
                    }
                }),
        );
    }

    let tab_content = match app.settings.settings_tab {
        SettingsTab::Theme => view_theme_tab(app),
        SettingsTab::General => view_general_tab(app),
        SettingsTab::Proxy => view_proxy_tab(app),
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

    card(body, 580.0)
}

fn section_label(label: &str) -> iced::widget::Text<'_> {
    text(label)
        .size(12)
        .font(Font {
            weight: iced::font::Weight::Bold,
            ..Font::DEFAULT
        })
        .style(|theme: &Theme| text::Style {
            color: Some(muted_text_color(theme)),
        })
}

fn view_theme_tab(app: &Rustrest) -> iced::Element<'_, Message> {
    let registry = &app.theme.registry;
    let selection = &app.theme.selection;

    let mut modes = row![].spacing(6);
    for mode in ThemeMode::ALL {
        let is_active = selection.mode() == Some(mode);
        modes = modes.push(
            button(text(mode.label()).size(13))
                .on_press(Message::ThemeModeSelected(mode))
                .padding([6, 14])
                .style(move |theme: &Theme, status| {
                    let palette = theme.extended_palette();
                    if is_active {
                        let mut base = palette.primary.base.color;
                        base.a = 0.14;
                        button::Style {
                            background: Some(iced::Background::Color(base)),
                            text_color: palette.primary.base.color,
                            border: iced::Border {
                                color: palette.primary.base.color,
                                width: 1.0,
                                radius: RADIUS_MD.into(),
                            },
                            ..Default::default()
                        }
                    } else {
                        match status {
                            button::Status::Hovered => button::Style {
                                background: Some(iced::Background::Color(
                                    palette.background.weak.color,
                                )),
                                text_color: palette.background.weak.text,
                                border: iced::Border {
                                    color: palette.background.weak.color,
                                    width: 1.0,
                                    radius: RADIUS_MD.into(),
                                },
                                ..Default::default()
                            },
                            _ => button::Style {
                                background: None,
                                text_color: palette.background.base.text,
                                border: iced::Border {
                                    color: palette.background.weak.color,
                                    width: 1.0,
                                    radius: RADIUS_MD.into(),
                                },
                                ..Default::default()
                            },
                        }
                    }
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

fn view_proxy_tab(app: &Rustrest) -> iced::Element<'_, Message> {
    let proxy = &app.settings.proxy;
    let hint = |s: &'static str| {
        text(s).size(11).style(|theme: &Theme| text::Style {
            color: Some(muted_text_color(theme)),
        })
    };
    let field = |label: &'static str, input: iced::Element<'static, Message>| {
        row![text(label).size(13).width(Length::Fixed(90.0)), input]
            .spacing(8)
            .align_y(Alignment::Center)
    };
    let input = |placeholder: &str, value: &str, on_input: fn(String) -> ProxyEdit| {
        text_input(placeholder, value)
            .on_input(move |v| Message::ProxySettingChanged(on_input(v)))
            .padding([6, 10])
            .size(13)
    };

    let mut content = column![
        section_label("Custom proxy"),
        checkbox(proxy.enabled)
            .label("Use a custom proxy for all requests")
            .on_toggle(|v| Message::ProxySettingChanged(ProxyEdit::Enabled(v))),
        field(
            "Proxy type",
            pick_list(
                crate::http_client::ProxyProtocol::ALL,
                Some(proxy.protocol),
                |p| Message::ProxySettingChanged(ProxyEdit::Protocol(p)),
            )
            .text_size(13)
            .width(Length::Fixed(120.0))
            .into(),
        ),
        field(
            "Server",
            row![
                input("proxy.example.com", &proxy.host, ProxyEdit::Host).width(Length::Fill),
                text(":").size(13),
                input("8080", &app.settings.proxy_port_input, ProxyEdit::Port)
                    .width(Length::Fixed(80.0)),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into(),
        ),
        checkbox(proxy.auth_enabled)
            .label("Proxy requires authentication")
            .on_toggle(|v| Message::ProxySettingChanged(ProxyEdit::AuthEnabled(v))),
    ]
    .spacing(10);

    if proxy.auth_enabled {
        content = content
            .push(field(
                "Username",
                input("", &proxy.username, ProxyEdit::Username)
                    .width(Length::Fill)
                    .into(),
            ))
            .push(field(
                "Password",
                input("", &proxy.password, ProxyEdit::Password)
                    .secure(true)
                    .width(Length::Fill)
                    .into(),
            ));
        if matches!(
            proxy.protocol,
            crate::http_client::ProxyProtocol::Socks4 | crate::http_client::ProxyProtocol::Socks4a
        ) {
            content = content.push(hint(
                "SOCKS4 has no passwords: only the username is sent, as the user ID.",
            ));
        }
    }

    content
        .push(field(
            "Bypass",
            input("localhost, *.internal", &proxy.bypass, ProxyEdit::Bypass)
                .width(Length::Fill)
                .into(),
        ))
        .push(hint(
            "Comma-separated hosts that skip the proxy. *.example.com matches subdomains.",
        ))
        .push(section_label("System proxy"))
        .push(
            checkbox(proxy.use_system)
                .label("Use system proxy when no custom proxy is enabled")
                .on_toggle(|v| Message::ProxySettingChanged(ProxyEdit::UseSystem(v))),
        )
        .push(hint(
            "Reads HTTP_PROXY, HTTPS_PROXY, ALL_PROXY and NO_PROXY from the environment \
             (http:// or socks5h:// style URLs).",
        ))
        .into()
}
