//! Settings modal

use super::Rustrest;
use crate::http_client::ProxyConfig;
use crate::message::Message;
use crate::ui::settings::{ProxyEdit, SettingsTab};
use iced::Task;

#[derive(Default)]
pub struct SettingsState {
    pub settings_open: bool,
    pub settings_tab: SettingsTab,
    /// whether clicking outside an open modal/command palette dismisses it
    pub close_on_outside_click: bool,
    /// whether form-data tables show the per-field Content-Type column
    pub show_form_data_content_type: bool,
    /// app-wide proxy every request is sent through (Settings > Proxy)
    pub proxy: ProxyConfig,
    /// the port field's text, which may be empty or mid-edit
    pub proxy_port_input: String,
    /// whether HTTPS/WSS requests check the server certificate
    pub ssl_verification: bool,
}

pub(super) fn persist(app: &Rustrest) {
    crate::app_settings::save(&crate::app_settings::PersistedSettings {
        theme: app.theme.selection.clone(),
        theme_overrides: app.theme.overrides.clone(),
        close_on_outside_click: app.settings.close_on_outside_click,
        show_form_data_content_type: app.settings.show_form_data_content_type,
        proxy: app.settings.proxy.clone(),
        ssl_verification: app.settings.ssl_verification,
    });
}

pub fn open_pressed(app: &mut Rustrest) -> Task<Message> {
    app.settings.settings_open = true;
    Task::none()
}

pub fn close_pressed(app: &mut Rustrest) -> Task<Message> {
    app.settings.settings_open = false;
    Task::none()
}

pub fn tab_selected(app: &mut Rustrest, tab: SettingsTab) -> Task<Message> {
    app.settings.settings_tab = tab;
    Task::none()
}

pub fn form_data_content_type_toggled(app: &mut Rustrest) -> Task<Message> {
    app.settings.show_form_data_content_type = !app.settings.show_form_data_content_type;
    persist(app);
    Task::none()
}

pub fn close_on_outside_click_toggled(app: &mut Rustrest, enabled: bool) -> Task<Message> {
    app.settings.close_on_outside_click = enabled;
    persist(app);
    Task::none()
}

pub fn ssl_verification_toggled(app: &mut Rustrest, enabled: bool) -> Task<Message> {
    apply_ssl_verification(app, enabled);
    persist(app);
    Task::none()
}

/// Adopts the SSL verification setting, for every request sent from now on.
pub fn apply_ssl_verification(app: &mut Rustrest, enabled: bool) {
    app.settings.ssl_verification = enabled;
    crate::http_client::set_ssl_verification(enabled);
}

pub fn proxy_changed(app: &mut Rustrest, edit: ProxyEdit) -> Task<Message> {
    let proxy = &mut app.settings.proxy;
    match edit {
        ProxyEdit::Enabled(enabled) => proxy.enabled = enabled,
        ProxyEdit::Protocol(protocol) => proxy.protocol = protocol,
        ProxyEdit::Host(host) => proxy.host = host,
        ProxyEdit::Port(input) => {
            let input: String = input.chars().filter(char::is_ascii_digit).take(5).collect();
            if let Ok(port) = input.parse::<u16>() {
                proxy.port = port;
            }
            app.settings.proxy_port_input = input;
        }
        ProxyEdit::AuthEnabled(enabled) => proxy.auth_enabled = enabled,
        ProxyEdit::Username(username) => proxy.username = username,
        ProxyEdit::Password(password) => proxy.password = password,
        ProxyEdit::Bypass(bypass) => proxy.bypass = bypass,
        ProxyEdit::UseSystem(enabled) => proxy.use_system = enabled,
    }
    crate::http_client::set_proxy_config(app.settings.proxy.clone());
    persist(app);
    Task::none()
}

/// Adopts a proxy config loaded from settings.json.
pub fn apply_proxy(app: &mut Rustrest, proxy: ProxyConfig) {
    // leave a half-typed port alone when it's our own save echoing back
    if app.settings.proxy_port_input.parse::<u16>().ok() != Some(proxy.port) {
        app.settings.proxy_port_input = proxy.port.to_string();
    }
    crate::http_client::set_proxy_config(proxy.clone());
    app.settings.proxy = proxy;
}
