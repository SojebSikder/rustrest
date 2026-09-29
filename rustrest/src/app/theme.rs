//! Theme state: which themes exist, which one is selected (per the Zed-style `"theme"` setting),
//! the OS light/dark appearance, and the theme selector overlay with its live preview.

use super::Rustrest;
use crate::message::Message;
use crate::theme::{
    ActiveTheme, Appearance, ThemeMode, ThemeOverrides, ThemeRegistry, ThemeSelection, loader,
};
use crate::ui::toast::toast::ToastStatus;
use iced::Task;
use rustrest_command_palette::PaletteState;
use std::path::PathBuf;

pub struct ThemeState {
    pub registry: ThemeRegistry,
    pub selection: ThemeSelection,
    pub overrides: ThemeOverrides,
    /// OS appearance, for `"mode": "system"`.
    pub system: Appearance,
    /// Theme being previewed by the theme selector, shown instead of the
    /// selection until the selector is confirmed or dismissed.
    pub preview: Option<String>,
    pub active: ActiveTheme,
    pub selector: Option<PaletteState>,
    /// true between Ctrl+K and the key that completes (or cancels) a chord.
    pub chord_pending: bool,
}

impl ThemeState {
    pub fn new(
        selection: ThemeSelection,
        overrides: ThemeOverrides,
        extension_themes: &[(String, PathBuf)],
    ) -> Self {
        let registry = loader::load_registry(extension_themes);
        let mut state = Self {
            active: registry
                .get_or_default(
                    crate::theme::default_theme_name(Appearance::Dark),
                    Appearance::Dark,
                )
                .build(None),
            registry,
            selection,
            overrides,
            system: Appearance::Dark,
            preview: None,
            selector: None,
            chord_pending: false,
        };
        state.refresh();
        state
    }

    /// name of the theme that should be showing right now.
    pub fn current_name(&self) -> String {
        match &self.preview {
            Some(name) => name.clone(),
            None => self.selection.resolve(self.system).0.to_string(),
        }
    }

    /// re-resolves the active theme from the selection/preview, registry and overrides, and publishes it for style closures.
    pub fn refresh(&mut self) {
        let (name, fallback) = match &self.preview {
            Some(name) => (name.as_str(), self.system),
            None => self.selection.resolve(self.system),
        };
        let entry = self.registry.get_or_default(name, fallback);
        let active = entry.build(self.overrides.get(&entry.name));
        crate::theme::publish(&active);
        self.active = active;
    }
}

/// every theme file location, re-read from disk.
fn extension_themes(app: &Rustrest) -> Vec<(String, PathBuf)> {
    let manager = &app.plugins.plugin_manager;
    rustrest_plugin_host::PluginManager::scan_theme_files(
        manager.plugins_dir(),
        manager.state_path(),
    )
}

/// rescans every theme source (after a theme file, extension or setting changed) and re-applies the selection.
pub fn reload(app: &mut Rustrest) -> Vec<String> {
    app.theme.registry = loader::load_registry(&extension_themes(app));
    app.theme.refresh();
    app.theme.registry.errors.clone()
}

fn select(app: &mut Rustrest, name: &str) {
    app.theme.selection.select(name, app.theme.system);
    app.theme.preview = None;
    app.theme.refresh();
    super::settings::persist(app);
}

pub fn selected(app: &mut Rustrest, name: String) -> Task<Message> {
    select(app, &name);
    Task::none()
}

pub fn selected_for(app: &mut Rustrest, appearance: Appearance, name: String) -> Task<Message> {
    app.theme.selection.select_for(appearance, &name);
    app.theme.refresh();
    super::settings::persist(app);
    Task::none()
}

pub fn mode_selected(app: &mut Rustrest, mode: ThemeMode) -> Task<Message> {
    let registry = app.theme.registry.clone();
    app.theme.selection.set_mode(mode, &registry);
    app.theme.refresh();
    super::settings::persist(app);
    Task::none()
}

pub fn system_changed(app: &mut Rustrest, mode: iced::theme::Mode) -> Task<Message> {
    let system = match mode {
        iced::theme::Mode::Light => Appearance::Light,
        iced::theme::Mode::Dark => Appearance::Dark,
        // unknown: keep whatever we had
        iced::theme::Mode::None => return Task::none(),
    };
    if system != app.theme.system {
        app.theme.system = system;
        app.theme.refresh();
    }
    Task::none()
}

/// watcher saw theme files, an extension or settings.json change.
pub fn files_changed(app: &mut Rustrest, paths: Vec<PathBuf>) -> Task<Message> {
    let settings_changed = paths
        .iter()
        .any(|p| p.file_name().is_some_and(|n| n == "settings.json"));
    if settings_changed {
        // only while it parses: a half-typed edit shouldn't reset everything
        if let Some(settings) = crate::app_settings::try_load() {
            app.theme.selection = settings.theme;
            app.theme.overrides = settings.theme_overrides;
            app.settings.close_on_outside_click = settings.close_on_outside_click;
            app.settings.show_form_data_content_type = settings.show_form_data_content_type;
        }
    }

    let themes_changed = paths
        .iter()
        .any(|p| p.file_name().is_none_or(|n| n != "settings.json"));
    if themes_changed {
        let errors = reload(app);
        if let Some(first) = errors.first() {
            return Task::done(Message::ShowToast(
                format!("Failed to load theme: {first}"),
                ToastStatus::Error,
            ));
        }
    } else {
        app.theme.refresh();
    }
    Task::none()
}

pub fn reload_pressed(app: &mut Rustrest) -> Task<Message> {
    let errors = reload(app);
    let count = app.theme.registry.all().len();
    Task::done(match errors.first() {
        Some(first) => Message::ShowToast(
            format!("Reloaded {count} themes; failed to load: {first}"),
            ToastStatus::Error,
        ),
        None => Message::ShowToast(format!("Reloaded {count} themes"), ToastStatus::Success),
    })
}

pub fn open_folder_pressed(_app: &mut Rustrest) -> Task<Message> {
    let Some(dir) = loader::user_themes_dir() else {
        return Task::none();
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Task::done(Message::ShowToast(
            format!("Couldn't create {}: {e}", dir.display()),
            ToastStatus::Error,
        ));
    }
    crate::utils::open_in_file_manager(&dir);
    Task::none()
}

pub fn import_pressed(_app: &mut Rustrest) -> Task<Message> {
    Task::perform(
        async {
            let file = rfd::AsyncFileDialog::new()
                .add_filter("Theme", &["json"])
                .pick_file()
                .await?;
            Some(file.path().to_path_buf())
        },
        Message::ThemeFilePicked,
    )
}

pub fn file_picked(app: &mut Rustrest, path: Option<PathBuf>) -> Task<Message> {
    let Some(path) = path else {
        return Task::none();
    };
    match loader::import_file(&path) {
        Ok(names) => {
            reload(app);
            // like installing a theme in Zed: switch to it straight away
            if let Some(first) = names.first() {
                select(app, first);
            }
            Task::done(Message::ShowToast(
                format!("Imported {}", names.join(", ")),
                ToastStatus::Success,
            ))
        }
        Err(e) => Task::done(Message::ShowToast(
            format!("Failed to import theme: {e}"),
            ToastStatus::Error,
        )),
    }
}

// --- theme selector (Zed's `theme selector: toggle`, Ctrl+K Ctrl+T) ---

pub fn chord_started(app: &mut Rustrest) -> Task<Message> {
    app.theme.chord_pending = true;
    Task::none()
}

pub fn chord_cancelled(app: &mut Rustrest) -> Task<Message> {
    app.theme.chord_pending = false;
    Task::none()
}

pub fn toggle_selector(app: &mut Rustrest) -> Task<Message> {
    app.theme.chord_pending = false;
    if app.theme.selector.is_some() {
        return close_selector(app);
    }
    app.overlays.command_palette = None;
    let current = app.theme.current_name();
    let mut state = PaletteState::new();
    state.selected = crate::ui::theme_selector::matches_for(app, &state)
        .iter()
        .position(|c| c.action == current)
        .unwrap_or(0);
    app.theme.selector = Some(state);
    iced::widget::operation::focus(crate::ui::theme_selector::input_id())
}

/// previews whatever is now highlighted.
fn preview_selected(app: &mut Rustrest) {
    let Some(state) = &app.theme.selector else {
        return;
    };
    let matches = crate::ui::theme_selector::matches_for(app, state);
    if let Some(cmd) = matches.get(state.selected.min(matches.len().saturating_sub(1))) {
        app.theme.preview = Some(cmd.action.clone());
        app.theme.refresh();
    }
}

pub fn selector_query_changed(app: &mut Rustrest, query: String) -> Task<Message> {
    if let Some(state) = app.theme.selector.as_mut() {
        state.query = query;
        state.selected = 0;
    }
    preview_selected(app);
    Task::none()
}

pub fn selector_move(app: &mut Rustrest, delta: i32) -> Task<Message> {
    if let Some(mut state) = app.theme.selector.take() {
        let len = crate::ui::theme_selector::matches_for(app, &state).len();
        state.move_selection(delta, len);
        app.theme.selector = Some(state);
    }
    preview_selected(app);
    Task::none()
}

pub fn selector_confirm(app: &mut Rustrest) -> Task<Message> {
    let Some(state) = app.theme.selector.take() else {
        return Task::none();
    };
    let matches = crate::ui::theme_selector::matches_for(app, &state);
    match matches.get(state.selected) {
        Some(cmd) => select(app, &cmd.action.clone()),
        None => {
            app.theme.preview = None;
            app.theme.refresh();
        }
    }
    Task::none()
}

pub fn selector_item_clicked(app: &mut Rustrest, name: String) -> Task<Message> {
    app.theme.selector = None;
    select(app, &name);
    Task::none()
}

/// dismissed without choosing: back to the theme from before the preview.
pub fn close_selector(app: &mut Rustrest) -> Task<Message> {
    app.theme.selector = None;
    if app.theme.preview.take().is_some() {
        app.theme.refresh();
    }
    Task::none()
}
