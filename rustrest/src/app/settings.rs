//! Settings modal: theme, "close on outside click", and which Settings
//! sub-tab is active.

use super::Rustrest;
use crate::message::Message;
use crate::ui::settings::{AppTheme, SettingsTab};
use iced::Task;

#[derive(Default)]
pub struct SettingsState {
    pub settings_open: bool,
    pub settings_tab: SettingsTab,
    pub theme: AppTheme,
    /// whether clicking outside an open modal/command palette dismisses it
    pub close_on_outside_click: bool,
    /// whether form-data tables show the per-field Content-Type column
    pub show_form_data_content_type: bool,
}

pub(super) fn persist(app: &Rustrest) {
    crate::app_settings::save(&crate::app_settings::PersistedSettings {
        theme: app.settings.theme,
        close_on_outside_click: app.settings.close_on_outside_click,
        show_form_data_content_type: app.settings.show_form_data_content_type,
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

pub fn theme_selected(app: &mut Rustrest, theme: AppTheme) -> Task<Message> {
    app.settings.theme = theme;
    persist(app);
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
