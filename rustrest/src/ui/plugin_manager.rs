//! "Manage Plugins" tab: lists every discovered plugin, lets the user
//! enable/disable it, and surfaces load errors so a broken plugin doesn't
//! just silently vanish.

use crate::app::Rustrest;
use crate::message::Message;
use crate::plugin_gallery::GalleryEntry;
use crate::ui::modal::{danger_text_color, muted_text_color};
use crate::ui::spinner::spinner_with_label;
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Theme};
use rustrest_command_palette::{Command, filter};
use rustrest_plugin_host::Capability;

/// reusable search box
fn search_bar<'a>(value: &'a str, placeholder: &'static str) -> Element<'a, Message> {
    text_input(placeholder, value)
        .on_input(Message::PluginManagerSearchChanged)
        .padding(8)
        .size(13)
        .into()
}

fn search_matches<T>(
    items: &[T],
    query: &str,
    text_for: impl Fn(&T) -> String,
    id_for: impl Fn(&T) -> String,
) -> Vec<usize> {
    let commands: Vec<Command<usize>> = items
        .iter()
        .enumerate()
        .map(|(idx, item)| Command::new(id_for(item), text_for(item), idx))
        .collect();
    filter(&commands, query)
        .into_iter()
        .map(|c| c.action)
        .collect()
}

/// short, user-facing label for a declared capability, shown as a badge in
/// the plugin manager so what a plugin can touch is visible before it's
/// even enabled
fn capability_label(capability: &Capability) -> &'static str {
    match capability {
        Capability::RequestHooks => "Request Hooks",
        Capability::Commands(_) => "Commands",
        Capability::MenuItems(_) => "Menu Items",
        Capability::SidebarPanel(_) => "Sidebar Panel",
        Capability::RightPanel(_) => "Right Panel",
        Capability::ImportFormat(_) => "Import Format",
        Capability::ExportFormat(_) => "Export Format",
        Capability::ExternalProcess => "External Process",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginManagerAction {
    Installing,
    Uninstalling(String),
    FetchingGallery,
    InstallingFromGallery(String),
}

/// which sub-view of the Manage Plugins tab is showing: the installed list
/// (with enable/disable/uninstall) or the remote gallery (browse/install).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PluginManagerView {
    #[default]
    Installed,
    Browse,
}

fn view_tab_button(
    label: &'static str,
    active: bool,
    message: Message,
) -> Element<'static, Message> {
    button(text(label).size(13))
        .on_press(message)
        .padding([6, 12])
        .style(if active {
            button::primary
        } else {
            button::secondary
        })
        .into()
}

pub fn render_plugin_manager_tab(app: &Rustrest) -> Element<'_, Message> {
    let title = text("Manage Plugins").size(28);
    let is_busy = app.plugin_manager_busy.is_some();

    let view_tabs = row![
        view_tab_button(
            "Installed",
            app.plugin_manager_view == PluginManagerView::Installed,
            Message::ShowPluginManagerView(PluginManagerView::Installed),
        ),
        view_tab_button(
            "Browse",
            app.plugin_manager_view == PluginManagerView::Browse,
            Message::ShowPluginManagerView(PluginManagerView::Browse),
        ),
    ]
    .spacing(6);

    let trailing_control: Element<'_, Message> = match app.plugin_manager_view {
        PluginManagerView::Installed => {
            if app.plugin_manager_busy == Some(PluginManagerAction::Installing) {
                spinner_with_label(app.spinner_tick, "Installing...")
            } else {
                button(text("Install Plugin Folder...").size(13))
                    .on_press_maybe((!is_busy).then_some(Message::InstallPluginPressed))
                    .padding([6, 12])
                    .style(button::secondary)
                    .into()
            }
        }
        PluginManagerView::Browse => {
            if app.plugin_manager_busy == Some(PluginManagerAction::FetchingGallery) {
                spinner_with_label(app.spinner_tick, "Fetching...")
            } else {
                button(text("Refresh").size(13))
                    .on_press_maybe((!is_busy).then_some(Message::FetchPluginGallery))
                    .padding([6, 12])
                    .style(button::secondary)
                    .into()
            }
        }
    };

    let header = row![
        title,
        container(view_tabs)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Center),
        trailing_control,
    ]
    .align_y(Alignment::Center);

    let search_placeholder = match app.plugin_manager_view {
        PluginManagerView::Installed => "Search installed plugins...",
        PluginManagerView::Browse => "Search gallery...",
    };
    let search = search_bar(&app.plugin_manager_search, search_placeholder);

    let body = match app.plugin_manager_view {
        PluginManagerView::Installed => render_installed_list(app, is_busy),
        PluginManagerView::Browse => render_gallery_list(app, is_busy),
    };

    column![header, search, scrollable(body).height(Length::Fill)]
        .spacing(20)
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn render_installed_list(app: &Rustrest, is_busy: bool) -> Element<'_, Message> {
    let installed = app.plugin_manager.installed();
    let mut list = column![].spacing(10);

    if installed.is_empty() {
        list = list.push(
            text(format!(
                "No plugins found in {}",
                app.plugin_manager.plugins_dir().display()
            ))
            .size(12)
            .style(|theme: &Theme| text::Style {
                color: Some(muted_text_color(theme)),
            }),
        );
        return list.into();
    }

    let matches = search_matches(
        installed,
        &app.plugin_manager_search,
        |p| match &p.manifest {
            Some(m) => format!("{} {} {}", m.name, m.author, m.description),
            None => p.dir_name.clone(),
        },
        |p| p.dir_name.clone(),
    );

    if matches.is_empty() {
        list = list.push(
            text("No plugins match your search.")
                .size(12)
                .style(|theme: &Theme| text::Style {
                    color: Some(muted_text_color(theme)),
                }),
        );
    }

    for &idx in &matches {
        let plugin = &installed[idx];
        let id = plugin.id().to_string();
        let uninstall_control: Element<'_, Message> = if app.plugin_manager_busy
            == Some(PluginManagerAction::Uninstalling(id.clone()))
        {
            spinner_with_label(app.spinner_tick, "Uninstalling...")
        } else {
            button(text("Uninstall").size(12))
                .on_press_maybe((!is_busy).then_some(Message::UninstallPluginPressed(id.clone())))
                .padding([4, 10])
                .style(button::danger)
                .into()
        };

        let info: Element<'_, Message> = match &plugin.manifest {
            Some(manifest) => {
                let meta_row = row![
                    checkbox(plugin.enabled)
                        .label(manifest.name.clone())
                        .on_toggle(move |enabled| {
                            Message::TogglePluginEnabled(id.clone(), enabled)
                        }),
                    text(format!("v{}", manifest.version))
                        .size(11)
                        .style(|theme: &Theme| text::Style {
                            color: Some(muted_text_color(theme)),
                        }),
                    text(format!("by {}", manifest.author))
                        .size(11)
                        .style(|theme: &Theme| text::Style {
                            color: Some(muted_text_color(theme)),
                        }),
                ]
                .spacing(8)
                .align_y(Alignment::Center);

                let mut badges = row![].spacing(6);
                for capability in &manifest.capabilities {
                    let is_external_process = matches!(capability, Capability::ExternalProcess);
                    badges = badges.push(
                        container(text(capability_label(capability)).size(10))
                            .padding([2, 6])
                            .style(move |theme: &Theme| {
                                let mut style = container::rounded_box(theme);
                                if is_external_process {
                                    style.text_color = Some(danger_text_color(theme));
                                } else {
                                    style.text_color = Some(muted_text_color(theme));
                                }
                                style
                            }),
                    );
                }

                column![
                    meta_row,
                    text(manifest.description.clone())
                        .size(11)
                        .style(|theme: &Theme| text::Style {
                            color: Some(muted_text_color(theme)),
                        }),
                    badges,
                ]
                .spacing(4)
                .into()
            }
            None => column![
                text(format!("{} (failed to load)", plugin.dir_name))
                    .size(13)
                    .style(|theme: &Theme| text::Style {
                        color: Some(danger_text_color(theme)),
                    }),
                text(plugin.load_error.clone().unwrap_or_default())
                    .size(11)
                    .style(|theme: &Theme| text::Style {
                        color: Some(danger_text_color(theme)),
                    }),
            ]
            .spacing(4)
            .into(),
        };

        let row_content = row![container(info).width(Length::Fill), uninstall_control]
            .spacing(10)
            .align_y(Alignment::Center);

        list = list.push(
            container(row_content)
                .padding(10)
                .width(Length::Fill)
                .style(container::bordered_box),
        );
    }

    list.into()
}

fn render_gallery_list(app: &Rustrest, is_busy: bool) -> Element<'_, Message> {
    let mut list = column![].spacing(10);

    match &app.plugin_gallery_entries {
        None => {
            list = list.push(
                text("Press Refresh to browse available plugins.")
                    .size(12)
                    .style(|theme: &Theme| text::Style {
                        color: Some(muted_text_color(theme)),
                    }),
            );
        }
        Some(Err(e)) => {
            list = list.push(
                text(format!("Failed to fetch plugin gallery: {e}"))
                    .size(12)
                    .style(|theme: &Theme| text::Style {
                        color: Some(danger_text_color(theme)),
                    }),
            );
        }
        Some(Ok(entries)) if entries.is_empty() => {
            list = list.push(
                text("No plugins are listed in the gallery yet.")
                    .size(12)
                    .style(|theme: &Theme| text::Style {
                        color: Some(muted_text_color(theme)),
                    }),
            );
        }
        Some(Ok(entries)) => {
            let already_installed: std::collections::HashSet<&str> = app
                .plugin_manager
                .installed()
                .iter()
                .map(|p| p.id())
                .collect();

            let matches = search_matches(
                entries,
                &app.plugin_manager_search,
                |e| format!("{} {} {}", e.name, e.author, e.description),
                |e| e.id.clone(),
            );

            if matches.is_empty() {
                list = list.push(text("No plugins match your search.").size(12).style(
                    |theme: &Theme| text::Style {
                        color: Some(muted_text_color(theme)),
                    },
                ));
            }

            for idx in matches {
                let entry = &entries[idx];
                list = list.push(render_gallery_entry(
                    app,
                    entry,
                    is_busy,
                    &already_installed,
                ));
            }
        }
    }

    list.into()
}

fn render_gallery_entry<'a>(
    app: &'a Rustrest,
    entry: &'a GalleryEntry,
    is_busy: bool,
    already_installed: &std::collections::HashSet<&str>,
) -> Element<'a, Message> {
    let is_installed = already_installed.contains(entry.id.as_str());

    let action_control: Element<'_, Message> = if app.plugin_manager_busy
        == Some(PluginManagerAction::InstallingFromGallery(entry.id.clone()))
    {
        spinner_with_label(app.spinner_tick, "Installing...")
    } else if is_installed {
        text("Installed")
            .size(12)
            .style(|theme: &Theme| text::Style {
                color: Some(muted_text_color(theme)),
            })
            .into()
    } else {
        button(text("Install").size(12))
            .on_press_maybe((!is_busy).then_some(Message::InstallFromGalleryPressed(entry.clone())))
            .padding([4, 10])
            .style(button::secondary)
            .into()
    };

    let meta_row = row![
        text(entry.name.clone()).size(14),
        text(format!("v{}", entry.version))
            .size(11)
            .style(|theme: &Theme| text::Style {
                color: Some(muted_text_color(theme)),
            }),
        text(format!("by {}", entry.author))
            .size(11)
            .style(|theme: &Theme| text::Style {
                color: Some(muted_text_color(theme)),
            }),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let info = column![
        meta_row,
        text(entry.description.clone())
            .size(11)
            .style(|theme: &Theme| text::Style {
                color: Some(muted_text_color(theme)),
            }),
    ]
    .spacing(4);

    let row_content = row![container(info).width(Length::Fill), action_control]
        .spacing(10)
        .align_y(Alignment::Center);

    container(row_content)
        .padding(10)
        .width(Length::Fill)
        .style(container::bordered_box)
        .into()
}
